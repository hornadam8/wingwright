//! Visual representation of parts (shared by the builder and the flight scene).

use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use std::f32::consts::FRAC_PI_2;

use crate::design::PlacedPart;
use crate::meshes;
use crate::parts::{Control, Dir, PartKind, PALETTE};
use crate::MaterialCache;

/// Index of the part in the design, on the part's root entity.
#[derive(Component)]
pub struct PartIndex(pub usize);

/// Spinning propeller hub; rotated about local Z by the flight visuals.
#[derive(Component)]
pub struct Propeller;

/// Retractable gear part root: scaled down when the gear is up.
#[derive(Component)]
pub struct GearVisual {
    /// Current fold angle (radians about X at the strut top), 0 = down.
    pub angle: f32,
}

/// Parachute canopy, hidden until deployed.
#[derive(Component)]
pub struct ChuteCanopy;

/// Drag chute canopy, hidden until deployed.
#[derive(Component)]
pub struct DragChuteCanopy;

pub const WOOD: [f32; 3] = [0.56, 0.37, 0.21];
const DARK: [f32; 3] = [0.15, 0.15, 0.17];
const METAL: [f32; 3] = [0.6, 0.62, 0.65];
const GLASS: [f32; 3] = [0.6, 0.8, 0.95];
const RUBBER: [f32; 3] = [0.08, 0.08, 0.08];
const SKIN: [f32; 3] = [0.92, 0.74, 0.58];
const HAIR: [f32; 3] = [0.28, 0.18, 0.1];
const HINGE: [f32; 3] = [0.42, 0.36, 0.3];

