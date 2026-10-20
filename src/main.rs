//! Wingwright: a build-and-fly cargo game.

mod autotest;
mod builder;
mod design;
mod flight;
mod icons;
mod menu;
mod meshes;
mod missions;
mod partvis;
mod parts;
mod profile;
mod settle;
mod ui;
mod world;

use bevy::core_pipeline::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::{DirectionalLightShadowMap, DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode, WindowResolution};
use bevy_egui::{egui, EguiContextPass, EguiContextSettings, EguiContexts, EguiPlugin};
use std::collections::HashMap;

#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    MainMenu,
    Hangar,
    Flight,
}

#[derive(Component)]
pub struct MainCamera;

/// Materials shared by color so parts do not each allocate one.
#[derive(Resource, Default)]
pub struct MaterialCache {
    map: HashMap<[u8; 3], Handle<StandardMaterial>>,
}

impl MaterialCache {
    pub fn get(&mut self, materials: &mut Assets<StandardMaterial>, rgb: [f32; 3]) -> Handle<StandardMaterial> {
        let key = [(rgb[0] * 255.0) as u8, (rgb[1] * 255.0) as u8, (rgb[2] * 255.0) as u8];
        self.map
            .entry(key)
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: Color::srgb(rgb[0], rgb[1], rgb[2]),
                    perceptual_roughness: 0.45,
                    metallic: 0.02,
                    reflectance: 0.45,
                    ..default()
                })
            })
            .clone()
    }
}

/// Short on-screen notifications.
#[derive(Resource, Default)]
pub struct Toasts {
    pub items: Vec<(String, f32)>,
}

impl Toasts {
    pub fn push(&mut self, s: impl Into<String>) {
        let s = s.into();
        info!("toast: {s}");
        self.items.push((s, 5.0));
        if self.items.len() > 6 {
            self.items.remove(0);
        }
    }
}

/// Whether egui currently owns the pointer / keyboard (so world input should be ignored).
#[derive(Resource, Default)]
pub struct UiFocus {
    pub pointer: bool,
    pub keyboard: bool,
}

/// Story radio messages waiting to be read ("Click to Continue").
#[derive(Resource, Default)]
pub struct Radio {
    pub queue: Vec<String>,
}

impl Radio {
    pub fn say(&mut self, s: impl Into<String>) {
        self.queue.push(s.into());
    }
}

fn radio_ui(mut contexts: EguiContexts, mut radio: ResMut<Radio>, keys: Res<ButtonInput<KeyCode>>) {
    if radio.queue.is_empty() {
        return;
    }
    let ctx = contexts.ctx_mut();
    let mut dismiss = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space);
    egui::Area::new(egui::Id::new("radio")).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 24.0)).show(ctx, |ui| {
        let resp = ui::card(ui, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(28.0, 40.0), egui::Sense::hover());
                let p = ui.painter();
                p.rect_filled(egui::Rect::from_center_size(r.center() + egui::vec2(0.0, 6.0), egui::vec2(16.0, 26.0)), 4.0, ui::TEXT);
                p.line_segment([r.center() + egui::vec2(4.0, -8.0), r.center() + egui::vec2(4.0, -20.0)], egui::Stroke::new(2.0_f32, ui::TEXT));
                ui.label(egui::RichText::new(&radio.queue[0]).size(17.0).strong().color(ui::TEXT));
            });
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("( Click to Continue )").size(12.0).italics().color(egui::Color32::from_gray(120)));
            });
            ui.interact(ui.min_rect(), egui::Id::new("radio-click"), egui::Sense::click())
        });
        if resp.clicked() {
            dismiss = true;
        }
    });
    if dismiss {
        radio.queue.remove(0);
    }
}

/// Lazily created egui texture of the island map.
#[derive(Resource, Default)]
pub struct MapTexture(pub Option<egui::TextureHandle>);

