//! Settlements around the airports: the street plan (painted as dirt roads and levelled by
//! the terrain generator) and the buildings, vehicles, docks and props that fill it.
//!
//! Everything is built from a handful of primitives on `Ctx` (box, roofed house, cylinder,
//! fence, car, boat, pier ...) so each airport kind is a short recipe. Solid pieces register
//! with the obstacle set through `spawn_box` / `Ctx::solid_cylinder`.

use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::meshes;
use crate::world::{spawn_box, spawn_palm, spawn_tree, Airport, AirportKind, Rng, SceneryLib, Terrain, WorldEntity};

/// Street segments (world xz) and pads flattened to the airport's elevation.
pub struct Plan {
    pub streets: Vec<(Vec2, Vec2)>,
    /// (centre, half size, yaw): rectangles levelled to the runway height.
    pub pads: Vec<(Vec2, Vec2, f32)>,
}

/// A block grid on the apron side of the runway: `cols` blocks along the runway (u) and
/// `rows` across it (v), starting at (u0, v0).
#[derive(Clone, Copy)]
struct Grid {
    u0: f32,
    v0: f32,
    cols: usize,
    rows: usize,
    pitch_u: f32,
    pitch_v: f32,
}

impl Grid {
    fn streets(&self, a: &Airport) -> Vec<(Vec2, Vec2)> {
        let mut v = Vec::new();
        let (u1, v1) = (self.u0 + self.cols as f32 * self.pitch_u, self.v0 + self.rows as f32 * self.pitch_v);
        for r in 0..=self.rows {
            let vv = self.v0 + r as f32 * self.pitch_v;
            v.push((a.world(self.u0, vv), a.world(u1, vv)));
        }
        for c in 0..=self.cols {
            let uu = self.u0 + c as f32 * self.pitch_u;
            v.push((a.world(uu, self.v0), a.world(uu, v1)));
        }
        v
    }

    fn pad(&self, a: &Airport) -> (Vec2, Vec2, f32) {
        let hu = self.cols as f32 * self.pitch_u * 0.5 + 40.0;
        let hv = self.rows as f32 * self.pitch_v * 0.5 + 40.0;
        // Pads are given in the box convention: half.x across (v), half.y along (u).
        (a.world(self.u0 + hu - 40.0, self.v0 + hv - 40.0), Vec2::new(hv, hu), a.yaw())
    }
}

fn grid_for(a: &Airport) -> Option<Grid> {
    let l = a.def.length;
    let w = a.def.width;
    let v0 = w * 0.5 + 130.0;
    Some(match a.def.kind {
        AirportKind::Home => Grid { u0: -l * 0.5 + 60.0, v0, cols: 5, rows: 3, pitch_u: 58.0, pitch_v: 46.0 },
        AirportKind::Town => Grid { u0: -l * 0.5 + 40.0, v0, cols: 7, rows: 4, pitch_u: 56.0, pitch_v: 46.0 },
        AirportKind::Village => Grid { u0: -l * 0.5 + 80.0, v0, cols: 4, rows: 2, pitch_u: 54.0, pitch_v: 44.0 },
        AirportKind::Hospital => Grid { u0: l * 0.5 - 260.0, v0: v0 + 20.0, cols: 3, rows: 1, pitch_u: 56.0, pitch_v: 46.0 },
        AirportKind::Base => Grid { u0: -l * 0.5 + 40.0, v0: v0 + 10.0, cols: 4, rows: 2, pitch_u: 70.0, pitch_v: 56.0 },
        AirportKind::Resort => Grid { u0: -l * 0.5 + 120.0, v0, cols: 3, rows: 1, pitch_u: 60.0, pitch_v: 50.0 },
        AirportKind::Mine | AirportKind::LumberCamp => Grid { u0: -l * 0.5 + 60.0, v0: v0 + 40.0, cols: 3, rows: 1, pitch_u: 56.0, pitch_v: 44.0 },
        // The city wraps the whole strip: blocks over the runway and its clear zones are
        // skipped when filling (see `city_keep_out`).
        AirportKind::City => Grid { u0: -l * 0.5 - 380.0, v0: -(w * 0.5 + 100.0) - 3.0 * 60.0, cols: 20, rows: 11, pitch_u: 64.0, pitch_v: 60.0 },
        _ => return None,
    })
}

/// Streets and level pads for an airport. Deterministic: called by the terrain generator
/// (to paint and flatten) and again when the scenery is spawned.
pub fn plan(a: &Airport) -> Plan {
    let l = a.def.length;
    let w = a.def.width;
    let mut streets = Vec::new();
    let mut pads = Vec::new();
    // Every airport gets a level strip beside the apron for its buildings (the gorge
    // strip only has room for a narrow shelf).
    if a.def.kind == AirportKind::Canyon {
        pads.push((a.world(0.0, w * 0.5 + 70.0), Vec2::new(60.0, l * 0.5 + 40.0), a.yaw()));
    } else {
        pads.push((a.world(0.0, w * 0.5 + 150.0), Vec2::new(120.0, l * 0.5 + 40.0), a.yaw()));
    }
    // A lane from the apron to the settlement.
    streets.push((a.world(-l * 0.5 + 140.0, w * 0.5 + 80.0), a.world(-l * 0.5 + 140.0, w * 0.5 + 130.0)));
    if let Some(g) = grid_for(a) {
        streets.extend(g.streets(a));
        pads.push(g.pad(a));
    }
    match a.def.kind {
        AirportKind::Farm => {
            // Farm track past the barns and along the fields.
            streets.push((a.world(-l * 0.5 + 140.0, w * 0.5 + 130.0), a.world(l * 0.3, w * 0.5 + 130.0)));
            streets.push((a.world(l * 0.3, w * 0.5 + 130.0), a.world(l * 0.3, w * 0.5 + 420.0)));
        }
        AirportKind::Prison => {
            streets.push((a.world(-l * 0.5 + 140.0, w * 0.5 + 130.0), a.world(-l * 0.5 + 140.0, w * 0.5 + 230.0)));
        }
        _ => {}
    }
    Plan { streets, pads }
}

/// Nearest point where the land meets the sea, searched from `c` between `r_min` and
/// `r_max` metres: (shore point on land, outward direction).
pub fn shore(terrain: &Terrain, c: Vec2, r_min: f32, r_max: f32) -> Option<(Vec2, Vec2)> {
    let mut best: Option<(f32, Vec2, Vec2)> = None;
    for k in 0..48 {
        let ang = k as f32 / 48.0 * TAU;
        let dir = Vec2::new(ang.cos(), ang.sin());
        let mut r = r_min;
        while r < r_max {
            let p = c + dir * r;
            if terrain.height(p.x, p.y) < 0.4 {
                // Back up to the last land sample, and make sure it is open water beyond.
                let land = c + dir * (r - 10.0);
                let beyond = c + dir * (r + 60.0);
                if terrain.height(beyond.x, beyond.y) < 0.0 && best.map_or(true, |b| r < b.0) {
                    best = Some((r, land, dir));
                }
                break;
            }
            r += 10.0;
        }
    }
    best.map(|(_, p, d)| (p, d))
}

pub enum Roof {
    Gable,
    Pyramid,
    Flat,
}

pub struct Ctx<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub lib: &'a SceneryLib,
    pub terrain: &'a Terrain,
    pub rng: &'a mut Rng,
}