/// Spawns the visual hierarchy for one placed part and returns its root entity.
/// Every entity gets `layers` so the builder can render on its own camera layer.
pub fn spawn_part_visual(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cache: &mut MaterialCache,
    placed: &PlacedPart,
    index: usize,
    layers: &RenderLayers,
) -> Entity {
    let def = placed.def();
    let dims = placed.dims();
    let (w, h, l) = (dims.x, dims.y, dims.z);
    let color = PALETTE[placed.color.min(PALETTE.len() - 1)].0;
    let body = cache.get(materials, color);
    let wood = cache.get(materials, WOOD);
    let dark = cache.get(materials, DARK);
    let metal = cache.get(materials, METAL);
    let rubber = cache.get(materials, RUBBER);
    let skin = cache.get(materials, SKIN);
    let hair = cache.get(materials, HAIR);
    let hinge = cache.get(materials, HINGE);
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgba(GLASS[0], GLASS[1], GLASS[2], 0.45),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.15,
        reflectance: 0.6,
        ..default()
    });

    let mirrored = placed.mirrored;
    // Tier-1 parts have the early-game wood-and-fabric look; researched parts look modern.
    let vintage = def.tier <= 1;
    let root = commands.spawn((placed.transform(), Visibility::default(), PartIndex(index), layers.clone())).id();
    let mut children: Vec<Entity> = Vec::new();
    let mut add = |commands: &mut Commands, mesh: Mesh, m: Handle<StandardMaterial>, tf: Transform| -> Entity {
        let (mesh, tf) = if mirrored { (meshes::mirror_x(mesh), meshes::mirror_transform(tf)) } else { (mesh, tf) };
        let e = commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(m), tf, layers.clone())).id();
        children.push(e);
        e
    };

    match def.kind {
        PartKind::Cockpit { .. } => {
            // Real nose shapes (Adam: "the cockpits don't look like plane cockpits"): prop
            // cockpits keep a firewall front for the engine; jets get an egg nose.
            let prof = meshes::rounded_rect(w, h, w.min(h) * 0.42, 6);
            let jet = matches!(def.id, "cockpit_fast" | "cockpit_airliner" | "cockpit_cargo_jet");
            let nose: Vec<(f32, f32)> = match def.id {
                "cockpit_fast" => vec![(-l * 0.5, 0.08), (-l * 0.5 + 0.35, 0.42), (-l * 0.5 + 0.9, 0.78), (-l * 0.5 + 1.6, 1.0), (l * 0.5, 1.0)],
                "cockpit_airliner" | "cockpit_cargo_jet" => vec![(-l * 0.5, 0.14), (-l * 0.5 + w * 0.3, 0.6), (-l * 0.5 + w * 0.75, 0.88), (-l * 0.5 + w * 1.4, 1.0), (l * 0.5, 1.0)],
                "cockpit_cabin" => vec![(-l * 0.5, 0.86), (-l * 0.5 + 0.3, 0.98), (-l * 0.5 + 0.6, 1.0), (l * 0.5, 1.0)],
                _ => vec![(-l * 0.5, 0.74), (-l * 0.5 + 0.35, 0.94), (-l * 0.5 + 0.8, 1.0), (l * 0.5, 1.0)],
            };
            let scale_at = |z: f32| -> f32 {
                let mut prev = nose[0];
                for &st in &nose {
                    if z <= st.0 {
                        let t = if st.0 > prev.0 { (z - prev.0) / (st.0 - prev.0) } else { 1.0 };
                        return prev.1 + (st.1 - prev.1) * t.clamp(0.0, 1.0);
                    }
                    prev = st;
                }
                prev.1
            };
            let sections: Vec<(f32, f32, f32)> = nose.iter().map(|(z, sc)| (*z, *sc, *sc)).collect();
            add(commands, meshes::loft(&prof, &sections, true, true), body.clone(), Transform::IDENTITY);
            // Where the canopy sits: further back on the pointed noses.
            let hz = if jet { -l * 0.5 + l * 0.62 } else { -l * 0.5 + l * 0.58 };
            let hw = w * 0.5;
            let hl = l * 0.4;
            let top_arc: Vec<Vec2> = prof.iter().copied().filter(|p| p.y > h * 0.1).collect();
            match def.id {
                "cockpit_basic" => {
                    // Open cockpit: a dark well with a wooden coaming and a small windscreen.
                    add(commands, meshes::rounded_box(hw, 0.34, hl, 0.45), dark.clone(), Transform::from_xyz(0.0, h * 0.5 - 0.03, hz));
                    for sgn in [-1.0, 1.0] {
                        add(commands, meshes::rounded_box(0.06, 0.07, hl + 0.06, 0.5), wood.clone(), Transform::from_xyz(sgn * (hw * 0.5 + 0.02), h * 0.5 + 0.02, hz));
                    }
                    add(commands, meshes::rounded_box(hw + 0.1, 0.07, 0.06, 0.5), wood.clone(), Transform::from_xyz(0.0, h * 0.5 + 0.02, hz + hl * 0.5 + 0.02));
                    add(
                        commands,
                        meshes::rounded_box(hw * 0.95, 0.24, 0.03, 0.3),
                        glass.clone(),
                        Transform { translation: Vec3::new(0.0, h * 0.5 + 0.13, hz - hl * 0.5 - 0.02), rotation: Quat::from_rotation_x(0.45), scale: Vec3::ONE },
                    );
                }
                "cockpit_closed" | "cockpit_fast" | "cockpit_light" => {
                    // Bubble canopy: a glass teardrop sunk into the top, with a dark frame at the
                    // front and a sill; the racer's is long and low.
                    let (cw, ch, cl) = if def.id == "cockpit_fast" { (w * 0.62, 0.34, l * 0.42) } else if def.id == "cockpit_light" { (w * 0.8, 0.5, l * 0.5) } else { (w * 0.72, 0.44, l * 0.46) };
                    let z0 = hz - cl * 0.5;
                    let canopy = meshes::loft(
                        &meshes::rounded_rect(cw, ch * 2.0, cw * 0.45, 6),
                        &[(z0, 0.15, 0.1), (z0 + cl * 0.25, 0.8, 0.85), (z0 + cl * 0.55, 1.0, 1.0), (z0 + cl * 0.85, 0.85, 0.75), (z0 + cl, 0.5, 0.2)],
                        true,
                        true,
                    );
                    add(commands, canopy, glass.clone(), Transform::from_xyz(0.0, h * 0.5 - 0.06, 0.0));
                    add(commands, meshes::rounded_box(cw * 0.96, 0.05, cl * 0.98, 0.4), dark.clone(), Transform::from_xyz(0.0, h * 0.5 - 0.04, hz));
                    add(commands, meshes::rounded_box(cw * 1.0, ch * 0.9, 0.05, 0.4), dark.clone(), Transform::from_xyz(0.0, h * 0.5 + ch * 0.25, z0 + cl * 0.28));
                    if def.id == "cockpit_light" {
                        // Cabin: side windows behind the canopy.
                        for sgn in [-1.0, 1.0] {
                            add(commands, meshes::rounded_box(0.03, h * 0.28, l * 0.16, 0.3), glass.clone(), Transform::from_xyz(sgn * (w * 0.5 - 0.005), h * 0.12, hz + cl * 0.55));
                        }
                    }
                }
                "cockpit_cabin" => {
                    // Utility cabin: raised roof, a big raked windscreen and square side windows.
                    add(commands, meshes::rounded_box(w * 0.9, 0.36, l * 0.55, 0.35), body.clone(), Transform::from_xyz(0.0, h * 0.5 + 0.1, hz + l * 0.04));
                    add(
                        commands,
                        meshes::rounded_box(w * 0.86, 0.36, 0.04, 0.2),
                        glass.clone(),
                        Transform { translation: Vec3::new(0.0, h * 0.5 + 0.14, hz - l * 0.24), rotation: Quat::from_rotation_x(0.55), scale: Vec3::ONE },
                    );
                    for sgn in [-1.0, 1.0] {
                        for k in 0..2 {
                            add(commands, meshes::rounded_box(0.03, 0.26, l * 0.16, 0.2), glass.clone(), Transform::from_xyz(sgn * (w * 0.45 - 0.005), h * 0.5 + 0.12, hz - l * 0.06 + k as f32 * l * 0.2));
                        }
                    }
                    add(commands, meshes::rounded_box(w * 0.92, 0.04, l * 0.58, 0.3), dark.clone(), Transform::from_xyz(0.0, h * 0.5 + 0.3, hz + l * 0.04));
                }
                _ => {
                    // Jet nose: a dark window band wrapping the egg where the flight deck is,
                    // with a glass strip over the top half (the cargo jet also gets a hump).
                    let zb = -l * 0.5 + w * 0.62;
                    let sb = scale_at(zb) * 1.03;
                    add(commands, meshes::loft(&prof, &[(zb - 0.12, sb, sb), (zb + 0.12, sb, sb)], true, true), dark.clone(), Transform::IDENTITY);
                    let sg = scale_at(zb) * 1.05;
                    add(commands, meshes::loft(&top_arc, &[(zb - 0.09, sg, sg), (zb + 0.09, sg, sg)], false, false), glass.clone(), Transform::IDENTITY);
                    if def.id == "cockpit_cargo_jet" {
                        let hump = meshes::loft(&meshes::rounded_rect(w * 0.7, h * 0.5, w * 0.2, 6), &[(zb + 0.2, 0.3, 0.2), (zb + 0.8, 1.0, 1.0), (l * 0.5, 1.0, 1.0)], true, true);
                        add(commands, hump, body.clone(), Transform::from_xyz(0.0, h * 0.45, 0.0));
                    }
                }
            }
            // Pilot (only where a head would be seen).
            if !jet || def.id == "cockpit_fast" {
                let head = Sphere::new(0.13).mesh().ico(3).unwrap();
                let py = h * 0.5 + 0.1;
                add(commands, head, skin.clone(), Transform::from_xyz(0.0, py, hz + 0.05));
                add(commands, meshes::dome(0.145, 6, 14), hair.clone(), Transform::from_xyz(0.0, py + 0.01, hz + 0.05));
                add(commands, meshes::rounded_box(0.4, 0.16, 0.24, 0.45), hair.clone(), Transform::from_xyz(0.0, h * 0.5 - 0.06, hz + 0.05));
            }
            // Firewall plate for the engine to sit against (prop cockpits only).
            if !jet {
                let sf = scale_at(-l * 0.5);
                add(commands, meshes::loft(&prof, &[(-l * 0.5 - 0.02, sf * 0.96, sf * 0.96), (-l * 0.5 + 0.03, sf * 0.96, sf * 0.96)], true, true), dark.clone(), Transform::IDENTITY);
            }
        }
        PartKind::Fuselage { seats_per_m, .. } => {
            add(commands, meshes::rounded_box(w, h, l, 0.5), body.clone(), Transform::IDENTITY);
            if seats_per_m > 0.0 {
                let n = ((l / 0.6).floor() as i32).max(1);
                for s in [-1.0, 1.0] {
                    for k in 0..n {
                        let z = -l * 0.5 + (k as f32 + 0.5) * (l / n as f32);
                        add(commands, meshes::cylinder_x(h * 0.11, 0.04, 16), glass.clone(), Transform::from_xyz(s * (w * 0.5 - 0.005), h * 0.15, z));
                    }
                }
            } else {
                add(commands, meshes::rounded_box(0.02, h * 0.5, (l * 0.55).min(1.5), 0.3), hinge.clone(), Transform::from_xyz(w * 0.5 - 0.0, -h * 0.05, 0.0));
            }
        }
        PartKind::Cone { tip_scale, .. } => {
            let tip_sign = if def.attach.contains(&Dir::PZ) { -1.0 } else { 1.0 };
            add(commands, meshes::cone_body(w, h, l, tip_scale, tip_sign), body.clone(), Transform::IDENTITY);
        }
        PartKind::Boom | PartKind::Hub => {
            add(commands, meshes::rounded_box(w, h, l, 0.45), body.clone(), Transform::IDENTITY);
        }
        PartKind::Wing { control, vertical, centered, .. } => {
            let mesh = if vertical {
                if vintage { meshes::fin_mesh(h, w, l, 0.6) } else { meshes::wing_mesh(h, w, l, 0.55).rotated_by(Quat::from_rotation_z(FRAC_PI_2)) }
            } else if centered {
                if vintage { meshes::symmetric_wing(w, h, l, 0.75) } else { meshes::symmetric_wing_smooth(w, h, l, 0.7) }
            } else if control == Control::Aileron {
                if vintage { meshes::ribbed_wing(w, h, l, 1.0) } else { meshes::wing_mesh(w, h, l, 1.0) }
            } else if vintage {
                meshes::ribbed_wing(w, h, l, 0.72)
            } else {
                meshes::wing_mesh(w, h, l, 0.62)
            };
            add(commands, mesh, body.clone(), Transform::IDENTITY);
            if !vintage {
                // Modern: a painted stripe near the tip and a dark hinge line for control surfaces.
                if control == Control::None {
                    add(commands, meshes::rounded_box(0.14, h * 1.06, l * 0.62, 0.5), cache.get(materials, PALETTE[1].0), Transform::from_xyz(w * 0.5 - 0.35, 0.0, l * 0.02));
                } else if !vertical {
                    add(commands, meshes::rounded_box(w * 0.92, h * 1.15, 0.02, 0.5), hinge.clone(), Transform::from_xyz(0.0, 0.0, l * 0.22));
                } else {
                    add(commands, meshes::rounded_box(w * 1.15, h * 0.85, 0.02, 0.5), hinge.clone(), Transform::from_xyz(0.0, 0.0, l * 0.22));
                }
            } else if control == Control::Aileron {
                // Wooden hinge along the front edge.
                add(commands, meshes::rounded_box(w * 0.98, h * 1.3, 0.05, 0.5), wood.clone(), Transform::from_xyz(0.0, 0.0, -l * 0.5 + 0.015));
            } else if vertical {
                add(commands, meshes::rounded_box(w * 1.3, h * 0.9, 0.05, 0.5), wood.clone(), Transform::from_xyz(0.0, 0.0, -l * 0.5 + 0.015));
                add(commands, meshes::rounded_box(w * 1.15, h * 0.85, 0.02, 0.5), hinge.clone(), Transform::from_xyz(0.0, 0.0, l * 0.22));
            } else {
                add(commands, meshes::rounded_box(w * 0.96, h * 1.3, 0.05, 0.5), wood.clone(), Transform::from_xyz(0.0, 0.0, -l * 0.5 + 0.015));
                if matches!(control, Control::Elevator | Control::Tail) {
                    add(commands, meshes::rounded_box(w * 0.9, h * 1.15, 0.02, 0.5), hinge.clone(), Transform::from_xyz(0.0, 0.0, l * 0.22));
                }
            }
        }
        PartKind::Engine { prop, electric, .. } => {
            let r = w.min(h) * 0.5;
            if prop {
                let cowl = meshes::spindle(&[(-l * 0.5, r * 0.8), (-l * 0.3, r * 0.97), (0.0, r), (l * 0.3, r * 0.98), (l * 0.5, r * 0.9)], 26);
                add(commands, cowl, body.clone(), Transform::IDENTITY);
                // Cylinder-head ring (or a blue band for electric motors).
                let band = cache.get(materials, if electric { [0.25, 0.5, 0.85] } else { DARK });
                add(commands, meshes::cylinder_z(r * 1.01, l * 0.16, 26), band, Transform::from_xyz(0.0, 0.0, -l * 0.5 + l * 0.22));
                if def.id == "eng_radial" {
                    // Radial: a ring of cylinder heads around a dark crankcase behind the prop.
                    add(commands, meshes::cylinder_z(r * 0.92, 0.1, 26), dark.clone(), Transform::from_xyz(0.0, 0.0, -l * 0.5 + 0.02));
                    for k in 0..9 {
                        let ang = k as f32 * std::f32::consts::TAU / 9.0;
                        add(commands, meshes::cylinder_z(r * 0.11, 0.16, 10).rotated_by(Quat::from_rotation_x(FRAC_PI_2) * Quat::from_rotation_z(ang)), metal.clone(), Transform::from_xyz(ang.cos() * r * 0.66, ang.sin() * r * 0.66, -l * 0.5 + 0.1));
                        add(commands, meshes::cylinder_z(r * 0.13, 0.04, 10), dark.clone(), Transform::from_xyz(ang.cos() * r * 0.66, ang.sin() * r * 0.66, -l * 0.5 + 0.06));
                    }
                }
                // Wooden two-blade propeller (vintage) or a spinner with metal blades (modern).
                let hub_tf = Transform::from_xyz(0.0, 0.0, -l * 0.5 - 0.03);
                let blade_mat = if vintage { wood.clone() } else { dark.clone() };
                let hub = commands
                    .spawn((if mirrored { meshes::mirror_transform(hub_tf) } else { hub_tf }, Visibility::default(), Propeller, layers.clone()))
                    .with_children(|p| {
                        if vintage {
                            p.spawn((Mesh3d(meshes.add(meshes::spindle(&[(-0.14, 0.02), (-0.08, r * 0.16), (0.0, r * 0.2)], 14))), MeshMaterial3d(dark.clone()), Transform::IDENTITY, layers.clone()));
                        } else {
                            p.spawn((Mesh3d(meshes.add(meshes::spinner(r * 0.3, (r * 0.75).max(0.2)))), MeshMaterial3d(metal.clone()), Transform::IDENTITY, layers.clone()));
                        }
                        let blade_len = (r * 2.7).max(0.9);
                        let n = match def.id {
                            "eng_small" | "eng_electric" => 2,
                            "eng_metal_prop" | "eng_radial" => 3,
                            _ => 4,
                        };
                        let chord = (r * 0.32).max(0.13);
                        let red = cache.get(materials, [0.85, 0.15, 0.15]);
                        for k in 0..n {
                            let ang = k as f32 * std::f32::consts::TAU / n as f32;
                            p.spawn((
                                Mesh3d(meshes.add(meshes::wing_mesh(blade_len, 0.035, chord, 0.75))),
                                MeshMaterial3d(blade_mat.clone()),
                                Transform {
                                    translation: Vec3::new(-ang.sin() * blade_len * 0.5, ang.cos() * blade_len * 0.5, -0.06),
                                    rotation: Quat::from_rotation_z(ang + FRAC_PI_2) * Quat::from_rotation_x(0.5),
                                    scale: Vec3::ONE,
                                },
                                layers.clone(),
                            ))
                            .with_children(|blade| {
                                if !vintage {
                                    blade.spawn((
                                        Mesh3d(meshes.add(meshes::rounded_box(0.12, 0.045, chord * 0.7, 0.4))),
                                        MeshMaterial3d(red.clone()),
                                        Transform::from_xyz(blade_len * 0.5 - 0.09, 0.0, 0.0),
                                        layers.clone(),
                                    ));
                                }
                            });
                        }
                    })
                    .id();
                children.push(hub);
            } else if def.id == "eng_ramjet" {
                // Ramjet: a long plain tube, a shock-cone spike in the intake, a flared nozzle
                // with a hot lip and three small fins. Nothing spins.
                let orange = cache.get(materials, [0.95, 0.45, 0.1]);
                add(commands, meshes::spindle(&[(-l * 0.5, r * 0.72), (-l * 0.42, r * 0.8), (l * 0.1, r * 0.8), (l * 0.32, r * 0.7), (l * 0.5, r * 0.9)], 28), metal.clone(), Transform::IDENTITY);
                add(commands, meshes::spindle(&[(-l * 0.5 - 0.02, r * 0.74), (-l * 0.5 + 0.05, r * 0.64)], 28), dark.clone(), Transform::IDENTITY);
                add(commands, meshes::spindle(&[(-l * 0.5 - r * 0.9, 0.01), (-l * 0.5 + 0.1, r * 0.42), (-l * 0.5 + 0.5, r * 0.3)], 22), dark.clone(), Transform::IDENTITY);
                add(commands, meshes::spindle(&[(l * 0.38, r * 0.66), (l * 0.5 + 0.01, r * 0.86)], 24), orange, Transform::IDENTITY);
                for k in 0..3 {
                    let ang = k as f32 * std::f32::consts::TAU / 3.0 + FRAC_PI_2;
                    add(commands, meshes::rounded_box(0.03, r * 0.5, l * 0.22, 0.2), metal.clone(), Transform { translation: Vec3::new(ang.cos() * r * 0.95, ang.sin() * r * 0.95, l * 0.3), rotation: Quat::from_rotation_z(ang - FRAC_PI_2), scale: Vec3::ONE });
                }
            } else if def.id == "eng_jet" {
                // Big turbofan: fat short nacelle, a wide intake with the fan and spinner
                // visible, a core exhaust cone sticking out the back.
                add(commands, meshes::spindle(&[(-l * 0.5, r * 0.94), (-l * 0.35, r * 1.0), (l * 0.1, r * 0.98), (l * 0.42, r * 0.78), (l * 0.5, r * 0.7)], 30), metal.clone(), Transform::IDENTITY);
                add(commands, meshes::spindle(&[(-l * 0.5 - 0.03, r * 0.9), (-l * 0.5 + 0.06, r * 0.98)], 30), dark.clone(), Transform::IDENTITY);
                add(commands, meshes::cylinder_z(r * 0.86, 0.06, 30), dark.clone(), Transform::from_xyz(0.0, 0.0, -l * 0.5 + 0.12));
                add(commands, meshes::spindle(&[(l * 0.2, r * 0.42), (l * 0.5 + 0.18, r * 0.2)], 20), dark.clone(), Transform::IDENTITY);
                let hub_tf = Transform::from_xyz(0.0, 0.0, -l * 0.5 + 0.08);
                let hub = commands
                    .spawn((if mirrored { meshes::mirror_transform(hub_tf) } else { hub_tf }, Visibility::default(), Propeller, layers.clone()))
                    .with_children(|p| {
                        p.spawn((Mesh3d(meshes.add(meshes::spinner(r * 0.22, r * 0.4))), MeshMaterial3d(metal.clone()), Transform::IDENTITY, layers.clone()));
                        for k in 0..12 {
                            let ang = k as f32 * std::f32::consts::TAU / 12.0;
                            p.spawn((
                                Mesh3d(meshes.add(meshes::rounded_box(r * 0.16, r * 0.62, 0.02, 0.2))),
                                MeshMaterial3d(metal.clone()),
                                Transform { translation: Vec3::new(-ang.sin() * r * 0.52, ang.cos() * r * 0.52, 0.0), rotation: Quat::from_rotation_z(ang) * Quat::from_rotation_y(0.6), scale: Vec3::ONE },
                                layers.clone(),
                            ));
                        }
                    })
                    .id();
                children.push(hub);
            } else if def.id == "eng_fan" {
                // Ducted fan: a short ring with a six-blade fan spinning inside.
                add(commands, meshes::loft(&meshes::circle(1.0, 28), &[(-l * 0.5, r, r), (-l * 0.4, r * 1.04, r * 1.04), (l * 0.4, r * 1.02, r * 1.02), (l * 0.5, r * 0.94, r * 0.94)], true, false), metal.clone(), Transform::IDENTITY);
                add(commands, meshes::loft(&meshes::circle(1.0, 28), &[(-l * 0.5, r * 0.9, r * 0.9), (l * 0.5, r * 0.86, r * 0.86)], true, false), dark.clone(), Transform::IDENTITY);
                let hub_tf = Transform::IDENTITY;
                let hub = commands
                    .spawn((hub_tf, Visibility::default(), Propeller, layers.clone()))
                    .with_children(|p| {
                        p.spawn((Mesh3d(meshes.add(meshes::spinner(r * 0.18, r * 0.3))), MeshMaterial3d(dark.clone()), Transform::from_xyz(0.0, 0.0, -0.05), layers.clone()));
                        for k in 0..6 {
                            let ang = k as f32 * std::f32::consts::TAU / 6.0;
                            p.spawn((
                                Mesh3d(meshes.add(meshes::rounded_box(r * 0.2, r * 0.7, 0.025, 0.2))),
                                MeshMaterial3d(metal.clone()),
                                Transform { translation: Vec3::new(-ang.sin() * r * 0.5, ang.cos() * r * 0.5, 0.0), rotation: Quat::from_rotation_z(ang) * Quat::from_rotation_y(0.7), scale: Vec3::ONE },
                                layers.clone(),
                            ));
                        }
                    })
                    .id();
                children.push(hub);
            } else {
                // Small jet: slim pod with a compressor face in the intake and a long tail cone.
                add(commands, meshes::spindle(&[(-l * 0.5, r * 0.86), (-l * 0.3, r), (l * 0.25, r), (l * 0.5, r * 0.66)], 28), metal.clone(), Transform::IDENTITY);
                add(commands, meshes::spindle(&[(-l * 0.5 - 0.02, r * 0.82), (-l * 0.5 + 0.05, r * 0.9)], 28), dark.clone(), Transform::IDENTITY);
                add(commands, meshes::cylinder_z(r * 0.78, 0.05, 24), dark.clone(), Transform::from_xyz(0.0, 0.0, -l * 0.5 + 0.09));
                add(commands, meshes::spinner(r * 0.18, r * 0.3), metal.clone(), Transform::from_xyz(0.0, 0.0, -l * 0.5 + 0.06));
                add(commands, meshes::spindle(&[(l * 0.1, r * 0.4), (l * 0.5 + 0.12, r * 0.55)], 20), dark.clone(), Transform::IDENTITY);
            }
        }
        PartKind::Wheel { retractable, .. } => {
            // Each wheel has its own build (Adam: "wheels all look the same"), and all of the
            // geometry hangs under a pivot at the strut top so retractable gear can fold up.
            let wheel_r = (l * 0.5).min(h * 0.4);
            let strut_h = h - wheel_r;
            let top = h * 0.5;
            let pivot_tf = Transform::from_xyz(0.0, top, 0.0);
            let pivot = commands.spawn((if mirrored { meshes::mirror_transform(pivot_tf) } else { pivot_tf }, Visibility::default(), layers.clone())).id();
            if retractable {
                commands.entity(pivot).insert(GearVisual { angle: 0.0 });
            }
            let mut sub: Vec<Entity> = Vec::new();
            let mut addp = |commands: &mut Commands, mesh: Mesh, m: Handle<StandardMaterial>, tf: Transform| {
                // Local to the pivot: y measured down from the strut top.
                let tf = Transform { translation: tf.translation - Vec3::new(0.0, top, 0.0), ..tf };
                let (mesh, tf) = if mirrored { (meshes::mirror_x(mesh), meshes::mirror_transform(tf)) } else { (mesh, tf) };
                let e = commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(m), tf, layers.clone())).id();
                sub.push(e);
            };
            let wy = -h * 0.5 + wheel_r;
            let tyre = |r: f32, width: f32| meshes::cylinder_x(r, width, 26);
            match def.id {
                "wheel_fixed" => {
                    // Wire wheel on a V of two struts with a small axle fairing.
                    for sgn in [-1.0, 1.0] {
                        addp(commands, meshes::cylinder_z(0.025, strut_h * 1.05, 8).rotated_by(Quat::from_rotation_x(FRAC_PI_2) * Quat::from_rotation_z(sgn * 0.35)), metal.clone(), Transform::from_xyz(0.0, top - strut_h * 0.5, sgn * l * 0.12));
                    }
                    addp(commands, tyre(wheel_r, w * 0.5), rubber.clone(), Transform::from_xyz(0.0, wy, 0.0));
                    addp(commands, meshes::cylinder_x(wheel_r * 0.3, w * 0.56, 12), body.clone(), Transform::from_xyz(0.0, wy, 0.0));
                    for k in 0..8 {
                        let ang = k as f32 * std::f32::consts::PI / 8.0;
                        addp(commands, meshes::rounded_box(w * 0.12, wheel_r * 1.7, 0.02, 0.2), metal.clone(), Transform { translation: Vec3::new(0.0, wy, 0.0), rotation: Quat::from_rotation_x(ang), scale: Vec3::ONE });
                    }
                    addp(commands, meshes::cylinder_x(wheel_r * 0.14, w * 0.8, 10), dark.clone(), Transform::from_xyz(0.0, wy, 0.0));
                }
                "wheel_short" => {
                    // Stubby: a thick short leg, a small solid wheel with a hubcap.
                    addp(commands, meshes::cylinder_z(w * 0.2, strut_h, 12).rotated_by(Quat::from_rotation_x(FRAC_PI_2)), metal.clone(), Transform::from_xyz(0.0, top - strut_h * 0.5, 0.0));
                    addp(commands, meshes::rounded_box(w * 0.7, 0.08, l * 0.5, 0.45), body.clone(), Transform::from_xyz(0.0, top - 0.04, 0.0));
                    addp(commands, tyre(wheel_r, w * 0.7), rubber.clone(), Transform::from_xyz(0.0, wy, 0.0));
                    addp(commands, meshes::cylinder_x(wheel_r * 0.6, w * 0.76, 18), metal.clone(), Transform::from_xyz(0.0, wy, 0.0));
                }
                "wheel_tall" => {
                    // Oleo leg: a fat upper cylinder, thin polished lower, and a drag brace.
                    addp(commands, meshes::cylinder_z(w * 0.17, strut_h * 0.55, 12).rotated_by(Quat::from_rotation_x(FRAC_PI_2)), body.clone(), Transform::from_xyz(0.0, top - strut_h * 0.28, 0.0));
                    addp(commands, meshes::cylinder_z(w * 0.09, strut_h * 0.5, 10).rotated_by(Quat::from_rotation_x(FRAC_PI_2)), metal.clone(), Transform::from_xyz(0.0, top - strut_h * 0.75, 0.0));
                    addp(commands, meshes::cylinder_z(0.03, strut_h * 0.7, 8).rotated_by(Quat::from_rotation_x(FRAC_PI_2 - 0.5)), metal.clone(), Transform::from_xyz(0.0, top - strut_h * 0.5, l * 0.18));
                    addp(commands, tyre(wheel_r, w * 0.6), rubber.clone(), Transform::from_xyz(0.0, wy, 0.0));
                    addp(commands, meshes::cylinder_x(wheel_r * 0.5, w * 0.66, 18), metal.clone(), Transform::from_xyz(0.0, wy, 0.0));
                    addp(commands, meshes::cylinder_x(wheel_r * 0.16, w * 0.8, 10), dark.clone(), Transform::from_xyz(0.0, wy, 0.0));
                }
                "wheel_tail" => {
                    // Castor: a bent leaf spring and a fork holding a tiny wheel.
                    addp(commands, meshes::rounded_box(w * 0.35, 0.05, l * 0.9, 0.3), metal.clone(), Transform { translation: Vec3::new(0.0, top - strut_h * 0.45, l * 0.1), rotation: Quat::from_rotation_x(-0.55), scale: Vec3::ONE });
                    for sgn in [-1.0, 1.0] {
                        addp(commands, meshes::rounded_box(0.02, wheel_r * 1.6, wheel_r * 1.2, 0.3), metal.clone(), Transform::from_xyz(sgn * w * 0.32, wy + wheel_r * 0.3, 0.0));
                    }
                    addp(commands, tyre(wheel_r * 0.9, w * 0.45), rubber.clone(), Transform::from_xyz(0.0, wy, 0.0));
                    addp(commands, meshes::cylinder_x(wheel_r * 0.35, w * 0.5, 12), metal.clone(), Transform::from_xyz(0.0, wy, 0.0));
                }
                "wheel_tough" => {
                    // Heavy: twin tyres on a thick leg with a torque link.
                    addp(commands, meshes::cylinder_z(w * 0.16, strut_h, 14).rotated_by(Quat::from_rotation_x(FRAC_PI_2)), metal.clone(), Transform::from_xyz(0.0, top - strut_h * 0.5, 0.0));
                    addp(commands, meshes::rounded_box(w * 0.5, 0.1, l * 0.5, 0.45), body.clone(), Transform::from_xyz(0.0, top - 0.05, 0.0));
                    addp(commands, meshes::rounded_box(0.06, strut_h * 0.4, 0.14, 0.3), dark.clone(), Transform::from_xyz(0.0, top - strut_h * 0.6, -l * 0.18));
                    for sgn in [-1.0, 1.0] {
                        addp(commands, tyre(wheel_r, w * 0.36), rubber.clone(), Transform::from_xyz(sgn * w * 0.24, wy, 0.0));
                        addp(commands, meshes::cylinder_x(wheel_r * 0.55, w * 0.4, 16), metal.clone(), Transform::from_xyz(sgn * w * 0.24, wy, 0.0));
                    }
                    addp(commands, meshes::cylinder_x(wheel_r * 0.15, w * 1.0, 10), dark.clone(), Transform::from_xyz(0.0, wy, 0.0));
                }
                _ => {
                    // Retractable: oleo leg, a gear door on the strut, single wheel.
                    addp(commands, meshes::cylinder_z(w * 0.14, strut_h * 0.6, 12).rotated_by(Quat::from_rotation_x(FRAC_PI_2)), metal.clone(), Transform::from_xyz(0.0, top - strut_h * 0.3, 0.0));
                    addp(commands, meshes::cylinder_z(w * 0.08, strut_h * 0.5, 10).rotated_by(Quat::from_rotation_x(FRAC_PI_2)), metal.clone(), Transform::from_xyz(0.0, top - strut_h * 0.75, 0.0));
                    addp(commands, meshes::rounded_box(0.03, strut_h * 0.95, l * 0.6, 0.3), body.clone(), Transform::from_xyz(w * 0.3, top - strut_h * 0.5, 0.0));
                    addp(commands, tyre(wheel_r, w * 0.55), rubber.clone(), Transform::from_xyz(0.0, wy, 0.0));
                    addp(commands, meshes::cylinder_x(wheel_r * 0.5, w * 0.6, 18), metal.clone(), Transform::from_xyz(0.0, wy, 0.0));
                    addp(commands, meshes::cylinder_x(wheel_r * 0.15, w * 0.7, 10), dark.clone(), Transform::from_xyz(0.0, wy, 0.0));
                }
            }
            commands.entity(pivot).add_children(&sub);
            children.push(pivot);
        }
        PartKind::Tank { external, .. } => {
            if external {
                let r = w.min(h) * 0.5;
                if def.id == "eng_rocket" {
                    // Rocket: squat tank body, narrow throat, flared bell at the back.
                    add(commands, meshes::spindle(&[(-l * 0.5, r * 0.55), (-l * 0.42, r * 0.85), (-l * 0.05, r * 0.85), (l * 0.05, r * 0.4)], 22), metal.clone(), Transform::IDENTITY);
                    add(commands, meshes::spindle(&[(l * 0.02, r * 0.3), (l * 0.2, r * 0.45), (l * 0.38, r * 0.8), (l * 0.5, r * 1.0)], 22), dark.clone(), Transform::IDENTITY);
                    add(commands, meshes::cylinder_z(r * 0.9, l * 0.06, 22), cache.get(materials, [0.85, 0.15, 0.15]), Transform::from_xyz(0.0, 0.0, -l * 0.44));
                } else {
                    add(commands, meshes::spindle(&[(-l * 0.5, r * 0.12), (-l * 0.36, r * 0.75), (-l * 0.1, r), (l * 0.15, r), (l * 0.4, r * 0.6), (l * 0.5, r * 0.1)], 22), body.clone(), Transform::IDENTITY);
                }
                add(commands, meshes::rounded_box(w * 0.25, h * 0.35, l * 0.3, 0.3), metal.clone(), Transform::from_xyz(0.0, h * 0.4, 0.0));
            } else {
                // Internal tank: a long slim cylinder with rounded ends and two metal straps.
                let r = w.min(h) * 0.5;
                add(commands, meshes::spindle(&[(-l * 0.5, r * 0.55), (-l * 0.5 + 0.12, r), (l * 0.5 - 0.12, r), (l * 0.5, r * 0.55)], 20), body.clone(), Transform::IDENTITY);
                for z in [-l * 0.3, l * 0.3] {
                    add(commands, meshes::cylinder_z(r * 1.04, 0.06, 20), metal.clone(), Transform::from_xyz(0.0, 0.0, z));
                }
                add(commands, meshes::cylinder_z(w * 0.12, 0.08, 12).rotated_by(Quat::from_rotation_x(FRAC_PI_2)), metal.clone(), Transform::from_xyz(w * 0.25, h * 0.5 + 0.03, 0.0));
            }
        }
        PartKind::Battery { .. } => {
            add(commands, meshes::rounded_box(w, h, l, 0.2), body.clone(), Transform::IDENTITY);
            add(commands, meshes::rounded_box(w * 0.3, h * 1.02, l * 1.02, 0.4), dark.clone(), Transform::IDENTITY);
        }
        PartKind::Parachute => {
            add(commands, meshes::rounded_box(w, h, l, 0.35), body.clone(), Transform::IDENTITY);
            let canopy = commands
                .spawn((
                    Mesh3d(meshes.add(meshes::dome(6.0, 8, 24))),
                    MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.95, 0.55, 0.1), cull_mode: None, double_sided: true, perceptual_roughness: 0.9, ..default() })),
                    Transform::from_xyz(0.0, 9.0, 0.0),
                    Visibility::Hidden,
                    ChuteCanopy,
                    layers.clone(),
                ))
                .id();
            children.push(canopy);
        }
        PartKind::DragChute => {
            add(commands, meshes::rounded_box(w, h, l, 0.35), body.clone(), Transform::IDENTITY);
            let canopy = commands
                .spawn((
                    Mesh3d(meshes.add(meshes::dome(2.5, 6, 18))),
                    MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.9, 0.9, 0.9), cull_mode: None, double_sided: true, perceptual_roughness: 0.9, ..default() })),
                    Transform { translation: Vec3::new(0.0, 0.0, 6.0), rotation: Quat::from_rotation_x(FRAC_PI_2), scale: Vec3::ONE },
                    Visibility::Hidden,
                    DragChuteCanopy,
                    layers.clone(),
                ))
                .id();
            children.push(canopy);
        }
    }
    commands.entity(root).add_children(&children);
    root
}
