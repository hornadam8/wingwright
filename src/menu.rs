//! Main menu.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContextPass, EguiContexts};

use crate::profile::{self, Profile};
use crate::world::Terrain;
use crate::{GameState, MainCamera, Toasts};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(OnEnter(GameState::MainMenu), enter_menu)
            .add_systems(Update, menu_camera.run_if(in_state(GameState::MainMenu)))
            .add_systems(EguiContextPass, menu_ui.run_if(in_state(GameState::MainMenu)));
    }
}

#[derive(Resource, Default)]
pub struct MenuState {
    confirm_new: Option<bool>, // Some(creative)
    angle: f32,
    show_trials: bool,
}

fn enter_menu(mut state: ResMut<MenuState>) {
    state.confirm_new = None;
    state.show_trials = false;
}

fn menu_camera(
    time: Res<Time>,
    terrain: Res<Terrain>,
    mut state: ResMut<MenuState>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
) {
    state.angle += time.delta_secs() * 0.06;
    let home = terrain.home();
    let center = home.center() + Vec3::new(0.0, 40.0, 0.0);
    let r = 520.0;
    let pos = center + Vec3::new(state.angle.cos() * r, 220.0, state.angle.sin() * r);
    if let Ok(mut t) = cam.single_mut() {
        *t = Transform::from_translation(pos).looking_at(center, Vec3::Y);
    }
}

fn menu_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut state: ResMut<MenuState>,
    mut next: ResMut<NextState<GameState>>,
    mut toasts: ResMut<Toasts>,
    mut exit: EventWriter<AppExit>,
    terrain: Res<Terrain>,
) {
    let ctx = contexts.ctx_mut();
    let has_profile = profile::profile_exists();
    egui::Area::new(egui::Id::new("title"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 70.0))
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                // Painted galleys never wrap (an anchored Area remembers its first-frame width).
                crate::ui::shadow_label(ui, "WINGWRIGHT", 68.0, egui::Color32::WHITE);
                crate::ui::shadow_label(ui, "Build a plane. Haul the cargo. Try not to crash.", 18.0, egui::Color32::from_gray(245));
            });
        });
    egui::Window::new("menu")
        .title_bar(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 40.0))
        .show(ctx, |ui| {
            ui.set_width(320.0);
            ui.vertical_centered_justified(|ui| {
                ui.add_space(6.0);
                if let Some(creative) = state.confirm_new {
                    ui.label("Starting a new game overwrites your existing save.");
                    ui.add_space(6.0);
                    if ui.button("Yes, start over").clicked() {
                        start_new(&mut commands, &mut next, creative, &mut toasts);
                        state.confirm_new = None;
                    }
                    if ui.button("Cancel").clicked() {
                        state.confirm_new = None;
                    }
                    return;
                }
                if has_profile && ui.add(egui::Button::new(egui::RichText::new("Continue").size(20.0)).min_size(egui::vec2(0.0, 40.0))).clicked() {
                    match profile::load_profile() {
                        Some(p) => {
                            commands.insert_resource(p);
                            next.set(GameState::Hangar);
                        }
                        None => toasts.push("Could not load the save file."),
                    }
                }
                if ui.add(egui::Button::new(egui::RichText::new("New Campaign").size(20.0)).min_size(egui::vec2(0.0, 40.0))).clicked() {
                    if has_profile {
                        state.confirm_new = Some(false);
                    } else {
                        start_new(&mut commands, &mut next, false, &mut toasts);
                    }
                }
                if ui.add(egui::Button::new(egui::RichText::new("Creative Mode").size(20.0)).min_size(egui::vec2(0.0, 40.0))).clicked() {
                    if has_profile {
                        state.confirm_new = Some(true);
                    } else {
                        start_new(&mut commands, &mut next, true, &mut toasts);
                    }
                }
                if ui.add(egui::Button::new(egui::RichText::new("Test Pilot").size(20.0)).min_size(egui::vec2(0.0, 40.0))).clicked() {
                    state.show_trials = !state.show_trials;
                }
                ui.add_space(10.0);
                if ui.add(egui::Button::new(egui::RichText::new("Quit").size(16.0)).min_size(egui::vec2(0.0, 32.0))).clicked() {
                    exit.write(AppExit::Success);
                }
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Campaign: earn money and research points by flying cargo missions.\nCreative: every part unlocked, unlimited money.\nTest Pilot: time trials with a budget and a tech ceiling; your times tune the mission timers.").size(12.0).color(egui::Color32::from_gray(110)));
            });
        });
    if state.show_trials {
        trials_window(ctx, &mut commands, &mut next, &mut toasts, &terrain);
    }
}

/// Test Pilot: one trial per timed mission, best time so far, a Fly button.
fn trials_window(ctx: &egui::Context, commands: &mut Commands, next: &mut NextState<GameState>, toasts: &mut Toasts, terrain: &Terrain) {
    let results = profile::trial_results();
    egui::Window::new("Test Pilot").anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 40.0)).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.set_width(760.0);
        ui.label(egui::RichText::new("Each trial gives you the money and research points the campaign has paid out before that job (starter parts only: research what you need). Build, load the cargo at the pickup (the clock starts then) and deliver it all. Times go to trials.json in the save folder.").size(12.0).color(egui::Color32::from_gray(110)));
        ui.add_space(6.0);
        egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
            for m in crate::missions::timed() {
                let (budget, rp) = Profile::trial_budget(m);
                let from = terrain.airport(m.from);
                let to = terrain.airport(m.to);
                let km = (from.def.pos - to.def.pos).length() / 1000.0;
                let best = results.get(m.id).and_then(|v| v.iter().min_by(|a, b| a.secs.partial_cmp(&b.secs).unwrap()));
                egui::Frame::new().fill(egui::Color32::from_rgb(236, 237, 242)).corner_radius(8.0).inner_margin(8.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(m.title).strong().size(15.0));
                            ui.label(egui::RichText::new(format!("{} x {} | {} -> {} ({km:.1} km) | ${} and {} RP | current timer {}", m.count, m.cargo.name(), from.name(), to.name(), budget, rp, m.time_limit.map_or("none".to_string(), |s| crate::flight::fmt_time_tenths(s)))).size(12.0).color(egui::Color32::from_gray(90)));
                            match best {
                                Some(b) => ui.label(egui::RichText::new(format!("Best {}  (${} plane: {})", crate::flight::fmt_time_tenths(b.secs as f32), b.cost, b.parts)).size(12.0).color(egui::Color32::from_rgb(40, 120, 60))),
                                None => ui.label(egui::RichText::new("No time yet").size(12.0).color(egui::Color32::from_gray(150))),
                            };
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(crate::ui::pill_btn("Fly")).clicked() {
                                commands.insert_resource(Profile::new_trial(m));
                                next.set(GameState::Hangar);
                                toasts.push(format!("Test Pilot: {} with ${} and {} RP. Load at {}; the clock starts on loading.", m.title, budget, rp, from.name()));
                            }
                        });
                    });
                });
            }
        });
    });
}

fn start_new(commands: &mut Commands, next: &mut NextState<GameState>, creative: bool, toasts: &mut Toasts) {
    let p = if creative { Profile::new_creative() } else { Profile::new_campaign() };
    profile::save_profile(&p);
    commands.insert_resource(p);
    next.set(GameState::Hangar);
    if creative {
        toasts.push("Creative mode: all parts unlocked.");
    } else {
        toasts.push("Welcome to Harbor Field. Build a plane and take the first mission.");
    }
}