impl Ctx<'_, '_, '_> {
    fn ground(&self, p: Vec2) -> Vec3 {
        Vec3::new(p.x, self.terrain.height(p.x, p.y), p.y)
    }

    fn mesh(&mut self, mesh: Handle<Mesh>, mat: &Handle<StandardMaterial>, tf: Transform) {
        self.commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat.clone()), tf, WorldEntity));
    }

    fn boxed(&mut self, mat: &Handle<StandardMaterial>, p: Vec3, size: Vec3, yaw: f32) {
        spawn_box(self.commands, self.lib, mat, p, size, yaw);
    }

    /// A box on the ground at `p2`.
    fn block(&mut self, mat: &Handle<StandardMaterial>, p2: Vec2, size: Vec3, yaw: f32) -> Vec3 {
        let g = self.ground(p2) - Vec3::Y * 0.4;
        self.boxed(mat, g, size + Vec3::Y * 0.4, yaw);
        g + Vec3::Y * (size.y + 0.4)
    }

    /// Walls plus a roof. `w` runs along the local z axis (the street when yaw = runway yaw),
    /// `d` across it.
    fn house(&mut self, p2: Vec2, yaw: f32, w: f32, d: f32, h: f32, wall: &Handle<StandardMaterial>, roof: Roof, roof_mat: &Handle<StandardMaterial>) {
        let top = self.block(wall, p2, Vec3::new(d, h, w), yaw);
        let rot = Quat::from_rotation_y(yaw);
        match roof {
            Roof::Gable => {
                let rh = (d * 0.45).clamp(1.8, 5.0);
                self.mesh(self.lib.gable.clone(), roof_mat, Transform { translation: top, rotation: rot, scale: Vec3::new(d + 1.2, rh, w + 1.2) });
            }
            Roof::Pyramid => {
                let rh = (d.min(w) * 0.4).clamp(1.5, 5.0);
                self.mesh(self.lib.pyramid.clone(), roof_mat, Transform { translation: top, rotation: rot, scale: Vec3::new(d + 1.2, rh, w + 1.2) });
            }
            Roof::Flat => {
                self.boxed(roof_mat, top - Vec3::Y * 0.1, Vec3::new(d + 0.8, 0.5, w + 0.8), yaw);
            }
        }
    }

    /// Vertical cylinder standing on the ground.
    fn cylinder(&mut self, p2: Vec2, r: f32, h: f32, mat: &Handle<StandardMaterial>, solid: bool) -> Vec3 {
        let g = self.ground(p2) - Vec3::Y * 0.3;
        self.mesh(self.lib.cyl.clone(), mat, Transform { translation: g, rotation: Quat::IDENTITY, scale: Vec3::new(r * 2.0, h + 0.3, r * 2.0) });
        if solid {
            self.lib.solid_cylinder(p2, r, g.y, g.y + h + 0.3, "a building");
        }
        g + Vec3::Y * (h + 0.3)
    }

    fn cone(&mut self, p: Vec3, r: f32, h: f32, mat: &Handle<StandardMaterial>) {
        self.mesh(self.lib.cone_unit.clone(), mat, Transform { translation: p, rotation: Quat::IDENTITY, scale: Vec3::new(r * 2.0, h, r * 2.0) });
    }

    fn dome(&mut self, p: Vec3, r: f32, mat: &Handle<StandardMaterial>) {
        self.mesh(self.lib.dome_unit.clone(), mat, Transform { translation: p, rotation: Quat::IDENTITY, scale: Vec3::splat(r) });
    }

    /// Post-and-rail fence from `a` to `b`. Solid (Adam: "the fence should be solid"): the
    /// whole run is one thin collision box, tall enough to catch a plane rolling on its wheels.
    fn fence(&mut self, a: Vec2, b: Vec2, mat: &Handle<StandardMaterial>) {
        let d = b - a;
        let len = d.length();
        if len < 1.0 {
            return;
        }
        let dir = d / len;
        let yaw = crate::world::yaw_from_forward(dir);
        let n = (len / 4.0).ceil() as usize;
        for k in 0..=n {
            let p = a + dir * (k as f32 * len / n as f32);
            let g = self.ground(p);
            self.boxed(mat, g - Vec3::Y * 0.2, Vec3::new(0.18, 0.95, 0.18), yaw);
        }
        let mid = self.ground(a + d * 0.5);
        for y in [0.4, 0.75] {
            self.boxed(mat, mid + Vec3::Y * y, Vec3::new(0.08, 0.08, len), yaw);
        }
        self.lib.solid_box(mid - Vec3::Y * 0.5, Vec3::new(0.4, 2.2, len), yaw, "a fence");
    }

    fn fence_rect(&mut self, a: &Airport, u0: f32, u1: f32, v0: f32, v1: f32, mat: &Handle<StandardMaterial>) {
        let c = [a.world(u0, v0), a.world(u1, v0), a.world(u1, v1), a.world(u0, v1)];
        for k in 0..4 {
            self.fence(c[k], c[(k + 1) % 4], mat);
        }
    }

    fn car(&mut self, p2: Vec2, yaw: f32, mat: &Handle<StandardMaterial>) {
        let g = self.ground(p2);
        self.boxed(mat, g + Vec3::Y * 0.3, Vec3::new(1.8, 0.9, 4.2), yaw);
        self.boxed(&self.lib.m_dark.clone(), g + Vec3::Y * 1.2, Vec3::new(1.6, 0.7, 2.2), yaw);
        for (sx, sz) in [(-0.9, -1.3), (0.9, -1.3), (-0.9, 1.3), (0.9, 1.3)] {
            let off = Quat::from_rotation_y(yaw) * Vec3::new(sx, 0.0, sz);
            self.boxed(&self.lib.m_dark.clone(), g + off, Vec3::new(0.25, 0.65, 0.65), yaw);
        }
    }

    fn truck(&mut self, p2: Vec2, yaw: f32, cab: &Handle<StandardMaterial>, bed: &Handle<StandardMaterial>) {
        let g = self.ground(p2);
        let rot = Quat::from_rotation_y(yaw);
        self.boxed(cab, g + Vec3::Y * 0.6 + rot * Vec3::new(0.0, 0.0, -2.6), Vec3::new(2.4, 2.4, 2.2), yaw);
        self.boxed(bed, g + Vec3::Y * 0.6 + rot * Vec3::new(0.0, 0.0, 1.2), Vec3::new(2.4, 2.0, 5.2), yaw);
        for (sx, sz) in [(-1.2, -2.6), (1.2, -2.6), (-1.2, 1.4), (1.2, 1.4), (-1.2, 2.8), (1.2, 2.8)] {
            self.boxed(&self.lib.m_dark.clone(), g + rot * Vec3::new(sx, 0.0, sz), Vec3::new(0.35, 1.0, 1.0), yaw);
        }
    }

    /// A small boat floating at sea level, bow toward `yaw`.
    fn boat(&mut self, p2: Vec2, yaw: f32, hull: &Handle<StandardMaterial>, big: bool) {
        let (w, h, l) = if big { (4.0, 2.2, 14.0) } else { (2.2, 1.2, 6.5) };
        let hull_mesh = self.meshes.add(meshes::tapered_box(w, h, l, 0.25, -1.0));
        let p = Vec3::new(p2.x, -h * 0.35, p2.y);
        let rot = Quat::from_rotation_y(yaw);
        self.mesh(hull_mesh, hull, Transform { translation: p + Vec3::Y * h * 0.5, rotation: rot, scale: Vec3::ONE });
        // Wheelhouse and mast.
        let cab = if big { Vec3::new(3.0, 2.4, 3.5) } else { Vec3::new(1.6, 1.2, 1.8) };
        self.boxed(&self.lib.m_white.clone(), p + Vec3::Y * h + rot * Vec3::new(0.0, 0.0, l * 0.15), cab, yaw);
        self.boxed(&self.lib.m_dark.clone(), p + Vec3::Y * h + rot * Vec3::new(0.0, 0.0, -l * 0.15), Vec3::new(0.15, if big { 9.0 } else { 5.0 }, 0.15), yaw);
    }

    /// Plank jetty on posts from `from` (on land) out along `dir` for `len` metres.
    fn pier(&mut self, from: Vec2, dir: Vec2, len: f32) -> Vec2 {
        let yaw = crate::world::yaw_from_forward(dir);
        let deck_y = 1.4;
        let n = (len / 8.0).ceil() as usize;
        for k in 0..n {
            let c = from + dir * ((k as f32 + 0.5) * len / n as f32);
            self.boxed(&self.lib.m_brown.clone(), Vec3::new(c.x, deck_y, c.y), Vec3::new(3.2, 0.3, len / n as f32 + 0.1), yaw);
            for s in [-1.3, 1.3] {
                let off = Quat::from_rotation_y(yaw) * Vec3::new(s, 0.0, 0.0);
                self.boxed(&self.lib.m_trunk.clone(), Vec3::new(c.x, -2.0, c.y) + off, Vec3::new(0.35, deck_y + 2.0 + 0.9, 0.35), yaw);
            }
        }
        from + dir * len
    }

    fn lamp(&mut self, p2: Vec2) {
        let g = self.ground(p2);
        self.boxed(&self.lib.m_dark.clone(), g, Vec3::new(0.22, 6.5, 0.22), 0.0);
        self.boxed(&self.lib.m_white.clone(), g + Vec3::Y * 6.4, Vec3::new(1.2, 0.4, 0.5), 0.0);
    }

    fn container(&mut self, p2: Vec2, yaw: f32, mat: &Handle<StandardMaterial>, stacked: usize) {
        for k in 0..=stacked {
            let g = self.ground(p2) + Vec3::Y * (k as f32 * 2.6);
            self.boxed(mat, g, Vec3::new(2.4, 2.6, 12.0), yaw);
        }
    }

    fn tank(&mut self, p2: Vec2, r: f32, h: f32, mat: &Handle<StandardMaterial>) {
        let top = self.cylinder(p2, r, h, mat, true);
        self.dome(top - Vec3::Y * 0.1, r * 0.98, mat);
    }

    fn silo(&mut self, p2: Vec2, r: f32, h: f32) {
        let top = self.cylinder(p2, r, h, &self.lib.m_steel.clone(), true);
        self.cone(top, r * 1.05, r * 0.9, &self.lib.m_slate.clone());
    }

    fn turbine(&mut self, p2: Vec2, h: f32) {
        let top = self.cylinder(p2, 1.0, h, &self.lib.m_white.clone(), true);
        let hub = top + Vec3::new(0.0, 0.0, -1.6);
        self.boxed(&self.lib.m_white.clone(), top - Vec3::Y * 1.5, Vec3::new(2.4, 3.0, 5.0), 0.0);
        let phase = self.rng.range(0.0, TAU);
        for k in 0..3 {
            let ang = phase + k as f32 * TAU / 3.0;
            let rot = Quat::from_rotation_z(ang);
            let blade = self.meshes.add(Mesh::from(Cuboid::new(1.0, h * 0.45, 0.25)).translated_by(Vec3::new(0.0, h * 0.225, 0.0)));
            self.mesh(blade, &self.lib.m_white.clone(), Transform { translation: hub, rotation: rot, scale: Vec3::ONE });
        }
    }

    fn crane(&mut self, p2: Vec2, yaw: f32, h: f32, jib: f32) {
        let g = self.ground(p2);
        let rot = Quat::from_rotation_y(yaw);
        for (sx, sz) in [(-2.0, -2.0), (2.0, -2.0), (-2.0, 2.0), (2.0, 2.0)] {
            self.boxed(&self.lib.m_yellow.clone(), g + rot * Vec3::new(sx, 0.0, sz), Vec3::new(0.6, h, 0.6), yaw);
        }
        self.boxed(&self.lib.m_yellow.clone(), g + Vec3::Y * h, Vec3::new(5.0, 1.5, 5.0), yaw);
        self.boxed(&self.lib.m_yellow.clone(), g + Vec3::Y * (h + 1.5) + rot * Vec3::new(0.0, 0.0, -jib * 0.35), Vec3::new(1.0, 1.0, jib), yaw);
        self.boxed(&self.lib.m_dark.clone(), g + Vec3::Y * (h * 0.3) + rot * Vec3::new(0.0, 0.0, -jib * 0.7), Vec3::new(0.08, h * 0.7 + 1.5, 0.08), yaw);
    }

    fn church(&mut self, p2: Vec2, yaw: f32) {
        self.house(p2, yaw, 22.0, 10.0, 7.0, &self.lib.m_white.clone(), Roof::Gable, &self.lib.m_slate.clone());
        let rot = Quat::from_rotation_y(yaw);
        let tp = p2 + Vec2::new((rot * Vec3::new(0.0, 0.0, -14.0)).x, (rot * Vec3::new(0.0, 0.0, -14.0)).z);
        let top = self.block(&self.lib.m_white.clone(), tp, Vec3::new(6.0, 14.0, 6.0), yaw);
        self.mesh(self.lib.pyramid.clone(), &self.lib.m_slate.clone(), Transform { translation: top, rotation: rot, scale: Vec3::new(6.6, 7.0, 6.6) });
    }

    /// Big shed with a low gable roof: hangars, barns, warehouses.
    fn shed(&mut self, p2: Vec2, yaw: f32, w: f32, d: f32, h: f32, wall: &Handle<StandardMaterial>, roof: &Handle<StandardMaterial>) {
        let top = self.block(wall, p2, Vec3::new(d, h, w), yaw);
        self.mesh(self.lib.gable.clone(), roof, Transform { translation: top, rotation: Quat::from_rotation_y(yaw), scale: Vec3::new(d + 1.5, d * 0.22, w + 1.5) });
    }

    fn tree(&mut self, p2: Vec2, s: f32, pine: bool) {
        let g = self.ground(p2) - Vec3::Y * 0.3;
        spawn_tree(self.commands, self.lib, g, s, pine);
        let (r, h) = if pine { (crate::world::PINE_RADIUS, crate::world::PINE_HEIGHT) } else { (crate::world::TREE_RADIUS, crate::world::TREE_HEIGHT) };
        self.lib.solid_cylinder(p2, r * s, g.y, g.y + h * s, "a tree");
    }

    fn palm(&mut self, p2: Vec2) {
        let g = self.ground(p2) - Vec3::Y * 0.3;
        spawn_palm(self.commands, self.lib, g);
        self.lib.solid_cylinder(p2, 1.2, g.y, g.y + crate::world::PALM_HEIGHT, "a palm");
    }

    fn heap(&mut self, p2: Vec2, r: f32, h: f32, mat: &Handle<StandardMaterial>) {
        let g = self.ground(p2) - Vec3::Y * 0.2;
        self.cone(g, r, h, mat);
        self.lib.solid_cylinder(p2, r * 0.6, g.y, g.y + h * 0.8, "a heap of ore");
    }

    /// Market stall: four posts and a striped canopy.
    fn stall(&mut self, p2: Vec2, yaw: f32, mat: &Handle<StandardMaterial>) {
        let g = self.ground(p2);
        let rot = Quat::from_rotation_y(yaw);
        for (sx, sz) in [(-1.5, -1.2), (1.5, -1.2), (-1.5, 1.2), (1.5, 1.2)] {
            self.boxed(&self.lib.m_brown.clone(), g + rot * Vec3::new(sx, 0.0, sz), Vec3::new(0.15, 2.4, 0.15), yaw);
        }
        self.boxed(mat, g + Vec3::Y * 2.4, Vec3::new(3.6, 0.2, 3.0), yaw);
        self.boxed(&self.lib.m_brown.clone(), g, Vec3::new(3.0, 0.9, 1.2), yaw);
    }

    fn tent(&mut self, p2: Vec2, yaw: f32, mat: &Handle<StandardMaterial>) {
        let g = self.ground(p2) - Vec3::Y * 0.2;
        self.mesh(self.lib.gable.clone(), mat, Transform { translation: g, rotation: Quat::from_rotation_y(yaw), scale: Vec3::new(3.2, 2.0, 4.5) });
    }

    fn flagpole(&mut self, p2: Vec2, flag: &Handle<StandardMaterial>) {
        let g = self.ground(p2);
        self.boxed(&self.lib.m_white.clone(), g, Vec3::new(0.2, 11.0, 0.2), 0.0);
        self.boxed(flag, g + Vec3::new(1.2, 9.6, 0.0), Vec3::new(2.4, 1.4, 0.08), 0.0);
    }

    fn watchtower(&mut self, p2: Vec2, yaw: f32) {
        let g = self.ground(p2);
        let rot = Quat::from_rotation_y(yaw);
        for (sx, sz) in [(-1.6, -1.6), (1.6, -1.6), (-1.6, 1.6), (1.6, 1.6)] {
            self.boxed(&self.lib.m_brown.clone(), g + rot * Vec3::new(sx, 0.0, sz), Vec3::new(0.4, 9.0, 0.4), yaw);
        }
        self.boxed(&self.lib.m_brown.clone(), g + Vec3::Y * 9.0, Vec3::new(5.0, 2.6, 5.0), yaw);
        self.mesh(self.lib.pyramid.clone(), &self.lib.m_slate.clone(), Transform { translation: g + Vec3::Y * 11.6, rotation: rot, scale: Vec3::new(6.0, 2.0, 6.0) });
    }

    fn dish(&mut self, p2: Vec2, r: f32) {
        let g = self.ground(p2);
        self.boxed(&self.lib.m_grey.clone(), g, Vec3::new(1.6, r * 0.9, 1.6), 0.0);
        let dish = self.meshes.add(meshes::cone_up(r, r * 0.35, 16));
        self.mesh(dish, &self.lib.m_white.clone(), Transform { translation: g + Vec3::Y * (r * 0.9 + 1.0), rotation: Quat::from_rotation_x(-0.9), scale: Vec3::ONE });
    }

    fn solar(&mut self, p2: Vec2, yaw: f32) {
        let g = self.ground(p2);
        let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-0.5);
        let panel = self.meshes.add(Cuboid::new(8.0, 0.2, 3.0));
        self.mesh(panel, &self.lib.m_navy.clone(), Transform { translation: g + Vec3::Y * 1.6, rotation: rot, scale: Vec3::ONE });
        for s in [-3.0, 3.0] {
            self.boxed(&self.lib.m_grey.clone(), g + Quat::from_rotation_y(yaw) * Vec3::new(s, 0.0, 0.0), Vec3::new(0.2, 1.4, 0.2), yaw);
        }
    }

    fn hay(&mut self, p2: Vec2, yaw: f32) {
        let g = self.ground(p2);
        let bale = self.meshes.add(Cylinder::new(0.9, 1.6).mesh().resolution(10).build());
        self.mesh(bale, &self.lib.m_tan.clone(), Transform { translation: g + Vec3::Y * 0.9, rotation: Quat::from_rotation_y(yaw) * Quat::from_rotation_z(FRAC_PI_2), scale: Vec3::ONE });
    }

    fn water_tower(&mut self, p2: Vec2) {
        let g = self.ground(p2);
        for (sx, sz) in [(-2.2, -2.2), (2.2, -2.2), (-2.2, 2.2), (2.2, 2.2)] {
            self.boxed(&self.lib.m_steel.clone(), g + Vec3::new(sx, 0.0, sz), Vec3::new(0.4, 14.0, 0.4), 0.0);
        }
        self.mesh(self.lib.cyl.clone(), &self.lib.m_steel.clone(), Transform { translation: g + Vec3::Y * 14.0, rotation: Quat::IDENTITY, scale: Vec3::new(7.0, 5.0, 7.0) });
        self.cone(g + Vec3::Y * 19.0, 3.8, 2.0, &self.lib.m_slate.clone());
        self.lib.solid_cylinder(p2, 3.5, g.y, g.y + 21.0, "a water tower");
    }

    fn pick<'m>(&mut self, list: &[&'m Handle<StandardMaterial>]) -> &'m Handle<StandardMaterial> {
        list[(self.rng.f32() * list.len() as f32) as usize % list.len()]
    }
}

