//! The builder: a plane floating over a grid in an indigo void, with drag-and-drop parts,
//! research, missions and design management.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::pbr::{CascadeShadowConfigBuilder, DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::camera::ClearColorConfig;
use bevy::render::view::RenderLayers;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContextPass, EguiContexts};
use std::f32::consts::FRAC_PI_2;

use crate::design::{self, snap, DesignStats, PlacedPart, PlaneDesign};
use crate::parts::{self, Category, Dir, PartDef, PartKind, PALETTE, PARTS};
use crate::partvis::{self, PartIndex};
use crate::profile::{self, Profile};
use crate::ui::{self, card, pill, pill_button, toggle_button};
use crate::world::Terrain;
use crate::{GameState, MainCamera, MapTexture, MaterialCache, Toasts, UiFocus};

pub struct BuilderPlugin;

impl Plugin for BuilderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Builder>()
            .add_systems(OnEnter(GameState::Hangar), enter_hangar)
            .add_systems(OnExit(GameState::Hangar), exit_hangar)
            .add_systems(
                Update,
                (builder_camera, builder_interact, sync_ghost_preview, tint_ghost_preview, builder_sync, builder_xray, builder_gizmo, builder_drag_overlay).chain().run_if(in_state(GameState::Hangar)),
            )
            .add_systems(EguiContextPass, builder_ui.run_if(in_state(GameState::Hangar)));
    }
}

const LAYER: usize = 1;
/// Markers (centre of mass, centre of lift, hover dot) render through a second camera on
/// their own layer so they show through the plane instead of getting buried in it.
const OVERLAY_LAYER: usize = 3;
const INDIGO: Color = Color::srgb(0.22, 0.28, 0.55);

pub struct Ghost {
    def: &'static PartDef,
    k: i32,
    candidate: Option<PlacedPart>,
    mirror_candidate: Option<PlacedPart>,
    valid: bool,
    reason: String,
}

#[derive(Resource)]
pub struct Builder {
    pub design: PlaneDesign,
    pub stats: DesignStats,
    pub selected: Option<usize>,
    ghost: Option<Ghost>,
    pub mirror: bool,
    category: Category,
    design_name: String,
    designs: Vec<String>,
    dirty: bool,
    hover: Option<(usize, Dir, Vec3)>,
    cam_yaw: f32,
    cam_pitch: f32,
    cam_dist: f32,
    root_pos: Vec3,
    root: Option<Entity>,
    history: Vec<(PlaneDesign, i64)>,
    /// Test hook: a hover forced by the autotest instead of the mouse (sticky until cleared).
    pub debug_hover: Option<(usize, Dir, Vec3)>,
    drag: Option<Drag>,
    /// Left button went down on empty space: dragging orbits the camera until release.
    orbit_left: bool,
    pub xray: bool,
    pub show_drag: bool,
    drag_overlay_applied: bool,
    show_cg: bool,
    show_stats: bool,
    show_research: bool,
    show_missions: bool,
    show_designs: bool,
    show_help: bool,
    launch: bool,
    go_menu: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            design: PlaneDesign::starter(),
            stats: DesignStats::default(),
            selected: None,
            ghost: None,
            mirror: true,
            category: Category::Wings,
            design_name: String::new(),
            designs: Vec::new(),
            dirty: true,
            hover: None,
            cam_yaw: 0.7,
            cam_pitch: 0.3,
            cam_dist: 11.0,
            root_pos: Vec3::ZERO,
            root: None,
            history: Vec::new(),
            drag: None,
            debug_hover: None,
            orbit_left: false,
            xray: false,
            show_drag: false,
            drag_overlay_applied: false,
            show_cg: true,
            show_stats: false,
            show_research: false,
            show_missions: false,
            show_designs: false,
            show_help: false,
            launch: false,
            go_menu: false,
        }
    }
}

/// The big hangar windows. Only one is open at a time.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Research,
    Missions,
    Designs,
    Help,
}

impl Builder {
    fn panel_flag(&mut self, p: Panel) -> &mut bool {
        match p {
            Panel::Research => &mut self.show_research,
            Panel::Missions => &mut self.show_missions,
            Panel::Designs => &mut self.show_designs,
            Panel::Help => &mut self.show_help,
        }
    }

    /// Open `p` and close the other big windows.
    pub fn open_panel(&mut self, p: Panel) {
        for other in [Panel::Research, Panel::Missions, Panel::Designs, Panel::Help] {
            *self.panel_flag(other) = other == p;
        }
    }

    /// Close `p` if it is open, otherwise open it (closing the others).
    pub fn toggle_panel(&mut self, p: Panel) {
        if *self.panel_flag(p) {
            *self.panel_flag(p) = false;
        } else {
            self.open_panel(p);
        }
    }

    /// Test hook: hold `id` over `host`'s `face` (plus `offset`) without placing it, so the
    /// placement preview can be inspected. An empty id clears the hover and the ghost.
    pub fn debug_hover(&mut self, id: &str, host: usize, face: Dir, offset: Vec3) {
        if id.is_empty() {
            self.debug_hover = None;
            self.ghost = None;
            return;
        }
        let Some(def) = parts::part(id) else { return };
        if host >= self.design.parts.len() {
            return;
        }
        let h = &self.design.parts[host];
        let n = face.vec();
        let he = h.half_extents();
        let hit = h.pos() + Vec3::new(n.x * he.x, n.y * he.y, n.z * he.z) + offset;
        self.ghost = Some(Ghost { def, k: 0, candidate: None, mirror_candidate: None, valid: false, reason: String::new() });
        self.debug_hover = Some((host, face, hit));
    }

    /// Test hook: open one of the builder windows or toggle an overlay by name.
    pub fn debug_set_tab(&mut self, name: &str) {
        match name {
            "research" => self.open_panel(Panel::Research),
            "missions" => self.open_panel(Panel::Missions),
            "designs" => self.open_panel(Panel::Designs),
            "stats" => self.show_stats = true,
            "cg" => self.show_cg = !self.show_cg,
            "drag" => {
                self.show_drag = !self.show_drag;
                self.dirty = true;
            }
            "xray" => self.xray = !self.xray,
            "body" => self.category = Category::Fuselage,
            "wings" => self.category = Category::Wings,
            "control" => self.category = Category::Control,
            "engines" => self.category = Category::Engines,
            "gear" => self.category = Category::Gear,
            "fuel" => self.category = Category::Fuel,
            "extras" => self.category = Category::Utility,
            "cockpit" => self.category = Category::Cockpit,
            _ => {}
        }
    }

    /// Test hook: reset to a bare cockpit, refunding everything.
    pub fn debug_clear(&mut self, profile: &mut Profile, cockpit: Option<&str>) {
        let refund = self.design.total_cost();
        profile.refund(refund);
        self.design = match cockpit {
            Some(id) => PlaneDesign::empty_with(id),
            None => PlaneDesign::empty(),
        };
        if let Some(c) = self.design.parts.first() {
            profile.spend(c.def().cost);
        }
        self.design_name = self.design.name.clone();
        self.selected = None;
        self.ghost = None;
        self.dirty = true;
    }

    /// Test hook: place `id` against face `face` of part `host`, pointing at the face center
    /// plus `offset`, through the same snapping and purchase logic as a mouse click.
    pub fn debug_place(&mut self, profile: &mut Profile, id: &str, host: usize, face: Dir, offset: Vec3, k: usize) -> Result<String, String> {
        let def = parts::part(id).ok_or_else(|| format!("unknown part {id}"))?;
        if host >= self.design.parts.len() {
            return Err(format!("no part {host}"));
        }
        let h = &self.design.parts[host];
        let n = face.vec();
        let he = h.half_extents();
        let hit = h.pos() + Vec3::new(n.x * he.x, n.y * he.y, n.z * he.z) + offset;
        self.ghost = Some(Ghost { def, k: k as i32, candidate: None, mirror_candidate: None, valid: false, reason: String::new() });
        self.hover = Some((host, face, hit));
        compute_placement(self, profile);
        let g = self.ghost.take().unwrap();
        self.hover = None;
        if !g.valid {
            return Err(g.reason);
        }
        let c = g.candidate.unwrap();
        let mut cost = c.cost();
        let pos = c.pos();
        let mut n_placed = 1;
        self.design.parts.push(c);
        if let Some(m) = g.mirror_candidate {
            cost += m.cost();
            self.design.parts.push(m);
            n_placed = 2;
        }
        profile.spend(cost);
        self.dirty = true;
        Ok(format!("placed {n_placed} x {id} at {pos:?} for ${cost}, ${} left", profile.money))
    }

    fn snapshot(&mut self, profile: &Profile) {
        self.history.push((self.design.clone(), profile.money));
        if self.history.len() > 60 {
            self.history.remove(0);
        }
    }
}

#[derive(Component)]
struct BuilderEntity;

#[derive(Component)]
struct BuilderRoot;

#[derive(Component)]
struct BuilderCamera;

#[derive(Component)]
struct GhostEntity;

#[derive(Component)]
struct HoverMarker;

#[derive(Component)]
struct MarkerCom;

#[derive(Component)]
struct MarkerCol;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn vec(self) -> Vec3 {
        match self {
            Axis::X => Vec3::X,
            Axis::Y => Vec3::Y,
            Axis::Z => Vec3::Z,
        }
    }
}

#[derive(Component)]
#[allow(dead_code)]
struct GizmoHandle(Axis);

/// An in-progress gizmo drag: which axis and where along it the drag started.
struct Drag {
    axis: Axis,
    start_t: f32,
    start_pos: Vec3,
    twin: Option<usize>,
    twin_start: Vec3,
}

#[derive(Component)]
struct GhostMaterials {
    ok: Handle<StandardMaterial>,
    bad: Handle<StandardMaterial>,
}

