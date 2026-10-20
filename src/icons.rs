//! Rendered part thumbnails for the palette tiles.
//!
//! A hidden orthographic camera on its own render layer draws one part per frame into a
//! small texture; egui then shows those textures on the tiles as part
//! pictures. Everything happens in the first second or so after launch, while the main
//! menu is up, and the textures live for the rest of the session.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::camera::ScalingMode;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::RenderLayers;
use bevy_egui::{egui, EguiUserTextures};
use std::collections::HashMap;

use crate::design::PlacedPart;
use crate::parts::{PartKind, PARTS};
use crate::{partvis, MaterialCache};

const LAYER: usize = 2;
/// Texture size in pixels. Tiles show it at about 64 points, so this stays crisp at 2x.
pub const ICON_PX: u32 = 192;

#[derive(Component)]
struct IconCamera;

#[derive(Resource, Default)]
pub struct PartIcons {
    ready: HashMap<&'static str, egui::TextureId>,
    /// Keeps the textures alive.
    handles: Vec<Handle<Image>>,
    next: usize,
    camera: Option<Entity>,
    /// The part visual rendered last frame; removed before the next one is spawned.
    subject: Option<Entity>,
    done: bool,
}

impl PartIcons {
    pub fn get(&self, id: &str) -> Option<egui::TextureId> {
        self.ready.get(id).copied()
    }
}

pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartIcons>().add_systems(Startup, setup).add_systems(Update, render_next);
    }
}

fn setup(mut commands: Commands, mut icons: ResMut<PartIcons>) {
    let layers = RenderLayers::layer(LAYER);
    let cam = commands
        .spawn((
            Camera3d::default(),
            Camera { order: -5, is_active: false, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            Msaa::Sample4,
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical { viewport_height: 2.0 },
                near: -100.0,
                far: 100.0,
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(3.0, 2.2, -3.6).looking_at(Vec3::ZERO, Vec3::Y),
            layers.clone(),
            IconCamera,
        ))
        .id();
    // Lights are per layer, so the icon layer needs its own sun (roughly the hangar's).
    commands.spawn((
        DirectionalLight { illuminance: 9_000.0, shadows_enabled: false, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.9, -0.85, 0.0)),
        layers,
    ));
    icons.camera = Some(cam);
}

/// Direction the icon camera looks from: front, right and above, like the palette
/// pictures (the nose points to -Z).
const VIEW_DIR: Vec3 = Vec3::new(0.58, 0.62, -0.58);

#[allow(clippy::too_many_arguments)]
fn render_next(
    mut commands: Commands,
    mut icons: ResMut<PartIcons>,
    mut images: ResMut<Assets<Image>>,
    mut egui_textures: ResMut<EguiUserTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<MaterialCache>,
    mut cams: Query<(&mut Camera, &mut Transform, &mut Projection), With<IconCamera>>,
) {
    if icons.done {
        return;
    }
    let Some(cam_entity) = icons.camera else { return };
    let Ok((mut cam, mut cam_tf, mut proj)) = cams.get_mut(cam_entity) else { return };
    // Last frame's subject has been drawn into its own texture; clear the stage.
    if let Some(e) = icons.subject.take() {
        commands.entity(e).despawn();
    }
    let Some(def) = PARTS.get(icons.next) else {
        icons.done = true;
        cam.is_active = false;
        info!("icons: rendered {} part icons", icons.ready.len());
        return;
    };
    icons.next += 1;

    // A fresh target texture for this part.
    let size = Extent3d { width: ICON_PX, height: ICON_PX, depth_or_array_layers: 1 };
    let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let handle = images.add(image);
    let tex_id = egui_textures.add_image(handle.clone());
    icons.handles.push(handle.clone());
    icons.ready.insert(def.id, tex_id);
    cam.target = bevy::render::camera::RenderTarget::Image(handle.into());
    cam.is_active = true;

    // Spawn the part at the origin in its default orientation and colour.
    let placed = PlacedPart::new(def, Vec3::ZERO, Quat::IDENTITY, false);
    let layers = RenderLayers::layer(LAYER);
    let subject = partvis::spawn_part_visual(&mut commands, &mut meshes, &mut materials, &mut cache, &placed, 0, &layers);
    icons.subject = Some(subject);

    // Frame the part's box exactly: project its corners into the camera's view plane.
    let dir = VIEW_DIR.normalize();
    *cam_tf = Transform::from_translation(dir * 20.0).looking_at(Vec3::ZERO, Vec3::Y);
    let right = cam_tf.right().as_vec3();
    let up = cam_tf.up().as_vec3();
    let mut h = placed.dims() * 0.5;
    if matches!(def.kind, PartKind::Engine { prop: true, .. }) {
        // The propeller disc is wider than the engine's box (see partvis blade_len).
        let tip = (h.x.min(h.y) * 2.7).max(0.9);
        h.x = h.x.max(tip);
        h.y = h.y.max(tip);
    }
    let mut ext = 0.05_f32;
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let c = Vec3::new(sx * h.x, sy * h.y, sz * h.z);
                ext = ext.max(c.dot(right).abs()).max(c.dot(up).abs());
            }
        }
    }
    if let Projection::Orthographic(o) = &mut *proj {
        o.scaling_mode = ScalingMode::FixedVertical { viewport_height: ext * 2.0 * 1.08 };
    }
}