/// City blocks that would sit on the runway, its overrun areas or the apron are left empty.
fn city_keep_out(a: &Airport, u: f32, v: f32) -> bool {
    let l = a.def.length;
    let w = a.def.width;
    let strip = u.abs() < l * 0.5 + 260.0 && v.abs() < w * 0.5 + 75.0;
    let apron = u > -l * 0.5 + 20.0 && u < -l * 0.5 + 300.0 && v > w * 0.5 && v < w * 0.5 + 135.0;
    strip || apron
}

/// Tallest building allowed at (u, v): low near the runway ends so the approach stays
/// flyable, rising with distance from the thresholds; full height off to the sides.
fn city_height_cap(a: &Airport, u: f32, v: f32) -> f32 {
    let l = a.def.length;
    if v.abs() < 160.0 && u.abs() > l * 0.5 {
        let past = u.abs() - l * 0.5 - 260.0;
        (12.0 + past * 0.075).clamp(8.0, 64.0)
    } else {
        64.0
    }
}

/// Fills the blocks of `g` with houses facing the streets. `style` picks the palette.
fn fill_blocks(cx: &mut Ctx, a: &Airport, g: Grid, style: AirportKind) {
    let yaw = a.yaw();
    let lib = cx.lib;
    let walls: Vec<&Handle<StandardMaterial>> = match style {
        AirportKind::Base => vec![&lib.m_olive, &lib.m_olive, &lib.m_tan],
        AirportKind::Village => vec![&lib.m_white, &lib.m_cream, &lib.m_tan, &lib.m_olive],
        AirportKind::Resort => vec![&lib.m_white, &lib.m_cream, &lib.m_yellow],
        AirportKind::Mine | AirportKind::LumberCamp => vec![&lib.m_brown, &lib.m_tan, &lib.m_olive],
        _ => vec![&lib.m_white, &lib.m_cream, &lib.m_yellow, &lib.m_brick, &lib.m_blue, &lib.m_tan],
    };
    let roofs: Vec<&Handle<StandardMaterial>> = match style {
        AirportKind::Base => vec![&lib.m_slate],
        AirportKind::Mine | AirportKind::LumberCamp => vec![&lib.m_slate, &lib.m_steel],
        _ => vec![&lib.m_red, &lib.m_terracotta, &lib.m_slate, &lib.m_red],
    };
    let hedge = lib.m_leaf.clone();
    for r in 0..g.rows {
        for c in 0..g.cols {
            let (u_a, u_b) = (g.u0 + c as f32 * g.pitch_u, g.u0 + (c + 1) as f32 * g.pitch_u);
            let (v_a, v_b) = (g.v0 + r as f32 * g.pitch_v, g.v0 + (r + 1) as f32 * g.pitch_v);
            // Downtown: one or two towers per block with a mid-rise between, all flat roofs.
            if style == AirportKind::City {
                let (bu, bv) = ((u_a + u_b) * 0.5, (v_a + v_b) * 0.5);
                if city_keep_out(a, bu, bv) {
                    continue;
                }
                let cap = city_height_cap(a, bu, bv);
                let n = 1 + (cx.rng.f32() < 0.6) as usize;
                for k in 0..n {
                    let fw = cx.rng.range(16.0, 26.0);
                    let fd = cx.rng.range(16.0, 24.0);
                    let tall = cx.rng.f32() < 0.55 && cap > 30.0;
                    let h = if tall { cx.rng.range(28.0, cap) } else { cx.rng.range(8.0, 20.0_f32.min(cap)) };
                    let u = u_a + 14.0 + k as f32 * 30.0 + fw * 0.5;
                    let v = (v_a + v_b) * 0.5 + cx.rng.range(-6.0, 6.0);
                    let wall = cx.pick(&[&lib.m_steel, &lib.m_white, &lib.m_cream, &lib.m_navy, &lib.m_glass_tint, &lib.m_brick]).clone();
                    cx.house(a.world(u, v), yaw, fw, fd, h, &wall, Roof::Flat, &lib.m_dark.clone());
                    if tall {
                        // Rooftop plant and a mast.
                        let top = cx.ground(a.world(u, v)) + Vec3::Y * (h + 0.5);
                        cx.boxed(&lib.m_grey.clone(), top, Vec3::new(fd * 0.3, 3.0, fw * 0.3), yaw);
                        cx.boxed(&lib.m_red.clone(), top + Vec3::Y * 3.0, Vec3::new(0.4, 6.0, 0.4), yaw);
                    }
                }
                if cx.rng.f32() < 0.5 {
                    let cm = cx.pick(&[&lib.m_red, &lib.m_white, &lib.m_blue, &lib.m_dark, &lib.m_yellow]).clone();
                    let ju = cx.rng.range(10.0, 50.0);
                    cx.car(a.world(u_a + ju, v_a + 4.0), yaw, &cm);
                }
                cx.lamp(a.world(u_a + 2.5, v_a + 2.5));
                cx.lamp(a.world(u_b - 2.5, v_b - 2.5));
                continue;
            }
            // Barracks / bunkhouses: long huts across the block instead of houses.
            if matches!(style, AirportKind::Base | AirportKind::Mine | AirportKind::LumberCamp) {
                let n = ((u_b - u_a - 12.0) / 16.0).floor().max(1.0) as usize;
                for k in 0..n {
                    let u = u_a + 12.0 + k as f32 * 16.0;
                    let v = (v_a + v_b) * 0.5;
                    let wall = cx.pick(&walls).clone();
                    let roof = cx.pick(&roofs).clone();
                    cx.house(a.world(u, v), yaw, 8.0, g.pitch_v - 20.0, 3.6, &wall, Roof::Gable, &roof);
                }
                continue;
            }
            // Houses along the two streets that run along the runway (v_a and v_b sides).
            for (v_edge, side) in [(v_a, 1.0), (v_b, -1.0)] {
                let mut u = u_a + 9.0;
                while u < u_b - 8.0 {
                    let w = cx.rng.range(8.0, 13.0);
                    if u + w > u_b - 6.0 {
                        break;
                    }
                    let d = cx.rng.range(7.0, 10.0);
                    let two_storey = cx.rng.f32() < 0.25;
                    let h = if two_storey { 6.4 } else { 3.6 };
                    let v = v_edge + side * (9.0 + d * 0.5);
                    let wall = cx.pick(&walls).clone();
                    let roof = cx.pick(&roofs).clone();
                    let kind = if cx.rng.f32() < 0.2 { Roof::Pyramid } else { Roof::Gable };
                    cx.house(a.world(u + w * 0.5, v), yaw, w, d, h, &wall, kind, &roof);
                    // A garden tree behind some houses, a car in front of others.
                    if cx.rng.f32() < 0.45 {
                        let (ju, ts) = (cx.rng.range(-3.0, 3.0), cx.rng.range(0.5, 0.8));
                        cx.tree(a.world(u + w * 0.5 + ju, v + side * (d * 0.5 + 5.0)), ts, false);
                    }
                    if cx.rng.f32() < 0.3 {
                        let cm = cx.pick(&[&lib.m_red, &lib.m_white, &lib.m_blue, &lib.m_dark, &lib.m_yellow]).clone();
                        cx.car(a.world(u + w * 0.5 + 2.0, v_edge + side * 4.0), yaw + FRAC_PI_2, &cm);
                    }
                    u += w + cx.rng.range(4.0, 9.0);
                }
            }
            // Hedges along the cross streets, lamps at the corners.
            if cx.rng.f32() < 0.5 {
                cx.boxed(&hedge, cx.ground(a.world(u_a + 4.0, (v_a + v_b) * 0.5)), Vec3::new(g.pitch_v - 26.0, 1.2, 1.0), yaw);
            }
            cx.lamp(a.world(u_a + 2.5, v_a + 2.5));
        }
    }
    // Cars parked along the outer street.
    let v_out = g.v0 + g.rows as f32 * g.pitch_v;
    for k in 0..g.cols {
        if cx.rng.f32() < 0.6 {
            let u = g.u0 + (k as f32 + cx.rng.range(0.2, 0.8)) * g.pitch_u;
            let cm = cx.pick(&[&lib.m_red, &lib.m_white, &lib.m_blue, &lib.m_dark]).clone();
            cx.car(a.world(u, v_out + 3.5), yaw, &cm);
        }
    }
}