fn enter_hangar(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut b: ResMut<Builder>,
    profile: Res<Profile>,
    mut main_cam: Query<&mut Camera, With<MainCamera>>,
) {
    let layers = RenderLayers::layer(LAYER);
    b.design = profile.design.clone();
    b.design_name = b.design.name.clone();
    b.designs = profile::list_designs();
    b.selected = None;
    b.ghost = None;
    b.hover = None;
    b.dirty = true;
    b.launch = false;
    b.go_menu = false;
    b.history.clear();
    b.show_research = false;
    b.show_missions = false;
    b.show_designs = false;
    b.root_pos = Vec3::ZERO;
    b.cam_yaw = 0.75;
    b.cam_pitch = 0.28;
    b.cam_dist = 11.0;
    for mut c in main_cam.iter_mut() {
        c.is_active = false;
    }
    // Dedicated camera and light that only see the builder layer.
    commands.spawn((
        Camera3d::default(),
        Camera { order: 1, clear_color: ClearColorConfig::Custom(INDIGO), ..default() },
        Msaa::Sample4,
        Projection::Perspective(PerspectiveProjection { fov: 50f32.to_radians(), near: 0.1, far: 500.0, ..default() }),
        Transform::from_xyz(8.0, 4.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
        DistanceFog { color: INDIGO, falloff: FogFalloff::Linear { start: 30.0, end: 140.0 }, ..default() },
        layers.clone(),
        BuilderCamera,
        BuilderEntity,
    ))
    .with_children(|cam| {
        // Same view, drawn after the scene with its own depth: markers always on top.
        cam.spawn((
            Camera3d::default(),
            Camera { order: 2, clear_color: ClearColorConfig::None, ..default() },
            Msaa::Sample4,
            Projection::Perspective(PerspectiveProjection { fov: 50f32.to_radians(), near: 0.1, far: 500.0, ..default() }),
            Transform::IDENTITY,
            RenderLayers::layer(OVERLAY_LAYER),
        ));
    });
    commands.spawn((
        DirectionalLight { illuminance: 9_000.0, shadows_enabled: true, ..default() },
        CascadeShadowConfigBuilder { first_cascade_far_bound: 25.0, maximum_distance: 80.0, ..default() }.build(),
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.9, -0.85, 0.0)),
        layers.clone(),
        BuilderEntity,
    ));
    let root = commands
        .spawn((Transform::from_translation(b.root_pos), Visibility::default(), BuilderRoot, BuilderEntity, layers.clone()))
        .id();
    b.root = Some(root);
    let sphere = meshes.add(Sphere::new(0.13).mesh().ico(2).unwrap());
    let com = commands
        .spawn((
            Mesh3d(sphere.clone()),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.25, 0.2), unlit: true, ..default() })),
            Transform::IDENTITY,
            MarkerCom,
            RenderLayers::layer(OVERLAY_LAYER),
        ))
        .id();
    let col = commands
        .spawn((
            Mesh3d(sphere.clone()),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.25, 0.55, 1.0), unlit: true, ..default() })),
            Transform::IDENTITY,
            MarkerCol,
            RenderLayers::layer(OVERLAY_LAYER),
        ))
        .id();
    let ok = materials.add(StandardMaterial { base_color: Color::srgba(0.3, 0.9, 0.4, 0.55), alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
    let bad = materials.add(StandardMaterial { base_color: Color::srgba(0.95, 0.55, 0.2, 0.55), alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
    let ghost = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            MeshMaterial3d(ok.clone()),
            Transform::IDENTITY,
            Visibility::Hidden,
            GhostEntity,
            GhostMaterials { ok, bad },
            layers.clone(),
        ))
        .id();
    let marker = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(0.07).mesh().ico(2).unwrap())),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.55, 0.1), unlit: true, ..default() })),
            Transform::IDENTITY,
            Visibility::Hidden,
            HoverMarker,
            RenderLayers::layer(OVERLAY_LAYER),
        ))
        .id();
    commands.entity(root).add_children(&[com, col, ghost, marker]);
    // Floor grid.
    let line = meshes.add(Cuboid::new(0.02, 0.005, 40.0));
    let grid_mat = materials.add(StandardMaterial { base_color: Color::srgba(0.75, 0.8, 1.0, 0.22), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
    for i in -20..=20 {
        let x = i as f32;
        commands.spawn((Mesh3d(line.clone()), MeshMaterial3d(grid_mat.clone()), Transform::from_xyz(x, 0.0, 0.0), layers.clone(), BuilderEntity));
        commands.spawn((Mesh3d(line.clone()), MeshMaterial3d(grid_mat.clone()), Transform { translation: Vec3::new(0.0, 0.0, x), rotation: Quat::from_rotation_y(FRAC_PI_2), scale: Vec3::ONE }, layers.clone(), BuilderEntity));
    }
    // Seamless void: the floor is the background color, only the grid and the plane stand out.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(1200.0, 1200.0))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: INDIGO, unlit: true, fog_enabled: false, ..default() })),
        Transform::from_xyz(0.0, -0.01, 0.0),
        layers.clone(),
        BuilderEntity,
    ));
    // Translate gizmo handles for the selected part (X red, Y green, Z blue).
    let shaft = meshes.add(Cylinder::new(0.035, 1.1).mesh().resolution(10).build());
    let tip = meshes.add(Cone::new(0.11, 0.3).mesh().resolution(12).build());
    for (axis, color) in [(Axis::X, Color::srgb(0.9, 0.3, 0.3)), (Axis::Y, Color::srgb(0.35, 0.85, 0.35)), (Axis::Z, Color::srgb(0.35, 0.5, 0.95))] {
        let mat = materials.add(StandardMaterial { base_color: color, unlit: true, ..default() });
        let dir = axis.vec();
        let rot = Quat::from_rotation_arc(Vec3::Y, dir);
        let handle = commands
            .spawn((Transform::IDENTITY, Visibility::Hidden, GizmoHandle(axis), layers.clone()))
            .with_children(|p| {
                p.spawn((Mesh3d(shaft.clone()), MeshMaterial3d(mat.clone()), Transform { translation: dir * 0.85, rotation: rot, scale: Vec3::ONE }, layers.clone()));
                p.spawn((Mesh3d(tip.clone()), MeshMaterial3d(mat.clone()), Transform { translation: dir * 1.55, rotation: rot, scale: Vec3::ONE }, layers.clone()));
            })
            .id();
        commands.entity(root).add_child(handle);
    }
}

fn exit_hangar(
    mut commands: Commands,
    q: Query<Entity, (With<BuilderEntity>, Without<ChildOf>)>,
    mut b: ResMut<Builder>,
    mut profile: ResMut<Profile>,
    mut main_cam: Query<&mut Camera, With<MainCamera>>,
) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
    for mut c in main_cam.iter_mut() {
        c.is_active = true;
    }
    b.root = None;
    b.ghost = None;
    profile.design = b.design.clone();
    profile::save_profile(&profile);
}

// ---------------------------------------------------------------------------------------
// camera & picking

fn builder_camera(
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    focus: Res<UiFocus>,
    mut b: ResMut<Builder>,
    mut cam: Query<&mut Transform, With<BuilderCamera>>,
) {
    if !focus.pointer {
        // Left-drag on empty space orbits too (a click on a part still selects it, and the
        // gizmo keeps its own drag). Right or middle drag always orbits.
        if mouse.just_pressed(MouseButton::Left) && b.hover.is_none() && b.drag.is_none() {
            b.orbit_left = true;
        }
        let left_orbit = b.orbit_left && mouse.pressed(MouseButton::Left) && b.drag.is_none();
        if mouse.pressed(MouseButton::Right) || mouse.pressed(MouseButton::Middle) || left_orbit {
            b.cam_yaw -= motion.delta.x * 0.006;
            b.cam_pitch = (b.cam_pitch + motion.delta.y * 0.005).clamp(-1.25, 1.45);
        }
        let s = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y * 0.02,
        };
        if s != 0.0 {
            b.cam_dist = (b.cam_dist * (1.0 - s * 0.1)).clamp(3.0, 45.0);
        }
    }
    if !mouse.pressed(MouseButton::Left) {
        b.orbit_left = false;
    }
    let target = b.root_pos + Vec3::new(0.0, 1.2, 0.0);
    let off = Vec3::new(b.cam_pitch.cos() * b.cam_yaw.sin(), b.cam_pitch.sin(), b.cam_pitch.cos() * b.cam_yaw.cos()) * b.cam_dist;
    if let Ok(mut t) = cam.single_mut() {
        *t = Transform::from_translation(target + off).looking_at(target, Vec3::Y);
    }
}

fn ray_aabb(o: Vec3, d: Vec3, min: Vec3, max: Vec3) -> Option<(f32, Dir)> {
    let mut tmin = 0.0f32;
    let mut tmax = f32::MAX;
    let mut face = Dir::PY;
    let faces = [(Dir::NX, Dir::PX), (Dir::NY, Dir::PY), (Dir::NZ, Dir::PZ)];
    for axis in 0..3 {
        let (oa, da, mn, mx) = (o[axis], d[axis], min[axis], max[axis]);
        if da.abs() < 1e-8 {
            if oa < mn || oa > mx {
                return None;
            }
            continue;
        }
        let inv = 1.0 / da;
        let mut t1 = (mn - oa) * inv;
        let mut t2 = (mx - oa) * inv;
        let (mut f1, f2) = faces[axis];
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
            f1 = f2;
        }
        if t1 > tmin {
            tmin = t1;
            face = f1;
        }
        tmax = tmax.min(t2);
        if tmin > tmax {
            return None;
        }
    }
    Some((tmin, face))
}

/// Parameter along the axis line (p + a*t) of the point closest to the ray (o + d*s).
fn ray_axis_param(o: Vec3, d: Vec3, p: Vec3, a: Vec3) -> f32 {
    let w = o - p;
    let b = d.dot(a);
    let denom = 1.0 - b * b;
    if denom.abs() < 1e-5 {
        return w.dot(a);
    }
    let e = w.dot(d);
    let f = w.dot(a);
    // s along the ray, t along the axis
    let s = (b * f - e) / denom;
    b * s + f
}

fn rotation_from_to(a: Vec3, b: Vec3) -> Quat {
    if a.dot(b) > 0.999 {
        Quat::IDENTITY
    } else if a.dot(b) < -0.999 {
        let axis = if a.y.abs() < 0.5 { Vec3::Y } else { Vec3::Z };
        Quat::from_axis_angle(axis, std::f32::consts::PI)
    } else {
        Quat::from_rotation_arc(a, b)
    }
}

/// Orientation for placing `def` against a face with outward normal `face`, rotated `k` quarter
/// turns around that normal. Returns (rotation, mirrored).
fn orient_for(def: &PartDef, face: Dir, k: i32) -> (Quat, bool) {
    let n = face.vec();
    let want = -n;
    let want_dir = Dir::from_vec(want);
    if def.attach.is_empty() {
        return (Quat::IDENTITY, false);
    }
    let (base, mirrored) = if def.attach.contains(&want_dir) {
        (Quat::IDENTITY, false)
    } else if def.mirrorable() && want_dir == Dir::PX && def.attach.contains(&Dir::NX) {
        (Quat::IDENTITY, true)
    } else {
        (rotation_from_to(def.attach[0].vec(), want), false)
    };
    let extra = Quat::from_axis_angle(n, k as f32 * FRAC_PI_2);
    (design::snap_rotation(extra * base), mirrored)
}