impl MapTexture {
    pub fn get(&mut self, ctx: &egui::Context, terrain: &world::Terrain) -> egui::TextureHandle {
        if let Some(t) = &self.0 {
            return t.clone();
        }
        const S: usize = 512;
        let px = terrain.map_pixels(S);
        let pixels: Vec<egui::Color32> = px.iter().map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])).collect();
        let image = egui::ColorImage { size: [S, S], pixels };
        let tex = ctx.load_texture("island-map", image, egui::TextureOptions::LINEAR);
        self.0 = Some(tex.clone());
        tex
    }
}

/// `WINGWRIGHT_WINDOW=1920x1080` forces a windowed size (tests check 16:9 layouts this way).
fn window_override() -> Option<(f32, f32)> {
    let s = std::env::var("WINGWRIGHT_WINDOW").ok()?;
    let (w, h) = s.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

fn main() {
    let terrain = world::Terrain::generate();
    let forced = window_override();
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Wingwright".into(),
                        resolution: match forced {
                            Some((w, h)) => WindowResolution::new(w, h),
                            None => WindowResolution::new(1440.0, 900.0),
                        },
                        // Vsync by default; WINGWRIGHT_NOVSYNC=1 uncaps the frame rate
                        // (used to measure the real frame cost and for high-refresh play).
                        present_mode: if std::env::var_os("WINGWRIGHT_NOVSYNC").is_some() { PresentMode::AutoNoVsync } else { PresentMode::AutoVsync },
                        // Scripted test runs take the whole screen so screenshots match play.
                        mode: if std::env::var_os("WINGWRIGHT_AUTOTEST").is_some() && forced.is_none() {
                            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
                        } else {
                            WindowMode::Windowed
                        },
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins(EguiPlugin { enable_multipass_for_primary_context: true })
        .insert_resource(terrain)
        // Crisper tree and building shadows over the long cascade range.
        .insert_resource(DirectionalLightShadowMap { size: 4096 })
        .init_state::<GameState>()
        .init_resource::<MaterialCache>()
        .init_resource::<Toasts>()
        .init_resource::<UiFocus>()
        .init_resource::<MapTexture>()
        .init_resource::<Radio>()
        .add_systems(Startup, (world::spawn_world, setup_camera))
        .add_plugins((menu::MenuPlugin, builder::BuilderPlugin, flight::FlightPlugin, icons::IconsPlugin, autotest::AutotestPlugin))
        .add_systems(EguiContextPass, (egui_style, toasts_ui, radio_ui, ui_focus).chain())
        .add_systems(Update, (tick_toasts, world::sky_follow, world::water_shimmer, world::terrain_lod, ui_scale))
        .add_systems(Last, exit_watchdog)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        // HDR so the sun disc can glow through the bloom pass; everything else stays under
        // the bloom threshold, so the scene itself does not get hazy.
        Camera { hdr: true, ..default() },
        Msaa::Sample4,
        Tonemapping::TonyMcMapface,
        Bloom {
            intensity: 0.3,
            composite_mode: BloomCompositeMode::Additive,
            prefilter: BloomPrefilter { threshold: 4.0, threshold_softness: 0.3 },
            ..Bloom::NATURAL
        },
        Projection::Perspective(PerspectiveProjection { fov: 60f32.to_radians(), near: 0.3, far: 30000.0, ..default() }),
        Transform::from_xyz(0.0, 300.0, 800.0).looking_at(Vec3::ZERO, Vec3::Y),
        // Thick pale haze fairly close in, like the reference game's islands fading out.
        DistanceFog {
            color: Color::srgb(world::HAZE[0], world::HAZE[1], world::HAZE[2]),
            falloff: FogFalloff::Linear { start: 900.0, end: 6500.0 },
            ..default()
        },
        MainCamera,
    ));
}

/// Shutting down the winit event loop hangs on macOS when the window is not fullscreen
/// (observed with Bevy 0.16.1: the last frame renders, then the process never exits).
/// Once an exit has been requested, give the normal path a moment and then leave.
fn exit_watchdog(mut events: EventReader<AppExit>, mut armed: Local<bool>) {
    if *armed || events.read().next().is_none() {
        return;
    }
    *armed = true;
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        std::process::exit(0);
    });
}