/// Runway surroundings shared by every airport: fence, edge lights, taxiway, fuel farm,
/// hangar (except the home field, which has its own), parked vehicles.
pub fn spawn_airfield(cx: &mut Ctx, a: &Airport) {
    let yaw = a.yaw();
    let l = a.def.length;
    let w = a.def.width;
    let lib = cx.lib;
    // Edge lights every 60 m and a fence around the field.
    let mut u = -l * 0.5 + 30.0;
    while u < l * 0.5 - 20.0 {
        for s in [-1.0, 1.0] {
            let p = a.world3(u, s * (w * 0.5 + 2.0), 0.0);
            cx.boxed(&lib.m_white.clone(), p, Vec3::new(0.4, 0.5, 0.4), yaw);
        }
        u += 60.0;
    }
    // The perimeter fence is closed and solid; it runs 160 m past each threshold so a short
    // landing clears it but an overrun ends in the wire (Adam: removing it is not the answer).
    cx.fence_rect(a, -l * 0.5 - 160.0, l * 0.5 + 160.0, -(w * 0.5 + 40.0), w * 0.5 + 118.0, &lib.m_grey.clone());
    // Taxiway from the runway to the apron.
    let taxi = cx.meshes.add(Cuboid::new(30.0, 0.25, 22.0));
    cx.mesh(taxi, &lib.m_concrete.clone(), Transform { translation: a.world3(-l * 0.5 + 140.0, w * 0.5 + 2.0, -0.08), rotation: Quat::from_rotation_y(yaw), scale: Vec3::ONE });
    // Fuel farm.
    for k in 0..2 {
        cx.tank(a.world(-l * 0.5 + 216.0 + k as f32 * 9.0, w * 0.5 + 62.0), 3.2, 5.0, &lib.m_white.clone());
    }
    cx.fence_rect(a, -l * 0.5 + 208.0, -l * 0.5 + 233.0, w * 0.5 + 54.0, w * 0.5 + 70.0, &lib.m_grey.clone());
    // Hangar and a fuel truck on the apron (the home field has its own hangar; the small
    // and cramped places make do with the apron).
    if !matches!(a.def.kind, AirportKind::Home | AirportKind::Hermit | AirportKind::Wreck | AirportKind::Outpost | AirportKind::Canyon | AirportKind::Mesa) {
        let mat = match a.def.kind {
            AirportKind::Base => lib.m_olive.clone(),
            AirportKind::Hospital | AirportKind::Research => lib.m_white.clone(),
            _ => lib.m_steel.clone(),
        };
        cx.shed(a.world(-l * 0.5 + 140.0, w * 0.5 + 112.0), yaw, 30.0, 26.0, 8.0, &mat, &lib.m_slate.clone());
    }
    cx.truck(a.world(-l * 0.5 + 100.0, w * 0.5 + 60.0), yaw + 0.3, &lib.m_yellow.clone(), &lib.m_steel.clone());
    cx.car(a.world(-l * 0.5 + 70.0, w * 0.5 + 40.0), yaw, &lib.m_white.clone());
    for k in 0..3 {
        cx.lamp(a.world(-l * 0.5 + 90.0 + k as f32 * 50.0, w * 0.5 + 84.0));
    }
}