fn self_fits(hmin: &Vec3, hmax: &Vec3, min: &Vec3, max: &Vec3) -> bool {
    const TOL: f32 = 0.01;
    min.x >= hmin.x - TOL && max.x <= hmax.x + TOL && min.y >= hmin.y - TOL && max.y <= hmax.y + TOL && min.z >= hmin.z - TOL && max.z <= hmax.z + TOL
}

fn mirror_part(p: &PlacedPart) -> PlacedPart {
    let mut m = p.clone();
    m.pos[0] = -m.pos[0];
    m.rot = [p.rot[0], -p.rot[1], -p.rot[2], p.rot[3]];
    m.mirrored = !p.mirrored;
    m
}

fn compute_placement(b: &mut Builder, profile: &Profile) {
    let Some(ghost) = b.ghost.as_mut() else { return };
    ghost.candidate = None;
    ghost.mirror_candidate = None;
    ghost.valid = false;
    ghost.reason.clear();
    let Some((hi, face, hit)) = b.hover else {
        ghost.reason = "Point at a part to attach to.".into();
        return;
    };
    let host = &b.design.parts[hi];
    // Parts that join nose-to-tail (engines, body segments, cones, cockpits, booms) only
    // make sense on a body's front or back: pointing at its side, top or bottom snaps
    // them to the nearer end instead of hanging them off that face. Fans, tanks, chutes,
    // wings and wheels keep their free placement.
    let host_body = matches!(host.def().kind, PartKind::Cockpit { .. } | PartKind::Fuselage { .. } | PartKind::Cone { .. } | PartKind::Boom | PartKind::Hub);
    let ghost_inline = matches!(ghost.def.kind, PartKind::Cockpit { .. } | PartKind::Fuselage { .. } | PartKind::Cone { .. } | PartKind::Boom | PartKind::Hub | PartKind::Engine { .. })
        && ghost.def.id != "eng_fan";
    let face = if host_body && ghost_inline && !matches!(face, Dir::PZ | Dir::NZ) {
        if hit.z < host.pos().z { Dir::NZ } else { Dir::PZ }
    } else {
        face
    };
    let n = face.vec();
    let (rot, mirrored) = orient_for(ghost.def, face, ghost.k);
    let mut placed = PlacedPart::new(ghost.def, Vec3::ZERO, rot, mirrored);
    if ghost.def.internal() {
        // Internal parts go inside the container under the cursor, centered on its cross
        // section and slid along its length to the pointed-at spot.
        if !host.def().container() {
            ghost.reason = "Fuel tanks and batteries go inside a fuselage or cockpit.".into();
            return;
        }
        let (hmin, hmax) = host.aabb();
        let he = design::rotated_half_extents(placed.dims(), Quat::IDENTITY);
        let center = (hmin + hmax) * 0.5;
        let mut p = center;
        // Slide along the host's longest axis.
        let ext = hmax - hmin;
        let axis = if ext.z >= ext.x && ext.z >= ext.y { 2 } else if ext.x >= ext.y { 0 } else { 1 };
        p[axis] = snap(hit[axis]).clamp(hmin[axis] + he[axis], hmax[axis] - he[axis]);
        placed.rot = Quat::IDENTITY.to_array();
        placed.mirrored = false;
        placed.pos = p.to_array();
        let (min, max) = placed.aabb();
        if self_fits(&hmin, &hmax, &min, &max) {
            if b.design.overlaps_kind(min, max, None, true) {
                ghost.reason = "Something is already in there.".into();
                ghost.candidate = Some(placed);
                return;
            }
            let cost = placed.cost();
            if !profile.can_afford(cost) {
                ghost.reason = format!("Costs ${cost}, you have ${}.", profile.money);
                ghost.candidate = Some(placed);
                return;
            }
            ghost.candidate = Some(placed);
            ghost.valid = true;
        } else {
            ghost.reason = format!("Does not fit inside the {}.", host.def().name);
            ghost.candidate = Some(placed);
        }
        return;
    }
    if ghost.def.fits_face() && matches!(face, Dir::PZ | Dir::NZ) {
        let hhe = host.half_extents();
        let mut w = hhe.x * 2.0;
        let mut h = hhe.y * 2.0;
        // Behind a cone's narrow end the next segment takes the tapered section, not the base:
        // a Tail End after a Tail Start keeps shrinking instead of ballooning back out.
        if let PartKind::Cone { tip_scale, .. } = host.def().kind {
            let tip_dir = host.rot() * Vec3::Z;
            if tip_dir.dot(face.vec()) > 0.5 {
                w *= tip_scale;
                h *= tip_scale;
            }
        }
        placed.section = Some([w.clamp(0.3, 3.0), h.clamp(0.3, 3.0)]);
    }
    let dims = placed.dims();
    let he = design::rotated_half_extents(dims, rot);
    let hhe = host.half_extents();
    let hp = host.pos();
    let mut p = Vec3::new(snap(hit.x), snap(hit.y), snap(hit.z));
    let axis = match face {
        Dir::PX | Dir::NX => 0,
        Dir::PY | Dir::NY => 1,
        Dir::PZ | Dir::NZ => 2,
    };
    p[axis] = hp[axis] + n[axis] * (hhe[axis] + he[axis]);
    // Anything joined nose-to-tail (body segments, cones, engines, cockpits...) snaps onto
    // the host's centre line: fuselages must never end up offset sideways or up/down.
    let inline = axis == 2 && !ghost.def.is_wing() && !ghost.def.is_wheel();
    for a in 0..3 {
        if a == axis {
            continue;
        }
        if inline {
            p[a] = hp[a];
            continue;
        }
        let lo = hp[a] - hhe[a] - he[a] + 0.05;
        let hi_ = hp[a] + hhe[a] + he[a] - 0.05;
        if lo < hi_ {
            p[a] = p[a].clamp(lo, hi_);
        } else {
            p[a] = hp[a];
        }
    }
    placed.pos = p.to_array();
    let (min, max) = placed.aabb();
    if b.design.overlaps(min, max, None) {
        ghost.reason = "Overlaps another part.".into();
        ghost.candidate = Some(placed);
        return;
    }
    let mut cost = placed.cost();
    let mut mirror = None;
    if b.mirror && p.x.abs() > 0.01 {
        let m = mirror_part(&placed);
        let (mmin, mmax) = m.aabb();
        let self_overlap = mmin.x < max.x - 0.02 && mmax.x > min.x + 0.02;
        if !b.design.overlaps(mmin, mmax, None) && !self_overlap {
            cost += m.cost();
            mirror = Some(m);
        }
    }
    if !profile.can_afford(cost) {
        ghost.reason = format!("Costs ${cost}, you have ${}.", profile.money);
        ghost.candidate = Some(placed);
        ghost.mirror_candidate = mirror;
        return;
    }
    ghost.candidate = Some(placed);
    ghost.mirror_candidate = mirror;
    ghost.valid = true;
}

/// The placement preview: the real part (and its mirror twin) where it would go, tinted
/// green when the spot is valid and amber when it is not, so you see the orientation.
#[derive(Component)]
struct GhostPreview {
    tint_pending: bool,
    valid: bool,
}

fn sync_ghost_preview(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<MaterialCache>,
    b: Res<Builder>,
    previews: Query<Entity, With<GhostPreview>>,
    mut key: Local<Option<String>>,
) {
    let cand = b.ghost.as_ref().and_then(|g| g.candidate.as_ref().map(|c| (c, g.mirror_candidate.as_ref(), g.valid)));
    let new_key = cand.map(|(c, m, valid)| {
        format!("{}|{:?}|{:?}|{}|{}|{:?}|{}|{:?}", c.id, c.pos, c.rot, c.mirrored, c.length, c.section, valid, m.map(|m| (m.pos, m.rot, m.mirrored)))
    });
    if *key == new_key {
        return;
    }
    *key = new_key;
    for e in previews.iter() {
        commands.entity(e).despawn();
    }
    let Some((c, m, valid)) = cand else { return };
    let layers = RenderLayers::layer(LAYER);
    for p in std::iter::once(c).chain(m) {
        let e = partvis::spawn_part_visual(&mut commands, &mut meshes, &mut materials, &mut cache, p, usize::MAX, &layers);
        commands.entity(e).remove::<partvis::PartIndex>().insert((GhostPreview { tint_pending: true, valid }, BuilderEntity));
        // Under the builder root like the real parts: a parentless root with children was
        // not getting its transform propagated, leaving the preview at the origin.
        if let Some(root) = b.root {
            commands.entity(root).add_child(e);
        }
    }
}