/// Scale the whole egui UI with the window height so it reads the same on any
/// monitor (about 2x on a 1440 px tall screen, 1.25x in the default 1440x900 window).
/// `WINGWRIGHT_UI_SCALE=1.5` overrides it.
fn ui_scale(mut q: Query<(&mut EguiContextSettings, &Window), With<PrimaryWindow>>) {
    let Ok((mut settings, window)) = q.single_mut() else { return };
    let scale = match std::env::var("WINGWRIGHT_UI_SCALE").ok().and_then(|s| s.parse::<f32>().ok()) {
        Some(s) => s.clamp(0.5, 4.0),
        None => (window.height() / 720.0).clamp(1.0, 2.4),
    };
    if (settings.scale_factor - scale).abs() > 0.01 {
        settings.scale_factor = scale;
    }
}

fn egui_style(mut contexts: EguiContexts, mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;
    let ctx = contexts.ctx_mut();
    // A rounded bold system font, if present, for the rounded look.
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/Supplemental/Arial Rounded Bold.ttf") {
        fonts.font_data.insert("rounded".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "rounded".to_owned());
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    use egui::FontFamily::{Monospace, Proportional};
    use egui::{FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Heading, FontId::new(20.0, Proportional)),
        (TextStyle::Body, FontId::new(14.5, Proportional)),
        (TextStyle::Monospace, FontId::new(13.5, Monospace)),
        (TextStyle::Button, FontId::new(14.5, Proportional)),
        (TextStyle::Small, FontId::new(12.0, Proportional)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 5.0);
    let mut visuals = egui::Visuals::light();
    visuals.window_fill = ui::CARD;
    visuals.panel_fill = ui::CARD;
    visuals.window_corner_radius = egui::CornerRadius::same(14);
    visuals.window_stroke = egui::Stroke::NONE;
    visuals.window_shadow = egui::Shadow { offset: [0, 3], blur: 14, spread: 0, color: egui::Color32::from_black_alpha(50) };
    visuals.override_text_color = Some(ui::TEXT);
    for w in [&mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active, &mut visuals.widgets.open, &mut visuals.widgets.noninteractive] {
        w.corner_radius = egui::CornerRadius::same(10);
        w.fg_stroke.color = ui::TEXT;
    }
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(236, 237, 242);
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(236, 237, 242);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(222, 224, 232);
    visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(222, 224, 232);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(200, 206, 220);
    visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(200, 206, 220);
    visuals.selection.bg_fill = egui::Color32::from_rgb(70, 130, 230);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    visuals.extreme_bg_color = egui::Color32::from_rgb(246, 246, 249);
    style.visuals = visuals;
    ctx.set_style(style);
}

fn toasts_ui(mut contexts: EguiContexts, toasts: Res<Toasts>) {
    if toasts.items.is_empty() {
        return;
    }
    let ctx = contexts.ctx_mut();
    // Banner cards under the top centre, like the reference game's "Low Fuel" strip.
    egui::Area::new(egui::Id::new("toasts"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 58.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                for (text, ttl) in toasts.items.iter() {
                    let alpha = (ttl.min(1.0) * 235.0) as u8;
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(250, 250, 252, alpha))
                        .corner_radius(10.0)
                        .inner_margin(egui::Margin::symmetric(16, 8))
                        .shadow(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: egui::Color32::from_black_alpha(alpha / 6) })
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.label(egui::RichText::new(text).strong().color(egui::Color32::from_rgba_unmultiplied(35, 38, 48, alpha)));
                        });
                }
            });
        });
}

fn tick_toasts(time: Res<Time>, mut toasts: ResMut<Toasts>) {
    let dt = time.delta_secs();
    for t in toasts.items.iter_mut() {
        t.1 -= dt;
    }
    toasts.items.retain(|t| t.1 > 0.0);
}

fn ui_focus(mut contexts: EguiContexts, mut focus: ResMut<UiFocus>) {
    let ctx = contexts.ctx_mut();
    focus.pointer = ctx.is_pointer_over_area() || ctx.wants_pointer_input();
    focus.keyboard = ctx.wants_keyboard_input();
}