/// Everything beyond the airfield itself: the settlement for this airport's kind.
pub fn spawn_settlement(cx: &mut Ctx, a: &Airport) {
    let yaw = a.yaw();
    let l = a.def.length;
    let w = a.def.width;
    let lib = cx.lib;
    let v_base = w * 0.5 + 130.0;
    if let Some(g) = grid_for(a) {
        fill_blocks(cx, a, g, a.def.kind);
    }
    match a.def.kind {
        AirportKind::Home => {
            // Church on the square, warehouses by the apron, and a working harbour on the
            // nearest shore.
            cx.church(a.world(-l * 0.5 + 60.0 + 2.5 * 58.0, v_base + 1.5 * 46.0), yaw);
            for k in 0..3 {
                cx.shed(a.world(-l * 0.5 + 250.0 + k as f32 * 34.0, w * 0.5 + 98.0), yaw, 26.0, 20.0, 8.0, &lib.m_blue.clone(), &lib.m_slate.clone());
            }
            cx.water_tower(a.world(l * 0.5 - 60.0, w * 0.5 + 100.0));
            if let Some((sp, dir)) = shore(cx.terrain, a.def.pos, 250.0, 900.0) {
                let along = Vec2::new(-dir.y, dir.x);
                let syaw = crate::world::yaw_from_forward(dir);
                // Quay: a concrete edge, two cranes, container stacks, a warehouse, boats.
                let quay = cx.meshes.add(Cuboid::new(90.0, 2.0, 14.0));
                cx.mesh(quay, &lib.m_concrete.clone(), Transform { translation: Vec3::new(sp.x, 0.6, sp.y) - Vec3::new(dir.x, 0.0, dir.y) * 2.0, rotation: Quat::from_rotation_y(syaw), scale: Vec3::ONE });
                for s in [-28.0, 24.0] {
                    cx.crane(sp + along * s - dir * 6.0, syaw, 16.0, 26.0);
                }
                for k in 0..5 {
                    let cm = cx.pick(&[&lib.m_red, &lib.m_blue, &lib.m_orange, &lib.m_olive]).clone();
                    cx.container(sp + along * (-10.0 + k as f32 * 3.0) - dir * 18.0, syaw + FRAC_PI_2, &cm, (k % 2) as usize);
                }
                cx.shed(sp - dir * 40.0 + along * 30.0, syaw, 28.0, 18.0, 7.0, &lib.m_brick.clone(), &lib.m_slate.clone());
                cx.boat(sp + dir * 14.0 + along * 10.0, syaw + FRAC_PI_2, &lib.m_navy.clone(), true);
                cx.boat(sp + dir * 12.0 - along * 30.0, syaw + FRAC_PI_2 + 0.2, &lib.m_red.clone(), false);
                cx.boat(sp + dir * 40.0 + along * 40.0, syaw - 0.8, &lib.m_white.clone(), false);
                let end = cx.pier(sp + along * 44.0, dir, 40.0);
                cx.boat(end + along * 4.0 - dir * 8.0, syaw, &lib.m_olive.clone(), false);
            }
        }
        AirportKind::Town => {
            let g = grid_for(a).unwrap();
            // Church and a park in the middle blocks, a school and a water tower.
            let cu = g.u0 + 3.5 * g.pitch_u;
            cx.church(a.world(cu, g.v0 + 1.5 * g.pitch_v), yaw);
            for _ in 0..8 {
                let (ju, jv, ts) = (cx.rng.range(-22.0, 22.0), cx.rng.range(-12.0, 12.0), cx.rng.range(0.7, 1.0));
                cx.tree(a.world(cu + ju, g.v0 + 2.5 * g.pitch_v + jv), ts, false);
            }
            cx.house(a.world(g.u0 + 1.5 * g.pitch_u, g.v0 + 4.0 * g.pitch_v + 26.0), yaw, 34.0, 12.0, 6.5, &lib.m_cream.clone(), Roof::Flat, &lib.m_slate.clone());
            cx.water_tower(a.world(g.u0 + 6.0 * g.pitch_u + 20.0, g.v0 - 20.0));
            // A row of shops with flat roofs on the first street.
            for k in 0..5 {
                let cm = cx.pick(&[&lib.m_yellow, &lib.m_white, &lib.m_brick, &lib.m_blue]).clone();
                cx.house(a.world(g.u0 + 20.0 + k as f32 * 16.0, g.v0 - 14.0), yaw, 12.0, 9.0, 4.5, &cm, Roof::Flat, &lib.m_dark.clone());
            }
        }
        AirportKind::Village => {
            let g = grid_for(a).unwrap();
            cx.church(a.world(g.u0 + g.cols as f32 * g.pitch_u + 30.0, g.v0 + 30.0), yaw);
            // A well on the green, orchard rows behind, fences round the paddocks.
            let green = a.world(g.u0 - 30.0, g.v0 + 30.0);
            cx.cylinder(green, 1.4, 1.0, &lib.m_grey.clone(), true);
            for r in 0..3 {
                for k in 0..9 {
                    cx.tree(a.world(g.u0 + 10.0 + k as f32 * 14.0, g.v0 + g.rows as f32 * g.pitch_v + 30.0 + r as f32 * 14.0), 0.6, false);
                }
            }
            cx.fence_rect(a, g.u0 - 60.0, g.u0 - 8.0, g.v0 + 10.0, g.v0 + 60.0, &lib.m_brown.clone());
            for k in 0..4 {
                cx.hay(a.world(g.u0 - 50.0 + k as f32 * 11.0, g.v0 + 35.0), yaw);
            }
        }
        AirportKind::Farm => {
            let fu = l * 0.3;
            // Farmhouse, two barns, silos, tractors and hay by the track; turbines up the hill.
            cx.house(a.world(fu - 40.0, v_base + 30.0), yaw, 14.0, 10.0, 6.4, &lib.m_white.clone(), Roof::Gable, &lib.m_slate.clone());
            cx.shed(a.world(fu + 30.0, v_base + 40.0), yaw, 24.0, 14.0, 7.0, &lib.m_brick.clone(), &lib.m_slate.clone());
            cx.shed(a.world(fu + 30.0, v_base + 80.0), yaw, 30.0, 14.0, 6.0, &lib.m_red.clone(), &lib.m_steel.clone());
            for k in 0..3 {
                cx.silo(a.world(fu + 60.0, v_base + 26.0 + k as f32 * 9.0), 3.6, 14.0);
            }
            cx.truck(a.world(fu - 10.0, v_base + 12.0), yaw + 0.4, &lib.m_green.clone(), &lib.m_tan.clone());
            cx.truck(a.world(fu + 5.0, v_base + 60.0), yaw + FRAC_PI_2, &lib.m_red.clone(), &lib.m_tan.clone());
            for _ in 0..12 {
                let (ju, jv, ang) = (cx.rng.range(-40.0, 40.0), cx.rng.range(-30.0, 30.0), cx.rng.range(0.0, PI));
                cx.hay(a.world(fu - 60.0 + ju, v_base + 150.0 + jv), ang);
            }
            cx.fence_rect(a, fu - 110.0, fu - 10.0, v_base + 110.0, v_base + 200.0, &lib.m_brown.clone());
            for k in 0..3 {
                cx.turbine(a.world(-l * 0.4 + k as f32 * 90.0, v_base + 260.0), 32.0);
            }
            // Orchard rows and a hedge line.
            for r in 0..4 {
                for k in 0..12 {
                    cx.tree(a.world(-l * 0.45 + k as f32 * 15.0, v_base + 20.0 + r as f32 * 15.0), 0.6, false);
                }
            }
            cx.house(a.world(fu + 90.0, v_base + 20.0), yaw, 6.0, 4.0, 2.4, &lib.m_tan.clone(), Roof::Gable, &lib.m_slate.clone());
        }
        AirportKind::Hospital => {
            // Main block with two wings, ambulance bay, car park, helipad, staff housing.
            let hp = a.world(0.0, v_base + 20.0);
            cx.house(hp, yaw, 46.0, 18.0, 12.0, &lib.m_white.clone(), Roof::Flat, &lib.m_slate.clone());
            let top = cx.ground(hp) + Vec3::Y * 12.6;
            cx.boxed(&lib.m_red.clone(), top, Vec3::new(3.0, 1.0, 10.0), yaw);
            cx.boxed(&lib.m_red.clone(), top, Vec3::new(10.0, 1.0, 3.0), yaw);
            for s in [-1.0, 1.0] {
                cx.house(a.world(s * 26.0, v_base + 44.0), yaw, 12.0, 28.0, 8.0, &lib.m_white.clone(), Roof::Flat, &lib.m_slate.clone());
            }
            for k in 0..3 {
                cx.truck(a.world(-30.0 + k as f32 * 8.0, v_base - 4.0), yaw + FRAC_PI_2, &lib.m_white.clone(), &lib.m_white.clone());
            }
            let park = cx.meshes.add(Cuboid::new(30.0, 0.2, 60.0));
            cx.mesh(park, &lib.m_dark.clone(), Transform { translation: a.world3(70.0, v_base + 10.0, -0.05), rotation: Quat::from_rotation_y(yaw), scale: Vec3::ONE });
            for k in 0..7 {
                if cx.rng.f32() < 0.7 {
                    let cm = cx.pick(&[&lib.m_red, &lib.m_white, &lib.m_blue, &lib.m_dark]).clone();
                    cx.car(a.world(48.0 + k as f32 * 7.0, v_base + 2.0), yaw + FRAC_PI_2, &cm);
                }
            }
            let pad = cx.meshes.add(Cylinder::new(12.0, 0.3).mesh().resolution(16).build());
            cx.mesh(pad, &lib.m_concrete.clone(), Transform::from_translation(a.world3(-80.0, w * 0.5 + 80.0, 0.0)));
            cx.boxed(&lib.m_white.clone(), a.world3(-80.0, w * 0.5 + 80.0, 0.3), Vec3::new(1.2, 0.05, 8.0), yaw);
            for k in 0..6 {
                cx.tree(a.world(-40.0 + k as f32 * 16.0, v_base + 70.0), 0.8, false);
            }
        }
        AirportKind::Lighthouse => {
            let (p, dir) = shore(cx.terrain, a.def.pos, 200.0, 800.0).unwrap_or((a.world(l * 0.5 + 90.0, -60.0), a.dir));
            let tower_p = p - dir * 12.0;
            let top = cx.cylinder(tower_p, 4.0, 34.0, &lib.m_white.clone(), true);
            cx.mesh(lib.cyl.clone(), &lib.m_red.clone(), Transform { translation: top - Vec3::Y * 12.0, rotation: Quat::IDENTITY, scale: Vec3::new(8.2, 6.0, 8.2) });
            cx.mesh(lib.cyl.clone(), &lib.m_dark.clone(), Transform { translation: top, rotation: Quat::IDENTITY, scale: Vec3::new(10.0, 4.0, 10.0) });
            cx.cone(top + Vec3::Y * 4.0, 5.0, 3.0, &lib.m_red.clone());
            let syaw = crate::world::yaw_from_forward(dir);
            let along = Vec2::new(-dir.y, dir.x);
            cx.house(tower_p - dir * 22.0 + along * 6.0, syaw, 12.0, 9.0, 4.0, &lib.m_white.clone(), Roof::Gable, &lib.m_red.clone());
            cx.house(tower_p - dir * 18.0 - along * 14.0, syaw, 6.0, 5.0, 3.0, &lib.m_grey.clone(), Roof::Gable, &lib.m_slate.clone());
            let c = [tower_p - dir * 34.0 + along * 26.0, tower_p - dir * 34.0 - along * 26.0, tower_p - dir * 2.0 - along * 26.0, tower_p - dir * 2.0 + along * 26.0];
            for k in 0..4 {
                cx.fence(c[k], c[(k + 1) % 4], &lib.m_white.clone());
            }
            let end = cx.pier(p + along * 30.0, dir, 30.0);
            cx.boat(end + along * 5.0 - dir * 6.0, syaw, &lib.m_red.clone(), false);
            // A couple of cottages up by the strip.
            cx.house(a.world(-40.0, v_base), yaw, 10.0, 8.0, 3.6, &lib.m_cream.clone(), Roof::Gable, &lib.m_slate.clone());
            cx.house(a.world(-10.0, v_base + 4.0), yaw, 9.0, 7.0, 3.6, &lib.m_white.clone(), Roof::Gable, &lib.m_red.clone());
        }
        AirportKind::Base => {
            // Three big hangars, tower, radar, fuel dump, bunkers, trucks, watchtowers.
            for k in 0..3 {
                cx.shed(a.world(-l * 0.5 + 260.0 + k as f32 * 60.0, w * 0.5 + 100.0), yaw, 44.0, 40.0, 12.0, &lib.m_olive.clone(), &lib.m_slate.clone());
            }
            cx.dish(a.world(l * 0.5 - 80.0, v_base + 40.0), 9.0);
            for k in 0..4 {
                cx.tank(a.world(l * 0.5 - 180.0 + k as f32 * 12.0, v_base + 20.0), 4.5, 6.0, &lib.m_olive.clone());
            }
            for k in 0..3 {
                let bp = a.world(l * 0.5 - 300.0 + k as f32 * 30.0, v_base + 60.0);
                cx.heap(bp, 11.0, 4.5, &lib.m_leaf.clone());
            }
            for k in 0..5 {
                cx.truck(a.world(-l * 0.5 + 250.0 + k as f32 * 9.0, w * 0.5 + 66.0), yaw + FRAC_PI_2, &lib.m_olive.clone(), &lib.m_olive.clone());
            }
            for u in [-l * 0.5 - 20.0, l * 0.5 + 20.0] {
                cx.watchtower(a.world(u, w * 0.5 + 110.0), yaw);
            }
            cx.flagpole(a.world(-l * 0.5 + 60.0, w * 0.5 + 100.0), &lib.m_navy.clone());
            let g = grid_for(a).unwrap();
            cx.fence_rect(a, g.u0 - 20.0, g.u0 + g.cols as f32 * g.pitch_u + 20.0, g.v0 - 10.0, g.v0 + g.rows as f32 * g.pitch_v + 20.0, &lib.m_grey.clone());
        }
        AirportKind::Outpost => {
            // Huts on stilts against the snow, an antenna, fuel drums, a snowcat and a flag.
            for k in 0..5 {
                let p = a.world(-60.0 + k as f32 * 22.0, v_base - 40.0 + (k % 2) as f32 * 14.0);
                cx.house(p, yaw, 9.0, 6.0, 3.2, &lib.m_orange.clone(), Roof::Gable, &lib.m_slate.clone());
            }
            cx.boxed(&lib.m_white.clone(), cx.ground(a.world(60.0, v_base - 30.0)), Vec3::new(1.2, 22.0, 1.2), yaw);
            for k in 0..8 {
                cx.cylinder(a.world(-80.0 + (k % 4) as f32 * 2.2, v_base - 60.0 + (k / 4) as f32 * 2.2), 0.6, 1.2, &lib.m_red.clone(), false);
            }
            cx.truck(a.world(20.0, v_base - 66.0), yaw + 0.6, &lib.m_red.clone(), &lib.m_dark.clone());
            cx.flagpole(a.world(0.0, v_base - 20.0), &lib.m_red.clone());
            cx.dish(a.world(90.0, v_base - 50.0), 5.0);
        }
        AirportKind::Oasis => {
            // Pond ringed with palms, mud houses with flat roofs, a market and a water tower.
            let pc = a.world(20.0, v_base + 60.0);
            let pond = cx.meshes.add(Cylinder::new(38.0, 0.4).mesh().resolution(24).build());
            cx.mesh(pond, &lib.m_blue.clone(), Transform::from_translation(cx.ground(pc) + Vec3::Y * 0.1));
            for k in 0..14 {
                let ang = k as f32 / 14.0 * TAU;
                let r = cx.rng.range(44.0, 54.0);
                cx.palm(pc + Vec2::new(ang.cos(), ang.sin()) * r);
            }
            for k in 0..12 {
                let ang = k as f32 / 12.0 * TAU + 0.2;
                let p = pc + Vec2::new(ang.cos(), ang.sin()) * cx.rng.range(80.0, 110.0);
                let (s, jy, hh) = (cx.rng.range(6.0, 11.0), cx.rng.range(-0.4, 0.4), cx.rng.range(3.0, 5.5));
                cx.house(p, yaw + jy, s, s * 0.8, hh, &lib.m_tan.clone(), Roof::Flat, &lib.m_tan.clone());
            }
            for k in 0..6 {
                let cm = cx.pick(&[&lib.m_red, &lib.m_white, &lib.m_orange]).clone();
                cx.stall(a.world(-60.0 + k as f32 * 7.0, v_base - 10.0), yaw, &cm);
            }
            cx.water_tower(a.world(90.0, v_base - 20.0));
            cx.truck(a.world(-40.0, v_base + 5.0), yaw + 0.3, &lib.m_white.clone(), &lib.m_tan.clone());
        }
        AirportKind::Hermit => {
            // A cabin, a woodpile, a vegetable patch and a jetty with a rowing boat.
            cx.house(a.world(0.0, v_base - 60.0), yaw, 8.0, 6.0, 3.4, &lib.m_brown.clone(), Roof::Gable, &lib.m_olive.clone());
            for k in 0..3 {
                cx.boxed(&lib.m_trunk.clone(), cx.ground(a.world(10.0 + k as f32 * 1.6, v_base - 60.0)), Vec3::new(1.4, 1.2, 3.0), yaw);
            }
            cx.fence_rect(a, -20.0, -4.0, v_base - 50.0, v_base - 36.0, &lib.m_brown.clone());
            cx.cylinder(a.world(0.0, v_base - 48.0), 1.6, 0.4, &lib.m_dark.clone(), false);
            if let Some((p, dir)) = shore(cx.terrain, a.def.pos, 150.0, 700.0) {
                let end = cx.pier(p, dir, 24.0);
                cx.boat(end + Vec2::new(-dir.y, dir.x) * 3.0 - dir * 4.0, crate::world::yaw_from_forward(dir), &lib.m_brown.clone(), false);
            }
        }
        AirportKind::Prison => {
            // Walled compound with cell blocks, guard towers, a yard, the car park and a bus.
            let c = a.world(0.0, v_base + 100.0);
            let rd = a.right();
            let ud = a.dir;
            let (hw, hd) = (70.0, 55.0);
            let g = cx.ground(c);
            for s in [-1.0, 1.0] {
                cx.boxed(&lib.m_concrete.clone(), g + Vec3::new(rd.x, 0.0, rd.y) * (s * hw), Vec3::new(2.0, 7.0, hd * 2.0), yaw);
                cx.boxed(&lib.m_concrete.clone(), g + Vec3::new(ud.x, 0.0, ud.y) * (s * hd), Vec3::new(hw * 2.0, 7.0, 2.0), yaw);
            }
            for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                cx.watchtower(c + ud * (su * hd) + rd * (sv * hw), yaw);
            }
            for k in 0..3 {
                cx.house(c + ud * (-35.0 + k as f32 * 35.0) + rd * 15.0, yaw, 24.0, 12.0, 8.0, &lib.m_grey.clone(), Roof::Flat, &lib.m_slate.clone());
            }
            cx.house(c - rd * 30.0, yaw, 40.0, 14.0, 6.0, &lib.m_concrete.clone(), Roof::Flat, &lib.m_slate.clone());
            cx.flagpole(c - rd * 48.0, &lib.m_navy.clone());
            for k in 0..4 {
                cx.car(c - rd * (hw + 12.0) + ud * (-20.0 + k as f32 * 7.0), yaw + FRAC_PI_2, &lib.m_white.clone());
            }
            cx.truck(c - rd * (hw + 12.0) + ud * 20.0, yaw + FRAC_PI_2, &lib.m_navy.clone(), &lib.m_navy.clone());
            for k in 0..8 {
                cx.lamp(c + rd * (hw + 4.0) * if k % 2 == 0 { 1.0 } else { -1.0 } + ud * (-45.0 + (k / 2) as f32 * 30.0));
            }
        }
        AirportKind::Resort => {
            // Hotel with a pool, tennis court, bungalow row, and a beach with umbrellas,
            // palms and a pier on the nearest shore.
            let hp = a.world(40.0, v_base + 40.0);
            cx.house(hp, yaw, 44.0, 16.0, 13.0, &lib.m_white.clone(), Roof::Flat, &lib.m_terracotta.clone());
            for k in 0..3 {
                let top = cx.ground(hp) + Vec3::Y * (13.5);
                cx.boxed(&lib.m_white.clone(), top + Quat::from_rotation_y(yaw) * Vec3::new(0.0, 0.0, -18.0 + k as f32 * 18.0), Vec3::new(6.0, 2.5, 8.0), yaw);
            }
            let pool = cx.meshes.add(Cuboid::new(14.0, 0.3, 24.0));
            cx.mesh(pool, &lib.m_blue.clone(), Transform { translation: a.world3(40.0, v_base + 68.0, 0.1), rotation: Quat::from_rotation_y(yaw), scale: Vec3::ONE });
            let court = cx.meshes.add(Cuboid::new(24.0, 0.2, 36.0));
            cx.mesh(court, &lib.m_green.clone(), Transform { translation: a.world3(100.0, v_base + 50.0, 0.1), rotation: Quat::from_rotation_y(yaw), scale: Vec3::ONE });
            cx.fence_rect(a, 82.0, 118.0, v_base + 38.0, v_base + 62.0, &lib.m_grey.clone());
            for k in 0..8 {
                cx.palm(a.world(-10.0 + k as f32 * 14.0, v_base + 90.0));
            }
            let (sp, dir) = shore(cx.terrain, a.def.pos, 200.0, 900.0).unwrap_or((a.world(0.0, -(w * 0.5 + 120.0)), -a.right()));
            let along = Vec2::new(-dir.y, dir.x);
            let syaw = crate::world::yaw_from_forward(dir);
            for k in 0..9 {
                let p = sp + along * (-100.0 + k as f32 * 25.0) - dir * 45.0;
                if cx.terrain.height(p.x, p.y) > 0.8 {
                    cx.house(p, syaw, 6.0, 5.0, 3.2, &lib.m_yellow.clone(), Roof::Pyramid, &lib.m_olive.clone());
                }
            }
            let umb = cx.meshes.add(meshes::cone_up(2.5, 1.0, 10));
            for k in 0..12 {
                let p = sp + along * (-70.0 + k as f32 * 12.0) - dir * 16.0;
                if cx.terrain.height(p.x, p.y) > 0.5 {
                    let g = cx.ground(p);
                    cx.boxed(&lib.m_white.clone(), g, Vec3::new(0.2, 2.3, 0.2), syaw);
                    let um = if k % 2 == 0 { lib.m_red.clone() } else { lib.m_blue.clone() };
                    cx.mesh(umb.clone(), &um, Transform::from_translation(g + Vec3::Y * 2.3));
                }
            }
            for k in 0..12 {
                let p = sp + along * (-110.0 + k as f32 * 20.0) - dir * cx.rng.range(24.0, 34.0);
                if cx.terrain.height(p.x, p.y) > 0.8 {
                    cx.palm(p);
                }
            }
            let end = cx.pier(sp + along * 60.0, dir, 45.0);
            cx.boat(end + along * 5.0 - dir * 10.0, syaw, &lib.m_white.clone(), false);
            cx.boat(end - along * 6.0 - dir * 20.0, syaw + 0.3, &lib.m_navy.clone(), true);
        }
        AirportKind::FishingVillage => {
            // Huts on stilts along the shore, two jetties with boats, drying racks, a market.
            let (sp, dir) = shore(cx.terrain, a.def.pos, 150.0, 800.0).unwrap_or((a.world(l * 0.5 + 40.0, 0.0), a.dir));
            let along = Vec2::new(-dir.y, dir.x);
            let syaw = crate::world::yaw_from_forward(dir);
            for k in 0..12 {
                let p = sp + along * (-80.0 + k as f32 * 15.0) - dir * cx.rng.range(10.0, 30.0);
                if cx.terrain.height(p.x, p.y) < 0.6 {
                    continue;
                }
                let g = cx.ground(p);
                for (sx, sz) in [(-2.5, -2.5), (2.5, -2.5), (-2.5, 2.5), (2.5, 2.5)] {
                    cx.boxed(&lib.m_trunk.clone(), g + Quat::from_rotation_y(syaw) * Vec3::new(sx, 0.0, sz), Vec3::new(0.3, 2.0, 0.3), syaw);
                }
                let wall = cx.pick(&[&lib.m_brown, &lib.m_blue, &lib.m_cream, &lib.m_tan]).clone();
                cx.boxed(&wall, g + Vec3::Y * 2.0, Vec3::new(6.0, 3.0, 6.0), syaw);
                cx.mesh(lib.pyramid.clone(), &lib.m_olive.clone(), Transform { translation: g + Vec3::Y * 5.0, rotation: Quat::from_rotation_y(syaw), scale: Vec3::new(7.4, 2.6, 7.4) });
            }
            for s in [-50.0, 40.0] {
                let end = cx.pier(sp + along * s, dir, 36.0);
                cx.boat(end + along * 4.0 - dir * 8.0, syaw, &lib.m_blue.clone(), false);
                cx.boat(end - along * 4.0 - dir * 16.0, syaw + 0.2, &lib.m_red.clone(), false);
            }
            cx.boat(sp + dir * 70.0 + along * 10.0, syaw + 1.2, &lib.m_navy.clone(), true);
            for k in 0..5 {
                let p = sp - dir * 45.0 + along * (-30.0 + k as f32 * 12.0);
                let g = cx.ground(p);
                cx.boxed(&lib.m_brown.clone(), g, Vec3::new(0.3, 2.5, 8.0), syaw);
                cx.boxed(&lib.m_grey.clone(), g + Vec3::Y * 2.3, Vec3::new(0.2, 0.2, 8.0), syaw);
            }
            cx.shed(a.world(-l * 0.5 + 220.0, w * 0.5 + 100.0), yaw, 20.0, 12.0, 5.0, &lib.m_blue.clone(), &lib.m_steel.clone());
            for k in 0..4 {
                cx.stall(a.world(-l * 0.5 + 250.0 + k as f32 * 6.0, w * 0.5 + 96.0), yaw, &lib.m_white.clone());
            }
            for k in 0..6 {
                let (jv, jy) = (cx.rng.range(-4.0, 4.0), cx.rng.range(-0.2, 0.2));
                cx.house(a.world(-l * 0.5 + 60.0 + k as f32 * 22.0, v_base + jv), yaw + jy, 7.0, 6.0, 3.4, &lib.m_cream.clone(), Roof::Pyramid, &lib.m_terracotta.clone());
            }
        }
        AirportKind::Research => {
            // Observatory domes on the hill side, a dish, the lab, masts, solar field, huts.
            let lab = a.world(0.0, v_base + 10.0);
            cx.house(lab, yaw, 36.0, 16.0, 6.0, &lib.m_white.clone(), Roof::Flat, &lib.m_slate.clone());
            for k in 0..3 {
                let p = a.world(-60.0 + k as f32 * 60.0, v_base + 60.0);
                let top = cx.cylinder(p, 7.0, 5.0, &lib.m_concrete.clone(), true);
                cx.dome(top, 7.0, &lib.m_white.clone());
            }
            cx.dish(a.world(70.0, v_base + 20.0), 10.0);
            for k in 0..3 {
                cx.boxed(&lib.m_white.clone(), cx.ground(a.world(-l * 0.5 + 80.0 + k as f32 * 25.0, v_base + 40.0)), Vec3::new(0.6, 22.0 + k as f32 * 4.0, 0.6), yaw);
            }
            for r in 0..3 {
                for k in 0..6 {
                    cx.solar(a.world(-120.0 + k as f32 * 10.0, v_base + 100.0 + r as f32 * 8.0), yaw + FRAC_PI_2);
                }
            }
            for k in 0..4 {
                cx.house(a.world(l * 0.5 - 200.0 + k as f32 * 18.0, v_base + 8.0), yaw, 10.0, 6.0, 3.2, &lib.m_orange.clone(), Roof::Gable, &lib.m_slate.clone());
            }
            cx.truck(a.world(40.0, v_base - 12.0), yaw + 0.5, &lib.m_red.clone(), &lib.m_dark.clone());
        }
        AirportKind::LumberCamp => {
            // Sawmill, log yard with piles and a loader crane, bunkhouses (grid), stumps.
            let mill = a.world(-l * 0.5 + 220.0, w * 0.5 + 110.0);
            cx.shed(mill, yaw, 36.0, 18.0, 8.0, &lib.m_brown.clone(), &lib.m_steel.clone());
            cx.cylinder(mill + a.dir * 14.0 + a.right() * 8.0, 1.0, 14.0, &lib.m_steel.clone(), true);
            let log = cx.meshes.add(Cylinder::new(0.6, 10.0).mesh().resolution(8).build());
            for pile in 0..5 {
                let base = a.world(-l * 0.5 + 300.0 + pile as f32 * 22.0, w * 0.5 + 100.0);
                let g = cx.ground(base);
                for row in 0..3 {
                    for k in 0..(4 - row) {
                        let off = Quat::from_rotation_y(yaw) * Vec3::new(k as f32 * 1.3 + row as f32 * 0.65, 0.6 + row as f32 * 1.1, 0.0);
                        cx.mesh(log.clone(), &lib.m_trunk.clone(), Transform { translation: g + off, rotation: Quat::from_rotation_y(yaw) * Quat::from_rotation_x(FRAC_PI_2), scale: Vec3::ONE });
                    }
                }
                cx.lib.solid_cylinder(base, 3.0, g.y, g.y + 3.5, "a log pile");
            }
            cx.crane(a.world(-l * 0.5 + 340.0, w * 0.5 + 130.0), yaw, 12.0, 18.0);
            for k in 0..2 {
                cx.truck(a.world(-l * 0.5 + 260.0 + k as f32 * 12.0, w * 0.5 + 76.0), yaw + FRAC_PI_2, &lib.m_yellow.clone(), &lib.m_trunk.clone());
            }
            for _ in 0..40 {
                let (ju, jv, jy) = (cx.rng.range(-l * 0.4, l * 0.4), cx.rng.range(0.0, 120.0), cx.rng.range(0.0, 3.0));
                let g = cx.ground(a.world(ju, v_base + 120.0 + jv)) - Vec3::Y * 0.2;
                cx.boxed(&lib.m_trunk.clone(), g, Vec3::new(1.2, 0.9, 1.2), jy);
            }
            for _ in 0..12 {
                let (ju, jv, ts) = (cx.rng.range(-l * 0.4, l * 0.4), cx.rng.range(0.0, 40.0), cx.rng.range(0.9, 1.4));
                cx.tree(a.world(ju, v_base + 250.0 + jv), ts, true);
            }
        }
        AirportKind::Mine => {
            // Headframe, crusher and conveyor, ore heaps, workshops, trucks, bunkhouses.
            let shaft = a.world(0.0, v_base + 40.0);
            let g = cx.ground(shaft);
            for (dx, dz) in [(-4.0, -4.0), (4.0, -4.0), (-4.0, 4.0), (4.0, 4.0)] {
                cx.boxed(&lib.m_steel.clone(), g + Quat::from_rotation_y(yaw) * Vec3::new(dx, 0.0, dz), Vec3::new(0.8, 24.0, 0.8), yaw);
            }
            cx.boxed(&lib.m_steel.clone(), g + Vec3::Y * 24.0, Vec3::new(11.0, 3.0, 11.0), yaw);
            let wheel = cx.meshes.add(Cylinder::new(3.2, 0.6).mesh().resolution(16).build());
            cx.mesh(wheel, &lib.m_red.clone(), Transform { translation: g + Vec3::Y * 29.0, rotation: Quat::from_rotation_y(yaw) * Quat::from_rotation_z(FRAC_PI_2), scale: Vec3::ONE });
            cx.shed(a.world(-70.0, v_base + 30.0), yaw, 28.0, 16.0, 10.0, &lib.m_brown.clone(), &lib.m_steel.clone());
            // Conveyor from the crusher up to the heap.
            let conv = cx.meshes.add(Cuboid::new(2.0, 1.0, 60.0));
            let cg = cx.ground(a.world(-30.0, v_base + 70.0));
            cx.mesh(conv, &lib.m_steel.clone(), Transform { translation: cg + Vec3::Y * 8.0, rotation: Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-0.2), scale: Vec3::ONE });
            for k in 0..3 {
                cx.boxed(&lib.m_steel.clone(), cg + Quat::from_rotation_y(yaw) * Vec3::new(0.0, 0.0, -20.0 + k as f32 * 20.0), Vec3::new(0.5, 6.0 + k as f32 * 4.0, 0.5), yaw);
            }
            for k in 0..6 {
                let (r, h) = (cx.rng.range(7.0, 11.0), cx.rng.range(4.0, 7.0));
                cx.heap(a.world(40.0 + k as f32 * 20.0, v_base + 60.0 + (k % 2) as f32 * 20.0), r, h, &lib.m_brown.clone());
            }
            for k in 0..3 {
                cx.truck(a.world(30.0 + k as f32 * 10.0, v_base + 10.0), yaw + FRAC_PI_2, &lib.m_yellow.clone(), &lib.m_dark.clone());
            }
            for k in 0..8 {
                cx.boxed(&lib.m_steel.clone(), cx.ground(a.world(-l * 0.5 + 80.0 + k as f32 * 4.5, w * 0.5 + 100.0)), Vec3::new(2.4, 1.8, 3.0), yaw);
            }
            cx.shed(a.world(l * 0.5 - 120.0, v_base + 20.0), yaw, 20.0, 12.0, 6.0, &lib.m_grey.clone(), &lib.m_slate.clone());
            cx.water_tower(a.world(l * 0.5 - 60.0, v_base + 40.0));
        }
        AirportKind::City => {
            // The grid wraps the strip; here the civic pieces: a cathedral square, a bus
            // station by the apron, a park, a water tower.
            let g = grid_for(a).unwrap();
            cx.church(a.world(g.u0 + 15.0 * g.pitch_u, g.v0 + 9.5 * g.pitch_v), yaw);
            cx.house(a.world(-l * 0.5 + 250.0, w * 0.5 + 100.0), yaw, 30.0, 10.0, 5.0, &lib.m_white.clone(), Roof::Flat, &lib.m_dark.clone());
            for k in 0..3 {
                cx.truck(a.world(-l * 0.5 + 236.0 + k as f32 * 9.0, w * 0.5 + 84.0), yaw + FRAC_PI_2, &lib.m_yellow.clone(), &lib.m_yellow.clone());
            }
            for _ in 0..14 {
                let (ju, jv, ts) = (cx.rng.range(-50.0, 50.0), cx.rng.range(-40.0, 40.0), cx.rng.range(0.7, 1.0));
                cx.tree(a.world(g.u0 + 4.0 * g.pitch_u + ju, g.v0 + 1.5 * g.pitch_v + jv), ts, false);
            }
            cx.water_tower(a.world(g.u0 + 17.0 * g.pitch_u, g.v0 + 1.0 * g.pitch_v));
        }
        AirportKind::Canyon => {
            // Quarry on the shelf beside the strip: crusher, ore heaps, trucks, huts, a crane.
            let v_shelf = w * 0.5 + 60.0;
            cx.shed(a.world(-l * 0.5 + 250.0, v_shelf + 20.0), yaw, 26.0, 14.0, 9.0, &lib.m_brown.clone(), &lib.m_steel.clone());
            for k in 0..5 {
                let (r, h) = (cx.rng.range(6.0, 10.0), cx.rng.range(4.0, 6.0));
                cx.heap(a.world(-l * 0.5 + 300.0 + k as f32 * 20.0, v_shelf + 30.0), r, h, &lib.m_terracotta.clone());
            }
            for k in 0..3 {
                cx.truck(a.world(-l * 0.5 + 200.0 + k as f32 * 10.0, v_shelf + 5.0), yaw + FRAC_PI_2, &lib.m_yellow.clone(), &lib.m_dark.clone());
            }
            cx.crane(a.world(-l * 0.5 + 400.0, v_shelf + 30.0), yaw, 12.0, 16.0);
            for k in 0..4 {
                cx.house(a.world(l * 0.5 - 150.0 + k as f32 * 16.0, v_shelf + 10.0), yaw, 9.0, 6.0, 3.2, &lib.m_tan.clone(), Roof::Gable, &lib.m_slate.clone());
            }
            cx.tank(a.world(l * 0.5 - 60.0, v_shelf + 12.0), 3.0, 5.0, &lib.m_white.clone());
            for k in 0..6 {
                cx.cylinder(a.world(l * 0.5 - 40.0 + (k % 3) as f32 * 2.2, v_shelf + 8.0 + (k / 3) as f32 * 2.2), 0.6, 1.2, &lib.m_red.clone(), false);
            }
            let conv = cx.meshes.add(Cuboid::new(2.0, 1.0, 50.0));
            let cg = cx.ground(a.world(-l * 0.5 + 280.0, v_shelf + 40.0));
            cx.mesh(conv, &lib.m_steel.clone(), Transform { translation: cg + Vec3::Y * 7.0, rotation: Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-0.18), scale: Vec3::ONE });
        }
        AirportKind::Mesa => {
            // Relay station on the summit: a tall lattice mast, dishes, huts, solar, a fence.
            let mp = a.world(40.0, v_base - 40.0);
            let g = cx.ground(mp);
            for (dx, dz) in [(-2.5, -2.5), (2.5, -2.5), (-2.5, 2.5), (2.5, 2.5)] {
                cx.boxed(&lib.m_red.clone(), g + Vec3::new(dx, 0.0, dz), Vec3::new(0.5, 46.0, 0.5), 0.0);
            }
            for k in 0..5 {
                let m = if k % 2 == 0 { lib.m_white.clone() } else { lib.m_red.clone() };
                cx.boxed(&m, g + Vec3::Y * (k as f32 * 9.0), Vec3::new(5.4, 0.5, 5.4), 0.0);
            }
            cx.boxed(&lib.m_white.clone(), g + Vec3::Y * 46.0, Vec3::new(0.4, 12.0, 0.4), 0.0);
            cx.dish(a.world(-20.0, v_base - 50.0), 7.0);
            cx.dish(a.world(90.0, v_base - 30.0), 5.0);
            for k in 0..3 {
                cx.house(a.world(-90.0 + k as f32 * 18.0, v_base - 20.0), yaw, 10.0, 7.0, 3.4, &lib.m_white.clone(), Roof::Gable, &lib.m_slate.clone());
            }
            for k in 0..8 {
                cx.solar(a.world(-100.0 + k as f32 * 10.0, v_base - 60.0), yaw + FRAC_PI_2);
            }
            cx.tank(a.world(120.0, v_base - 50.0), 2.6, 4.0, &lib.m_white.clone());
            cx.fence_rect(a, -120.0, 140.0, v_base - 75.0, v_base - 5.0, &lib.m_grey.clone());
            cx.flagpole(a.world(0.0, v_base - 10.0), &lib.m_navy.clone());
        }
        AirportKind::Wreck => {
            // A beached freighter, the salvage camp: tents, a fire, crates, a rusty truck.
            let (sp, dir) = shore(cx.terrain, a.def.pos, 150.0, 800.0).unwrap_or((a.world(-l * 0.5 - 90.0, -80.0), -a.dir));
            let hull = cx.meshes.add(meshes::tapered_box(16.0, 12.0, 70.0, 0.3, -1.0));
            let hp = sp + dir * 25.0;
            cx.mesh(hull, &lib.m_brick.clone(), Transform { translation: Vec3::new(hp.x, 2.0, hp.y), rotation: Quat::from_rotation_y(crate::world::yaw_from_forward(dir) + 0.6) * Quat::from_rotation_z(0.35), scale: Vec3::ONE });
            cx.lib.solid_cylinder(hp, 20.0, -2.0, 14.0, "the wreck");
            cx.boxed(&lib.m_white.clone(), Vec3::new(hp.x, 7.0, hp.y), Vec3::new(8.0, 7.0, 10.0), crate::world::yaw_from_forward(dir) + 0.6);
            let camp = a.world(20.0, v_base - 40.0);
            for k in 0..4 {
                let cm = cx.pick(&[&lib.m_olive, &lib.m_orange, &lib.m_tan]).clone();
                let jy = cx.rng.range(-0.3, 0.3);
                cx.tent(camp + a.dir * (k as f32 * 8.0) + a.right() * ((k % 2) as f32 * 6.0), yaw + jy, &cm);
            }
            cx.cylinder(camp + a.right() * 14.0, 1.5, 0.3, &lib.m_dark.clone(), false);
            for k in 0..5 {
                cx.boxed(&lib.m_brown.clone(), cx.ground(camp + a.dir * (-10.0 + k as f32 * 2.5) + a.right() * 20.0), Vec3::new(2.0, 2.0, 2.0), yaw + k as f32 * 0.3);
            }
            cx.truck(camp + a.dir * 30.0, yaw + 0.9, &lib.m_brick.clone(), &lib.m_brick.clone());
            cx.house(a.world(-30.0, v_base - 30.0), yaw, 8.0, 6.0, 3.2, &lib.m_grey.clone(), Roof::Gable, &lib.m_slate.clone());
        }
    }
}