fn tint_ghost_preview(
    mut previews: Query<(Entity, &mut GhostPreview)>,
    children: Query<&Children>,
    mut mats: Query<&mut MeshMaterial3d<StandardMaterial>>,
    tint: Query<&GhostMaterials>,
) {
    let Ok(t) = tint.single() else { return };
    for (root, mut gp) in previews.iter_mut() {
        if !gp.tint_pending {
            continue;
        }
        let handle = if gp.valid { t.ok.clone() } else { t.bad.clone() };
        let mut any = false;
        if let Ok(mut m) = mats.get_mut(root) {
            m.0 = handle.clone();
            any = true;
        }
        for e in children.iter_descendants(root) {
            if let Ok(mut m) = mats.get_mut(e) {
                m.0 = handle.clone();
                any = true;
            }
        }
        if any {
            gp.tint_pending = false;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn builder_interact(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    focus: Res<UiFocus>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &GlobalTransform), With<BuilderCamera>>,
    mut b: ResMut<Builder>,
    mut profile: ResMut<Profile>,
    mut toasts: ResMut<Toasts>,
    mut ghost_q: Query<(&mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>, &GhostMaterials), (With<GhostEntity>, Without<HoverMarker>)>,
    mut marker_q: Query<(&mut Transform, &mut Visibility), (With<HoverMarker>, Without<GhostEntity>)>,
) {
    b.hover = None;
    if !focus.pointer {
        if let (Ok(window), Ok((cam, cam_tf))) = (windows.single(), cams.single()) {
            if let Some(cursor) = window.cursor_position() {
                if let Ok(ray) = cam.viewport_to_world(cam_tf, cursor) {
                    let o = ray.origin - b.root_pos;
                    let d = Vec3::from(ray.direction);
                    let mut best: Option<(f32, usize, Dir)> = None;
                    let placing_internal = b.ghost.as_ref().map_or(false, |g| g.def.internal());
                    for (i, p) in b.design.parts.iter().enumerate() {
                        let def = p.def();
                        // In X-ray mode containers are see-through so internal parts can be picked;
                        // internal parts are only pickable in X-ray mode (or hidden otherwise).
                        if b.xray && def.container() && !placing_internal {
                            continue;
                        }
                        if !b.xray && def.internal() {
                            continue;
                        }
                        let (min, max) = p.aabb();
                        if let Some((t, face)) = ray_aabb(o, d, min, max) {
                            if best.map_or(true, |(bt, _, _)| t < bt) {
                                best = Some((t, i, face));
                            }
                        }
                    }
                    if let Some((t, i, face)) = best {
                        b.hover = Some((i, face, o + d * t));
                    }
                }
            }
        }
    }

    if !focus.keyboard {
        if keys.just_pressed(KeyCode::KeyM) {
            b.mirror = !b.mirror;
        }
        if keys.just_pressed(KeyCode::KeyX) {
            b.xray = !b.xray;
        }
        if keys.just_pressed(KeyCode::KeyR) {
            if let Some(g) = b.ghost.as_mut() {
                g.k += 1;
            }
        }
        if keys.just_pressed(KeyCode::Escape) {
            if b.ghost.is_some() {
                b.ghost = None;
            } else if b.show_stats || b.show_research || b.show_missions || b.show_designs || b.show_help {
                b.show_stats = false;
                b.show_research = false;
                b.show_missions = false;
                b.show_designs = false;
                b.show_help = false;
            } else {
                b.selected = None;
            }
        }
        if keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace) {
            if let Some(i) = b.selected.take() {
                delete_part(&mut b, &mut profile, &mut toasts, i);
            }
        }
        if keys.just_pressed(KeyCode::KeyZ) && (keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight) || keys.pressed(KeyCode::ControlLeft)) {
            undo(&mut b, &mut profile);
        }
    }

    if b.ghost.is_some() {
        if let Some(h) = b.debug_hover {
            if h.0 < b.design.parts.len() {
                b.hover = Some(h);
            }
        }
        compute_placement(&mut b, &profile);
    }

    if let Ok((mut tf, mut vis, mut mat, mats)) = ghost_q.single_mut() {
        match b.ghost.as_ref().and_then(|g| g.candidate.as_ref().map(|c| (c, g.valid))) {
            Some((c, valid)) => {
                // The box stays hidden: sync_ghost_preview draws the real part instead. It
                // still carries the tint materials the preview borrows.
                let he = c.half_extents();
                *tf = Transform { translation: c.pos(), rotation: Quat::IDENTITY, scale: he * 2.0 + Vec3::splat(0.02) };
                *vis = Visibility::Hidden;
                mat.0 = if valid { mats.ok.clone() } else { mats.bad.clone() };
            }
            None => *vis = Visibility::Hidden,
        }
    }
    if let Ok((mut tf, mut vis)) = marker_q.single_mut() {
        match (b.ghost.is_some(), b.hover) {
            (true, Some((_, _, hit))) => {
                tf.translation = hit;
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }

    if focus.pointer {
        return;
    }
    // Gizmo drag on the selected part.
    if let (Some(sel), Ok(window), Ok((cam, cam_tf))) = (b.selected, windows.single(), cams.single()) {
        if sel < b.design.parts.len() {
            if let Some(cursor) = window.cursor_position() {
                if let Ok(ray) = cam.viewport_to_world(cam_tf, cursor) {
                    let o = ray.origin - b.root_pos;
                    let d = Vec3::from(ray.direction);
                    let center = b.design.parts[sel].pos();
                    if b.drag.is_none() && mouse.just_pressed(MouseButton::Left) && b.ghost.is_none() {
                        // Which handle, if any, is under the cursor?
                        let mut best: Option<(f32, Axis)> = None;
                        for axis in [Axis::X, Axis::Y, Axis::Z] {
                            let dir = axis.vec();
                            let min = center + dir * 0.3 - Vec3::splat(0.14) + dir * 0.14;
                            let max = center + dir * 1.7 + Vec3::splat(0.14) - dir * 0.14;
                            let (min, max) = (min.min(max), min.max(max));
                            if let Some((t, _)) = ray_aabb(o, d, min, max) {
                                if best.map_or(true, |(bt, _)| t < bt) {
                                    best = Some((t, axis));
                                }
                            }
                        }
                        if let Some((_, axis)) = best {
                            let t0 = ray_axis_param(o, d, center, axis.vec());
                            let me = &b.design.parts[sel];
                            let twin = b.design.parts.iter().position(|p| {
                                p.id == me.id && p.mirrored != me.mirrored && (p.pos().x + me.pos().x).abs() < 0.01 && (p.pos().y - me.pos().y).abs() < 0.01 && (p.pos().z - me.pos().z).abs() < 0.01
                            });
                            let twin_start = twin.map(|t| b.design.parts[t].pos()).unwrap_or(Vec3::ZERO);
                            b.snapshot(&profile);
                            b.orbit_left = false;
                            b.drag = Some(Drag { axis, start_t: t0, start_pos: center, twin: if b.mirror { twin } else { None }, twin_start });
                            b.hover = None;
                            return;
                        }
                    }
                    if let Some(drag) = b.drag.as_ref() {
                        if mouse.pressed(MouseButton::Left) {
                            let t = ray_axis_param(o, d, drag.start_pos, drag.axis.vec());
                            let delta = snap(t - drag.start_t);
                            let new_pos = drag.start_pos + drag.axis.vec() * delta;
                            if (new_pos - b.design.parts[sel].pos()).length() > 1e-4 {
                                let twin = drag.twin;
                                let twin_pos = if drag.axis == Axis::X {
                                    drag.twin_start - Vec3::X * delta
                                } else {
                                    drag.twin_start + drag.axis.vec() * delta
                                };
                                let mut trial = b.design.parts[sel].clone();
                                trial.pos = new_pos.to_array();
                                let (min, max) = trial.aabb();
                                let internal = trial.def().internal();
                                let mut ok = !b.design.overlaps_kind(min, max, Some(sel), internal);
                                if let Some(ti) = twin {
                                    let mut tt = b.design.parts[ti].clone();
                                    tt.pos = twin_pos.to_array();
                                    let (tmin, tmax) = tt.aabb();
                                    ok = ok && !b.design.overlaps_kind(tmin, tmax, Some(ti), internal) && !(tmin.x < max.x - 0.02 && tmax.x > min.x + 0.02 && ti != sel);
                                }
                                if ok {
                                    b.design.parts[sel].pos = new_pos.to_array();
                                    if let Some(ti) = twin {
                                        b.design.parts[ti].pos = twin_pos.to_array();
                                    }
                                    b.dirty = true;
                                }
                            }
                            return;
                        } else {
                            b.drag = None;
                        }
                    }
                }
            }
        }
    }
    if mouse.just_pressed(MouseButton::Left) {
        if let Some(g) = b.ghost.as_ref() {
            if g.valid {
                let c = g.candidate.clone().unwrap();
                let m = g.mirror_candidate.clone();
                b.snapshot(&profile);
                let mut cost = c.cost();
                b.design.parts.push(c);
                if let Some(m) = m {
                    cost += m.cost();
                    b.design.parts.push(m);
                }
                profile.spend(cost);
                b.dirty = true;
            } else if let Some(g) = b.ghost.as_ref() {
                if !g.reason.is_empty() && b.hover.is_some() {
                    toasts.push(g.reason.clone());
                }
            }
        } else {
            b.selected = b.hover.map(|(i, _, _)| i);
        }
    }
    if mouse.just_released(MouseButton::Right) && b.ghost.is_some() {
        b.ghost = None;
    }
}

/// Upgrade (or downgrade) the cockpit: refund the old one, charge the new one, keep the
/// nose in place and push the rest of the plane back to fit.
pub fn swap_cockpit(b: &mut Builder, profile: &mut Profile, toasts: &mut Toasts, def: &'static PartDef) {
    let Some(ci) = b.design.cockpit_index() else { return };
    if b.design.parts[ci].id == def.id {
        toasts.push(format!("The plane already has the {}.", def.name));
        return;
    }
    let old_cost = b.design.parts[ci].cost();
    if !profile.can_afford(def.cost - old_cost) {
        toasts.push(format!("You cannot afford the {} (${} after refunding the old cockpit).", def.name, def.cost - old_cost));
        return;
    }
    b.snapshot(profile);
    let refund = b.design.swap_cockpit(def);
    profile.refund(refund);
    profile.spend(def.cost);
    b.selected = None;
    b.ghost = None;
    b.dirty = true;
    let overlapping = b.design.parts.iter().enumerate().any(|(i, p)| {
        let (min, max) = p.aabb();
        b.design.overlaps(min, max, Some(i))
    });
    if overlapping {
        toasts.push(format!("Fitted the {}. Some parts now overlap it: move them with the gizmo or undo (Cmd+Z).", def.name));
    } else {
        toasts.push(format!("Fitted the {}.", def.name));
    }
}

fn undo(b: &mut Builder, profile: &mut Profile) {
    if let Some((design, money)) = b.history.pop() {
        b.design = design;
        if !profile.creative {
            profile.money = money;
        }
        b.selected = None;
        b.dirty = true;
    }
}

fn delete_part(b: &mut Builder, profile: &mut Profile, toasts: &mut Toasts, i: usize) {
    if Some(i) == b.design.cockpit_index() {
        toasts.push("The cockpit cannot be removed.");
        return;
    }
    b.snapshot(profile);
    let removed = b.design.remove_part(i);
    let refund: i64 = removed.iter().map(|p| p.cost()).sum();
    profile.refund(refund);
    if removed.len() > 1 {
        toasts.push(format!("Removed {} parts (refunded ${refund}).", removed.len()));
    }
    b.selected = None;
    b.dirty = true;
}

// ---------------------------------------------------------------------------------------
// sync visuals with the design

#[allow(clippy::too_many_arguments)]
fn builder_sync(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<MaterialCache>,
    mut b: ResMut<Builder>,
    parts_q: Query<Entity, (With<PartIndex>, With<BuilderEntity>)>,
    mut roots: Query<&mut Transform, (With<BuilderRoot>, Without<MarkerCom>, Without<MarkerCol>)>,
    mut com_q: Query<(&mut Transform, &mut Visibility), (With<MarkerCom>, Without<MarkerCol>, Without<BuilderRoot>)>,
    mut col_q: Query<(&mut Transform, &mut Visibility), (With<MarkerCol>, Without<MarkerCom>, Without<BuilderRoot>)>,
) {
    // X-ray: containers vanish so internal tanks and batteries show.
    // (handled in builder_xray)
    // Marker visibility follows the CG toggle every frame (cheap).
    if let Ok((_, mut v)) = com_q.single_mut() {
        *v = if b.show_cg { Visibility::Inherited } else { Visibility::Hidden };
    }
    if let Ok((_, mut v)) = col_q.single_mut() {
        *v = if b.show_cg && b.stats.col.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    }
    if !b.dirty {
        return;
    }
    b.dirty = false;
    b.drag_overlay_applied = false;
    let Some(root) = b.root else { return };
    for e in parts_q.iter() {
        commands.entity(e).despawn();
    }
    let layers = RenderLayers::layer(LAYER);
    for (i, p) in b.design.parts.iter().enumerate() {
        let e = partvis::spawn_part_visual(&mut commands, &mut meshes, &mut materials, &mut cache, p, i, &layers);
        commands.entity(e).insert(BuilderEntity);
        commands.entity(root).add_child(e);
    }
    let min_y = b.design.parts.iter().map(|p| p.aabb().0.y).fold(f32::MAX, f32::min);
    b.root_pos.y = -min_y + 0.02;
    if let Ok(mut t) = roots.single_mut() {
        t.translation = b.root_pos;
    }
    b.stats = b.design.stats();
    if let Ok((mut t, _)) = com_q.single_mut() {
        t.translation = b.stats.com;
    }
    if let Ok((mut t, _)) = col_q.single_mut() {
        if let Some(c) = b.stats.col {
            t.translation = Vec3::new(c.x, c.y + 0.35, c.z);
        }
    }
    if let Some(sel) = b.selected {
        if sel >= b.design.parts.len() {
            b.selected = None;
        }
    }
}

/// Tints every part by its share of the plane's drag when the Drag overlay is on.
fn builder_drag_overlay(
    mut b: ResMut<Builder>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    parts_q: Query<(Entity, &PartIndex), With<BuilderEntity>>,
    children_q: Query<&Children>,
    mut mats: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut pending: Local<u8>,
) {
    // Part entities are spawned by commands; wait a frame after a sync before recoloring.
    if b.dirty {
        *pending = 2;
        return;
    }
    if !b.show_drag {
        *pending = 0;
        return;
    }
    if *pending > 0 {
        *pending -= 1;
        if *pending > 0 {
            return;
        }
    } else if !b.drag_overlay_applied {
        // fallthrough: apply once
    } else {
        return;
    }
    let areas = crate::flight::drag_areas(&b.design.parts);
    let max = areas.iter().map(|a| a.z).fold(0.01f32, f32::max);
    let mut handles: Vec<Handle<StandardMaterial>> = Vec::new();
    for a in &areas {
        let t = (a.z / max).clamp(0.0, 1.0).powf(0.6);
        // white -> yellow -> orange -> red
        let (r, g, bl) = if t < 0.5 { (1.0, 1.0 - t * 0.6, 1.0 - t * 2.0) } else { (1.0, 0.7 - (t - 0.5) * 1.2, 0.0) };
        handles.push(materials.add(StandardMaterial { base_color: Color::srgb(r, g.max(0.1), bl.max(0.0)), perceptual_roughness: 0.6, ..default() }));
    }
    fn recolor(e: Entity, h: &Handle<StandardMaterial>, children_q: &Query<&Children>, mats: &mut Query<&mut MeshMaterial3d<StandardMaterial>>) {
        if let Ok(mut m) = mats.get_mut(e) {
            m.0 = h.clone();
        }
        if let Ok(ch) = children_q.get(e) {
            for c in ch.iter() {
                recolor(c, h, children_q, mats);
            }
        }
    }
    for (e, idx) in parts_q.iter() {
        if let Some(h) = handles.get(idx.0) {
            recolor(e, h, &children_q, &mut mats);
        }
    }
    b.drag_overlay_applied = true;
}

fn builder_gizmo(b: Res<Builder>, mut handles: Query<(&mut Transform, &mut Visibility), With<GizmoHandle>>) {
    let sel = b.selected.filter(|&i| i < b.design.parts.len() && b.ghost.is_none());
    for (mut t, mut vis) in handles.iter_mut() {
        match sel {
            Some(i) => {
                t.translation = b.design.parts[i].pos();
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

fn builder_xray(b: Res<Builder>, mut parts_q: Query<(&PartIndex, &mut Visibility), With<BuilderEntity>>) {
    for (idx, mut vis) in parts_q.iter_mut() {
        let Some(p) = b.design.parts.get(idx.0) else { continue };
        let def = p.def();
        let hide = b.xray && def.container();
        *vis = if hide { Visibility::Hidden } else { Visibility::Inherited };
    }
}

// ---------------------------------------------------------------------------------------
// UI

/// A contextual tip for the current design and missions, shown on a lightbulb card.
fn builder_hint(b: &Builder, profile: &Profile, terrain: &Terrain) -> Option<String> {
    let s = &b.stats;
    if s.engines == 0 {
        return Some("Add an engine to the nose so you can take off.".into());
    }
    if s.wing_area <= 0.0 {
        return Some("Attach wings to the sides of the fuselage.".into());
    }
    if s.wheels == 0 {
        return Some("Hang wheels under the wings and nose so you can land.".into());
    }
    let total = s.mass_empty + s.fuel_capacity * design::FUEL_KG_PER_L;
    if total > s.wheel_capacity {
        return Some("Wheel load too heavy: add more wheels or stronger wheels.".into());
    }
    let active = profile.active_missions();
    if active.is_empty() {
        return Some("Accept a mission from the Missions board to earn money and research.".into());
    }
    for m in &active {
        let p = profile.progress(m.id);
        let left = m.count.saturating_sub(p.delivered);
        if m.cargo.is_passenger() && s.seats == 0 {
            return Some("Passengers need seats: research the Cabin Cockpit or a Passenger Cabin.".into());
        }
        if !m.cargo.is_passenger() && s.cargo_slots < left {
            return Some("Make the fuselage bigger to carry more cargo.".into());
        }
        let from = terrain.airport(m.from);
        let to = terrain.airport(m.to);
        let trip = (from.def.pos - to.def.pos).length() + (terrain.home().def.pos - from.def.pos).length();
        if s.range > 0.0 && trip > s.range * 0.8 {
            return Some("That trip is long for this fuel load: put a fuel tank inside the fuselage.".into());
        }
    }
    if let Some(m) = active.first() {
        return Some(format!("Press FLY, collect {} at {}, deliver to {}.", m.cargo.name().to_lowercase(), terrain.airport(m.from).name(), terrain.airport(m.to).name()));
    }
    None
}

fn part_tooltip(def: &PartDef) -> String {
    let mut s = format!("{}\n{}\n\nMass {:.0} kg", def.name, def.desc, def.mass);
    if def.stretchable() {
        s.push_str("/m");
    }
    match def.kind {
        PartKind::Wing { area, cl_max, .. } => s.push_str(&format!("\nArea {area:.1} m2, CL max {cl_max:.1}")),
        PartKind::Engine { thrust, burn, electric, .. } => s.push_str(&format!("\nThrust {:.1} kN, {} {:.2}/s", thrust / 1000.0, if electric { "charge" } else { "fuel L" }, burn)),
        PartKind::Wheel { max_impact, .. } => s.push_str(&format!("\nMax touchdown {max_impact:.0} m/s")),
        PartKind::Tank { fuel, .. } => s.push_str(&format!("\nFuel {fuel:.0} L")),
        PartKind::Battery { charge } => s.push_str(&format!("\nCharge {charge:.0}")),
        PartKind::Cockpit { cargo, seats, fuel, .. } => s.push_str(&format!("\nCargo {cargo} slots, seats {seats}, fuel {fuel:.0} L")),
        PartKind::Fuselage { cargo_per_m, seats_per_m, .. } => s.push_str(&format!("\nCargo {cargo_per_m:.0} slots/m, seats {seats_per_m:.0}/m")),
        _ => {}
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn builder_ui(
    mut contexts: EguiContexts,
    mut b: ResMut<Builder>,
    mut profile: ResMut<Profile>,
    terrain: Res<Terrain>,
    mut toasts: ResMut<Toasts>,
    mut next: ResMut<NextState<GameState>>,
    mut map_tex: ResMut<MapTexture>,
    mut radio: ResMut<crate::Radio>,
    icons: Res<crate::icons::PartIcons>,
) {
    let ctx = contexts.ctx_mut();
    let screen = ctx.screen_rect();

    // ---- money (top center)
    egui::Area::new(egui::Id::new("money")).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 12.0)).show(ctx, |ui| {
        pill(ui, &profile.money_text(), 20.0);
    });

    // ---- quick stats (top right)
    egui::Area::new(egui::Id::new("quick-stats")).anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0)).show(ctx, |ui| {
        let s = &b.stats;
        let total = s.mass_empty + s.fuel_capacity * design::FUEL_KG_PER_L;
        // Right-aligned so the row hugs the screen edge whatever the hint card's width.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            pill(ui, &format!("{} RP", profile.rp), 16.0);
            pill(ui, &format!("{:.1} kN", s.thrust / 1000.0), 16.0);
            if s.seats > 0 {
                pill(ui, &format!("{} seats", s.seats), 16.0);
            }
            // Slots and weight: a crate is 20 kg of mail or 130 kg of machinery, so the slot
            // count alone says nothing about what the plane can lift.
            if s.wheel_capacity > 0.0 {
                pill(ui, &format!("{} slots \u{b7} {:.0} kg payload", s.cargo_slots, s.payload), 16.0);
            } else {
                pill(ui, &format!("{} slots", s.cargo_slots), 16.0);
            }
            pill(ui, &format!("{:.0} L", s.fuel_capacity), 16.0);
            pill(ui, &format!("{:.0} kg", total), 16.0);
        });
        if let Some(w) = s.warnings.first() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui::amber_pill(ui, w);
            });
        }
        if let Some(hint) = builder_hint(&b, &profile, &terrain) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), 10.0, egui::Color32::from_rgb(250, 205, 60));
                        ui.painter().circle_filled(r.center(), 5.0, egui::Color32::WHITE);
                        ui.add_sized([300.0, 0.0], egui::Label::new(egui::RichText::new(hint).size(14.0).color(ui::TEXT)).wrap());
                    });
                });
            });
        }
    });

    // ---- parts palette (top left)
    egui::Area::new(egui::Id::new("palette")).anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 12.0)).show(ctx, |ui| {
        card(ui, |ui| {
            ui.set_width(TILE_W * 3.0 + TILE_GAP * 2.0);
            ui.horizontal_wrapped(|ui| {
                for c in Category::ALL {
                    let short = match c {
                        Category::Fuselage => "Body",
                        Category::Wings => "Wings",
                        Category::Control => "Tail",
                        Category::Engines => "Engines",
                        Category::Gear => "Gear",
                        Category::Fuel => "Fuel",
                        Category::Utility => "Extras",
                        Category::Cockpit => "Cockpit",
                    };
                    if pill_button(ui, short, b.category == c).clicked() {
                        b.category = c;
                    }
                }
            });
            ui.add_space(4.0);
            let max_h = (screen.height() - 220.0).max(200.0);
            let category = b.category;
            egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(TILE_GAP, TILE_GAP);
                    for def in PARTS.iter().filter(|p| p.category == category) {
                        let unlocked = profile.is_unlocked(def.id);
                        let placing = b.ghost.as_ref().map_or(false, |g| g.def.id == def.id);
                        let resp = part_tile(ui, def, icons.get(def.id), unlocked, placing).on_hover_text(part_tooltip(def));
                        if resp.clicked() {
                            b.selected = None;
                            if def.category == Category::Cockpit {
                                // Cockpit tiles swap the cockpit in place (the plane keeps its nose).
                                swap_cockpit(&mut b, &mut profile, &mut toasts, def);
                            } else {
                                b.ghost = Some(Ghost { def, k: 0, candidate: None, mirror_candidate: None, valid: false, reason: String::new() });
                                if !profile.can_afford(def.cost) {
                                    toasts.push(format!("You cannot afford the {} (${}).", def.name, def.cost));
                                }
                            }
                        }
                    }
                });
            });
        });
    });

    // ---- bottom left: undo, mirror, help
    egui::Area::new(egui::Id::new("bottom-left")).anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -12.0)).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.add_enabled(!b.history.is_empty(), ui::pill_btn("Undo")).clicked() {
                undo(&mut b, &mut profile);
            }
            let mirror = b.mirror;
            if toggle_button(ui, "Mirror", mirror).clicked() {
                b.mirror = !mirror;
            }
            if toggle_button(ui, "Help", b.show_help).clicked() {
                b.toggle_panel(Panel::Help);
            }
        });
    });

    // ---- bottom right: toggles and navigation
    egui::Area::new(egui::Id::new("bottom-right")).anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0)).show(ctx, |ui| {
        ui.horizontal(|ui| {
            let cg = b.show_cg;
            if toggle_button(ui, "CG", cg).clicked() {
                b.show_cg = !cg;
            }
            let xr = b.xray;
            if toggle_button(ui, "X-ray", xr).on_hover_text("See through fuselages to reach the tanks and batteries inside.").clicked() {
                b.xray = !xr;
            }
            let dr = b.show_drag;
            if toggle_button(ui, "Drag", dr).on_hover_text("Tint parts by how much drag they add.").clicked() {
                b.show_drag = !dr;
                b.dirty = true;
            }
            let st = b.show_stats;
            if toggle_button(ui, "Stats", st).clicked() {
                b.show_stats = !st;
            }
            let r = b.show_research;
            if toggle_button(ui, "Research", r).clicked() {
                b.toggle_panel(Panel::Research);
            }
            let m = b.show_missions;
            if toggle_button(ui, "Missions", m).clicked() {
                b.toggle_panel(Panel::Missions);
            }
            let d = b.show_designs;
            if toggle_button(ui, "Designs", d).clicked() {
                b.toggle_panel(Panel::Designs);
            }
            if ui.add(ui::pill_btn("Menu")).clicked() {
                b.go_menu = true;
            }
            let launch = egui::Button::new(egui::RichText::new("FLY").size(18.0).strong().color(egui::Color32::WHITE))
                .fill(egui::Color32::from_rgb(52, 160, 90))
                .corner_radius(20.0)
                .min_size(egui::vec2(96.0, 40.0));
            if ui.add(launch).clicked() {
                b.launch = true;
            }
        });
    });

    // ---- bottom center: placement hint or selected part
    egui::Area::new(egui::Id::new("bottom-center")).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -12.0)).show(ctx, |ui| {
        if let Some(g) = b.ghost.as_ref() {
            let text = if g.valid {
                format!("Placing {}: click to attach, R to rotate, Esc to cancel", g.def.name)
            } else if g.reason.is_empty() {
                format!("Placing {}", g.def.name)
            } else {
                format!("Placing {}: {}", g.def.name, g.reason)
            };
            if g.valid {
                pill(ui, &text, 14.0);
            } else {
                ui::amber_pill(ui, &text);
            }
        } else if b.selected.is_some() {
            card(ui, |ui| selection_panel(ui, &mut b, &mut profile, &mut toasts));
        }
    });

    // ---- windows
    if b.show_stats {
        let mut open = true;
        egui::Window::new("Aircraft").open(&mut open).anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 60.0)).resizable(false).collapsible(false).show(ctx, |ui| {
            stats_panel(ui, &b);
        });
        b.show_stats = open;
    }
    if b.show_research {
        let mut open = true;
        egui::Window::new("Research").open(&mut open).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).resizable(false).collapsible(false).default_width(420.0).show(ctx, |ui| {
            research_panel(ui, &mut profile, &mut toasts);
        });
        b.show_research = open;
    }
    if b.show_missions {
        let mut open = true;
        egui::Window::new("Missions").open(&mut open).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).resizable(true).collapsible(false).default_width(640.0).show(ctx, |ui| {
            let tex = map_tex.get(ctx, &terrain);
            let hl: Vec<(&str, egui::Color32)> = profile
                .active_missions()
                .iter()
                .flat_map(|m| [(m.from, egui::Color32::from_rgb(70, 150, 240)), (m.to, egui::Color32::from_rgb(60, 180, 90))])
                .collect();
            ui.horizontal_top(|ui| {
                let map_size = (ui.ctx().screen_rect().height() - 240.0).clamp(300.0, 640.0);
                crate::flight::draw_map(ui, &tex, &terrain, &profile, map_size, None, &hl);
                ui.vertical(|ui| {
                    ui.set_width(330.0);
                    crate::flight::mission_board(ui, &mut profile, &terrain, &mut toasts, &mut radio);
                });
            });
        });
        b.show_missions = open;
    }
    if b.show_designs {
        let mut open = true;
        egui::Window::new("Designs").open(&mut open).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).resizable(false).collapsible(false).default_width(380.0).show(ctx, |ui| {
            designs_panel(ui, &mut b, &mut profile, &mut toasts);
        });
        b.show_designs = open;
    }
    if b.show_help {
        let mut open = true;
        egui::Window::new("How to build").open(&mut open).anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -60.0)).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.label("Pick a part on the left, point at a face of your plane and click to attach it. Mirror places a matching part on the other side.");
            ui.label("R rotates the part in 90 degree steps. Right click or Esc cancels.");
            ui.label("Click a placed part to select it: stretch fuselages, recolor, or delete (Del). Removing a part removes everything hanging off it. Cmd+Z or Undo reverts.");
            ui.label("Drag on empty space (or right-drag) to orbit, scroll to zoom. Red ball: center of mass. Blue ball: center of lift. Keep the red one a little ahead of the blue one.");
        });
        b.show_help = open;
    }

    // ---- transitions
    if b.launch {
        b.launch = false;
        if b.stats.wing_area <= 0.0 {
            toasts.push("Launching without wings. Good luck.");
        }
        b.design.name = b.design_name.clone();
        profile.design = b.design.clone();
        profile::save_profile(&profile);
        next.set(GameState::Flight);
    }
    if b.go_menu {
        b.go_menu = false;
        profile.design = b.design.clone();
        profile::save_profile(&profile);
        next.set(GameState::MainMenu);
    }
}

const TILE_W: f32 = 104.0;
const TILE_H: f32 = 120.0;
const TILE_GAP: f32 = 6.0;
const TILE_ICON: f32 = 62.0;

/// One palette tile: the rendered part on top, name and price below.
fn part_tile(ui: &mut egui::Ui, def: &PartDef, icon: Option<egui::TextureId>, unlocked: bool, placing: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(TILE_W, TILE_H), if unlocked { egui::Sense::click() } else { egui::Sense::hover() });
    let fill = if placing {
        egui::Color32::from_rgb(95, 150, 235)
    } else if unlocked && resp.hovered() {
        egui::Color32::from_rgb(218, 222, 232)
    } else if unlocked {
        egui::Color32::from_rgb(232, 234, 240)
    } else {
        egui::Color32::from_rgb(243, 243, 246)
    };
    ui.painter().rect_filled(rect, 10.0, fill);
    let icon_rect = egui::Rect::from_min_size(egui::pos2(rect.center().x - TILE_ICON * 0.5, rect.min.y + 5.0), egui::vec2(TILE_ICON, TILE_ICON));
    if let Some(tex) = icon {
        let tint = if unlocked { egui::Color32::WHITE } else { egui::Color32::from_white_alpha(60) };
        egui::Image::new((tex, icon_rect.size())).tint(tint).paint_at(ui, icon_rect);
    }
    let (name_col, sub_col) = if placing {
        (egui::Color32::WHITE, egui::Color32::from_white_alpha(220))
    } else if unlocked {
        (ui::TEXT, egui::Color32::from_gray(110))
    } else {
        (egui::Color32::from_gray(150), egui::Color32::from_gray(160))
    };
    let name = ui.painter().layout(def.name.to_string(), egui::FontId::proportional(12.0), name_col, TILE_W - 10.0);
    let name_h = name.size().y.min(30.0);
    let sub = if unlocked {
        if def.stretchable() {
            format!("${}/m", def.cost)
        } else {
            format!("${}", def.cost)
        }
    } else {
        format!("{} RP", def.research)
    };
    let sub_galley = ui.painter().layout_no_wrap(sub, egui::FontId::proportional(11.5), sub_col);
    let sub_h = sub_galley.size().y;
    let text_top = rect.max.y - 6.0 - sub_h - name_h;
    ui.painter().galley(egui::pos2(rect.center().x - name.size().x * 0.5, text_top), name, name_col);
    ui.painter().galley(egui::pos2(rect.center().x - sub_galley.size().x * 0.5, rect.max.y - 6.0 - sub_h), sub_galley, sub_col);
    if !unlocked {
        // Small padlock in the corner.
        let c = egui::pos2(rect.max.x - 12.0, rect.min.y + 12.0);
        let p = ui.painter();
        p.rect_filled(egui::Rect::from_center_size(c + egui::vec2(0.0, 2.5), egui::vec2(9.0, 7.0)), 1.5, egui::Color32::from_gray(150));
        p.circle_stroke(c + egui::vec2(0.0, -2.5), 3.0, egui::Stroke::new(1.5_f32, egui::Color32::from_gray(150)));
    }
    resp
}