/// Unit gable-roof prism: base 1 x 1 on y = 0, ridge along z at y = 1. Every face is wound
/// counter-clockwise seen from outside (back faces are culled: a wrong winding shows as a
/// see-through roof, which Adam spotted).
pub fn gable_mesh() -> Mesh {
    let a = Vec3::new(-0.5, 0.0, -0.5);
    let b = Vec3::new(0.5, 0.0, -0.5);
    let c = Vec3::new(0.5, 0.0, 0.5);
    let d = Vec3::new(-0.5, 0.0, 0.5);
    let r0 = Vec3::new(0.0, 1.0, -0.5);
    let r1 = Vec3::new(0.0, 1.0, 0.5);
    let centre = Vec3::new(0.0, 0.35, 0.0);
    let mut tris: Vec<[f32; 3]> = Vec::new();
    let mut tri = |p: Vec3, q: Vec3, r: Vec3| {
        let n = (q - p).cross(r - p);
        let outward = n.dot((p + q + r) / 3.0 - centre) >= 0.0;
        let (q, r) = if outward { (q, r) } else { (r, q) };
        for v in [p, q, r] {
            tris.push(v.to_array());
        }
    };
    // Two slopes (two triangles each), two gable ends, the underside.
    tri(a, d, r1);
    tri(a, r1, r0);
    tri(b, r0, r1);
    tri(b, r1, c);
    tri(a, r0, b);
    tri(c, r1, d);
    tri(a, b, c);
    tri(a, c, d);
    meshes::flat_mesh(tris, None)
}