/// Research tech tree: one row per category, one column per tier, connected tiles.
/// Also used from the airport card in flight.
pub fn research_panel(ui: &mut egui::Ui, profile: &mut Profile, toasts: &mut Toasts) {
    let sel_id = ui.id().with("research-selected");
    let mut selected: Option<String> = ui.data(|d| d.get_temp(sel_id));
    let tile = egui::vec2(100.0, 62.0);
    let gap = 16.0;
    let rows = [Category::Engines, Category::Wings, Category::Control, Category::Fuselage, Category::Gear, Category::Fuel, Category::Utility, Category::Cockpit];
    let max_tier = PARTS.iter().map(|p| p.tier).max().unwrap_or(1) as usize;
    let mut rects: Vec<(usize, u8, egui::Rect)> = Vec::new();
    let max_h = (ui.ctx().screen_rect().height() - 230.0).max(240.0);
    let scroll = egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
        egui::Frame::new().fill(egui::Color32::from_rgb(236, 237, 242)).corner_radius(12.0).inner_margin(10.0).show(ui, |ui| {
            for (row, cat) in rows.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for tier in 1..=max_tier {
                        let here: Vec<&PartDef> = PARTS.iter().filter(|p| p.category == *cat && p.tier as usize == tier).collect();
                        if here.is_empty() {
                            ui.allocate_exact_size(tile, egui::Sense::hover());
                            continue;
                        }
                        // Several parts can share a tier; stack them vertically inside the column.
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 6.0;
                            for def in here {
                                let unlocked = profile.is_unlocked(def.id);
                                let can = parts::researchable(def, &profile.unlocked);
                                let is_sel = selected.as_deref() == Some(def.id);
                                let short: String = def.name.split(' ').take(3).collect::<Vec<_>>().join(" ");
                                let (fill, text_col) = if unlocked {
                                    (egui::Color32::WHITE, ui::TEXT)
                                } else if can {
                                    (egui::Color32::from_rgb(248, 248, 250), egui::Color32::from_rgb(190, 40, 40))
                                } else {
                                    (egui::Color32::from_rgb(226, 227, 232), egui::Color32::from_gray(150))
                                };
                                let label = if unlocked { format!("{short}") } else { format!("{short}\n{} RP", def.research) };
                                let btn = egui::Button::new(egui::RichText::new(label).size(12.0).strong().color(text_col))
                                    .fill(fill)
                                    .corner_radius(10.0)
                                    .min_size(tile)
                                    .stroke(if is_sel { egui::Stroke::new(2.5_f32, egui::Color32::from_rgb(70, 130, 230)) } else { egui::Stroke::NONE })
                                    .wrap_mode(egui::TextWrapMode::Wrap);
                                let resp = ui.add_sized(tile, btn).on_hover_text(part_tooltip(def));
                                if resp.clicked() {
                                    selected = Some(def.id.to_string());
                                }
                                rects.push((row, def.tier, resp.rect));
                            }
                        });
                    }
                });
                ui.add_space(6.0);
            }
        });
    });
    // Connector lines between consecutive tiers in a row, clipped to the scrolled view so
    // a line to a tile below the fold does not run through the footer.
    let painter = ui.painter().with_clip_rect(scroll.inner_rect);
    for (row, tier, r) in &rects {
        if let Some((_, _, prev)) = rects.iter().filter(|(rw, t, _)| rw == row && *t + 1 == *tier).min_by(|a, b| a.2.min.y.partial_cmp(&b.2.min.y).unwrap()) {
            painter.line_segment([prev.right_center(), r.left_center()], egui::Stroke::new(2.0_f32, egui::Color32::from_gray(200)));
        }
    }
    ui.add_space(8.0);
    let sel_def = selected.as_deref().and_then(parts::part);
    ui.horizontal(|ui| {
        let (enabled, label) = match sel_def {
            Some(def) if profile.is_unlocked(def.id) => (false, format!("{} is unlocked", def.name)),
            Some(def) => {
                let can = parts::researchable(def, &profile.unlocked);
                let affordable = profile.rp >= def.research;
                let needs: Vec<&str> = parts::prerequisites(def).iter().map(|p| p.name).collect();
                let text = if !can { format!("Unlock {} (needs {})", def.name, needs.join(" or ")) } else if !affordable { format!("Unlock {} ({} RP needed)", def.name, def.research) } else { format!("Unlock {} for {} RP", def.name, def.research) };
                (can && affordable, text)
            }
            None => (false, "Select a part".to_string()),
        };
        if ui.add_enabled(enabled, egui::Button::new(egui::RichText::new(label).size(15.0).strong().color(ui::TEXT)).fill(egui::Color32::from_rgb(236, 237, 242)).corner_radius(18.0).min_size(egui::vec2(260.0, 34.0))).clicked() {
            if let Some(def) = sel_def {
                profile.rp -= def.research;
                profile.unlocked.push(def.id.to_string());
                profile::save_profile(profile);
                toasts.push(format!("Researched {}.", def.name));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            pill(ui, &format!("{} RP", profile.rp), 15.0);
        });
    });
    // The description gets its own wrapped line: beside the button it overlapped it.
    if let Some(def) = sel_def {
        ui.add(egui::Label::new(egui::RichText::new(def.desc).size(12.0).color(egui::Color32::from_gray(110))).wrap());
    }
    ui.data_mut(|d| {
        if let Some(sel) = selected {
            d.insert_temp(sel_id, sel);
        }
    });
}

fn designs_panel(ui: &mut egui::Ui, b: &mut Builder, profile: &mut Profile, toasts: &mut Toasts) {
    ui.label(egui::RichText::new("Current design").strong());
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut b.design_name);
    });
    ui.horizontal(|ui| {
        if ui.add(ui::pill_btn("Save design")).clicked() {
            let name = if b.design_name.trim().is_empty() { "Untitled".to_string() } else { b.design_name.trim().to_string() };
            b.design_name = name.clone();
            b.design.name = name.clone();
            profile::save_design(&name, &b.design);
            b.designs = profile::list_designs();
            toasts.push(format!("Saved \"{name}\"."));
        }
        if ui.add(ui::pill_btn("New (cockpit only)")).clicked() {
            b.snapshot(profile);
            let refund = b.design.total_cost();
            profile.refund(refund);
            b.design = PlaneDesign::empty();
            b.design_name = b.design.name.clone();
            b.selected = None;
            b.ghost = None;
            b.dirty = true;
            toasts.push(format!("Cleared the design. ${refund} refunded."));
        }
    });
    ui.horizontal(|ui| {
        for (name, cockpit) in [("Basic", "cockpit_basic"), ("Cabin", "cockpit_cabin"), ("Airliner", "cockpit_airliner")] {
            let def = parts::part_or_panic(cockpit);
            let enabled = profile.is_unlocked(cockpit) && b.design.parts.len() == 1;
            if ui.add_enabled(enabled, ui::pill_btn(&format!("{name} cockpit (${})", def.cost))).on_hover_text("Available on an empty design.").clicked() {
                let current = b.design.total_cost();
                if profile.can_afford(def.cost - current) {
                    b.snapshot(profile);
                    profile.refund(current);
                    profile.spend(def.cost);
                    b.design.parts = vec![PlacedPart::new(def, Vec3::ZERO, Quat::IDENTITY, false)];
                    b.dirty = true;
                } else {
                    toasts.push("Not enough money for that cockpit.");
                }
            }
        }
    });
    ui.separator();
    ui.label(egui::RichText::new("Saved designs").strong());
    let names = b.designs.clone();
    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
        for name in names {
            ui.horizontal(|ui| {
                ui.label(&name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(ui::pill_btn("Delete")).clicked() {
                        profile::delete_design(&name);
                        b.designs = profile::list_designs();
                    }
                    if ui.add(ui::pill_btn("Load")).clicked() {
                        match profile::load_design(&name) {
                            Some(d) => load_design(b, profile, toasts, d),
                            None => toasts.push("Could not read that design."),
                        }
                    }
                });
            });
        }
    });
}

fn load_design(b: &mut Builder, profile: &mut Profile, toasts: &mut Toasts, d: PlaneDesign) {
    if d.parts.iter().any(|p| parts::part(&p.id).is_none()) {
        toasts.push("That design uses unknown parts.");
        return;
    }
    if let Some(p) = d.parts.iter().find(|p| !profile.is_unlocked(&p.id)) {
        toasts.push(format!("That design uses {}, which you have not researched.", p.def().name));
        return;
    }
    let current = b.design.total_cost();
    let new_cost = d.total_cost();
    if !profile.can_afford(new_cost - current) {
        toasts.push(format!("Loading it costs ${} more than you have.", new_cost - current - profile.money));
        return;
    }
    b.snapshot(profile);
    profile.refund(current);
    profile.spend(new_cost);
    b.design_name = d.name.clone();
    b.design = d;
    b.selected = None;
    b.ghost = None;
    b.dirty = true;
    toasts.push("Design loaded.");
}

fn stats_panel(ui: &mut egui::Ui, b: &Builder) {
    let s = &b.stats;
    let fuel_mass = s.fuel_capacity * design::FUEL_KG_PER_L;
    let total = s.mass_empty + fuel_mass;
    let weight = total * 9.81;
    let twr = if weight > 0.0 { s.thrust / weight } else { 0.0 };
    egui::Grid::new("stats").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
        let row = |ui: &mut egui::Ui, k: &str, v: String| {
            ui.label(egui::RichText::new(k).color(egui::Color32::from_gray(110)));
            ui.label(egui::RichText::new(v).strong());
            ui.end_row();
        };
        row(ui, "Empty mass", format!("{:.0} kg", s.mass_empty));
        row(ui, "With fuel", format!("{:.0} kg", total));
        row(ui, "Thrust", format!("{:.1} kN  ({:.2} T/W)", s.thrust / 1000.0, twr));
        row(ui, "Wing area", format!("{:.1} m2", s.wing_area));
        row(ui, "Stall speed", if s.stall_speed > 0.0 { format!("{:.0} km/h", s.stall_speed * 3.6) } else { "-".into() });
        row(ui, "Top speed", if s.top_speed > 0.0 { format!("~{:.0} km/h", s.top_speed * 3.6) } else { "-".into() });
        row(ui, "Fuel", format!("{:.0} L", s.fuel_capacity));
        if s.battery > 0.0 {
            row(ui, "Battery", format!("{:.0}", s.battery));
        }
        row(ui, "Endurance", if s.endurance > 0.0 { format!("~{:.0} min", s.endurance / 60.0) } else { "-".into() });
        row(ui, "Range", if s.range > 0.0 { format!("~{:.1} km", s.range / 1000.0) } else { "-".into() });
        row(ui, "Cargo slots", format!("{}", s.cargo_slots));
        row(ui, "Payload", if s.wheel_capacity > 0.0 { format!("{:.0} kg (gear limit)", s.payload) } else { "-".into() });
        row(ui, "Seats", format!("{}", s.seats));
        row(ui, "Engines", format!("{}", s.engines));
        row(ui, "Wheels", format!("{}", s.wheels));
        row(ui, "Parts", format!("{}", b.design.parts.len()));
        row(ui, "Value", format!("${}", s.cost));
    });
    if !s.warnings.is_empty() {
        ui.add_space(4.0);
        for w in &s.warnings {
            ui.label(egui::RichText::new(format!("! {w}")).color(egui::Color32::from_rgb(200, 120, 20)).size(13.0));
        }
    }
}

fn selection_panel(ui: &mut egui::Ui, b: &mut Builder, profile: &mut Profile, toasts: &mut Toasts) {
    let Some(i) = b.selected else { return };
    if i >= b.design.parts.len() {
        b.selected = None;
        return;
    }
    let def = b.design.parts[i].def();
    ui.set_min_width(360.0);
    ui.label(egui::RichText::new(def.name).strong().size(16.0));
    let p = &b.design.parts[i];
    ui.label(egui::RichText::new(format!("${}   {:.0} kg", p.cost(), p.mass())).size(12.0).color(egui::Color32::from_gray(110)));
    if let Some((lo, hi)) = def.stretch {
        let len = p.length;
        ui.horizontal(|ui| {
            ui.label(format!("Length {len:.1} m"));
            for (label, delta) in [("-0.5 m", -0.5f32), ("+0.5 m", 0.5f32)] {
                let target = (len + delta).clamp(lo, hi);
                let enabled = (target - len).abs() > 0.01;
                if ui.add_enabled(enabled, ui::pill_btn(label)).clicked() {
                    let old_cost = def.cost_for(len);
                    let new_cost = def.cost_for(target);
                    if profile.can_afford(new_cost - old_cost) {
                        b.snapshot(profile);
                        profile.spend(new_cost - old_cost);
                        b.design.set_length(i, target);
                        let (min, max) = b.design.parts[i].aabb();
                        if b.design.overlaps(min, max, Some(i)) {
                            undo(b, profile);
                            toasts.push("Cannot stretch: it would overlap another part.");
                        }
                        b.dirty = true;
                    } else {
                        toasts.push("Not enough money to stretch it.");
                    }
                }
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Color");
        for (k, (rgb, name)) in PALETTE.iter().enumerate() {
            let c = egui::Color32::from_rgb((rgb[0] * 255.0) as u8, (rgb[1] * 255.0) as u8, (rgb[2] * 255.0) as u8);
            let selected = b.design.parts[i].color == k;
            let btn = egui::Button::new(" ").fill(c).min_size(egui::vec2(22.0, 22.0)).corner_radius(11.0).stroke(if selected { egui::Stroke::new(2.0_f32, egui::Color32::from_gray(40)) } else { egui::Stroke::new(1.0_f32, egui::Color32::from_gray(200)) });
            if ui.add(btn).on_hover_text(*name).clicked() {
                b.snapshot(profile);
                b.design.parts[i].color = k;
                b.dirty = true;
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.add(ui::pill_btn("Paint whole plane")).clicked() {
            b.snapshot(profile);
            let c = b.design.parts[i].color;
            for p in b.design.parts.iter_mut() {
                if p.def().category != Category::Control && !p.def().is_wheel() && !p.def().is_engine() {
                    p.color = c;
                }
            }
            b.dirty = true;
        }
        if Some(i) != b.design.cockpit_index() {
            let del = egui::Button::new(egui::RichText::new("Delete").color(egui::Color32::WHITE)).fill(egui::Color32::from_rgb(200, 70, 60)).corner_radius(18.0).min_size(egui::vec2(0.0, 30.0));
            if ui.add(del).clicked() {
                delete_part(b, profile, toasts, i);
            }
        }
        if ui.add(ui::pill_btn("Done")).clicked() {
            b.selected = None;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_box_face() {
        let (t, face) = ray_aabb(Vec3::new(0.0, 0.0, -10.0), Vec3::Z, Vec3::splat(-1.0), Vec3::splat(1.0)).unwrap();
        assert!((t - 9.0).abs() < 1e-5);
        assert_eq!(face, Dir::NZ);
        let (_, face) = ray_aabb(Vec3::new(5.0, 0.5, 0.0), Vec3::NEG_X, Vec3::splat(-1.0), Vec3::splat(1.0)).unwrap();
        assert_eq!(face, Dir::PX);
        assert!(ray_aabb(Vec3::new(5.0, 5.0, 0.0), Vec3::NEG_X, Vec3::splat(-1.0), Vec3::splat(1.0)).is_none());
    }

    #[test]
    fn wings_mirror_on_the_left_and_engines_face_forward() {
        let wing = parts::part_or_panic("wing_biplane");
        let (rot, mirrored) = orient_for(wing, Dir::PX, 0);
        assert!(!mirrored && rot.abs_diff_eq(Quat::IDENTITY, 1e-5));
        let (rot, mirrored) = orient_for(wing, Dir::NX, 0);
        assert!(mirrored && rot.abs_diff_eq(Quat::IDENTITY, 1e-5));
        let engine = parts::part_or_panic("eng_small");
        let (rot, _) = orient_for(engine, Dir::NZ, 0);
        assert!(rot.abs_diff_eq(Quat::IDENTITY, 1e-5));
        let (rot, _) = orient_for(engine, Dir::NY, 0);
        assert!((rot * Vec3::NEG_Z).abs_diff_eq(Vec3::NEG_Z, 1e-5));
        let wheel = parts::part_or_panic("wheel_fixed");
        let (rot, _) = orient_for(wheel, Dir::PX, 0);
        assert!((rot * Vec3::Y).abs_diff_eq(Vec3::NEG_X, 1e-5));
    }

    #[test]
    fn placement_snaps_to_host_face_and_rejects_overlap() {
        let mut b = Builder::default();
        b.design = PlaneDesign::empty();
        let profile = Profile::new_creative();
        let hub = parts::part_or_panic("hub");
        b.ghost = Some(Ghost { def: hub, k: 0, candidate: None, mirror_candidate: None, valid: false, reason: String::new() });
        b.hover = Some((0, Dir::PZ, Vec3::new(0.13, -0.2, 1.0)));
        compute_placement(&mut b, &profile);
        let g = b.ghost.as_ref().unwrap();
        assert!(g.valid, "reason: {}", g.reason);
        let c = g.candidate.as_ref().unwrap();
        assert!((c.pos().z - 1.5).abs() < 1e-4, "z = {}", c.pos().z);
        assert!(g.mirror_candidate.is_none());
        // A fuel tank goes inside the cockpit instead of onto its face.
        let tank = parts::part_or_panic("tank_small");
        b.ghost = Some(Ghost { def: tank, k: 0, candidate: None, mirror_candidate: None, valid: false, reason: String::new() });
        b.hover = Some((0, Dir::PY, Vec3::new(0.0, 0.5, 0.6)));
        compute_placement(&mut b, &profile);
        let placed_tank = {
            let g = b.ghost.as_ref().unwrap();
            assert!(g.valid, "internal reason: {}", g.reason);
            let t = g.candidate.as_ref().unwrap();
            // The 1.6 m tank can only slide 0.2 m inside the 2 m cockpit before it hits the wall.
            assert!((t.pos().z - 0.2).abs() < 1e-4 && t.pos().y.abs() < 1e-4, "tank at {:?}", t.pos());
            t.clone()
        };
        b.design.parts.push(placed_tank);
        // A second tank cannot share the space.
        b.ghost = Some(Ghost { def: tank, k: 0, candidate: None, mirror_candidate: None, valid: false, reason: String::new() });
        b.hover = Some((0, Dir::PY, Vec3::new(0.0, 0.5, 0.6)));
        compute_placement(&mut b, &profile);
        assert!(!b.ghost.as_ref().unwrap().valid);
        b.design.parts.pop();
        let wing = parts::part_or_panic("wing_biplane");
        b.ghost = Some(Ghost { def: wing, k: 0, candidate: None, mirror_candidate: None, valid: false, reason: String::new() });
        b.hover = Some((0, Dir::PX, Vec3::new(0.5, 0.0, 0.0)));
        compute_placement(&mut b, &profile);
        let g = b.ghost.as_ref().unwrap();
        assert!(g.valid, "reason: {}", g.reason);
        let c = g.candidate.as_ref().unwrap();
        assert!((c.pos().x - (0.5 + wing.dims.x * 0.5)).abs() < 1e-4);
        let m = g.mirror_candidate.as_ref().expect("mirror twin");
        assert!((m.pos().x + c.pos().x).abs() < 1e-4 && m.mirrored);
    }

    #[test]
    fn campaign_cannot_afford_expensive_parts() {
        let mut b = Builder::default();
        b.design = PlaneDesign::empty();
        let profile = Profile::new_campaign();
        let jet = parts::part_or_panic("eng_jet");
        b.ghost = Some(Ghost { def: jet, k: 0, candidate: None, mirror_candidate: None, valid: false, reason: String::new() });
        b.hover = Some((0, Dir::NZ, Vec3::new(0.0, 0.0, -1.0)));
        compute_placement(&mut b, &profile);
        let g = b.ghost.as_ref().unwrap();
        assert!(!g.valid && g.reason.contains("Costs"));
    }

    #[test]
    fn undo_restores_design_and_money() {
        let mut b = Builder::default();
        b.design = PlaneDesign::empty();
        let mut profile = Profile::new_campaign();
        let money = profile.money;
        b.snapshot(&profile);
        let tank = parts::part_or_panic("tank_small");
        b.design.parts.push(PlacedPart::new(tank, Vec3::new(0.0, 0.0, 1.5), Quat::IDENTITY, false));
        profile.spend(tank.cost);
        assert_eq!(b.design.parts.len(), 2);
        undo(&mut b, &mut profile);
        assert_eq!(b.design.parts.len(), 1);
        assert_eq!(profile.money, money);
    }
}
