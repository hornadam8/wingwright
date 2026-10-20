//! A plane design: placed parts, their geometry, connectivity and aggregate statistics.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::parts::{Category, self, Control, Dir, PartDef, PartKind};

pub const GRID: f32 = 0.25;
pub const FUEL_KG_PER_L: f32 = 0.75;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlacedPart {
    pub id: String,
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    #[serde(default)]
    pub mirrored: bool,
    /// Stretched length (meters) for stretchable parts; otherwise the default length.
    pub length: f32,
    #[serde(default)]
    pub color: usize,
    /// Cross-section override (width, height) for parts that adapt to the face they sit on.
    #[serde(default)]
    pub section: Option<[f32; 2]>,
}

impl PlacedPart {
    pub fn new(def: &PartDef, pos: Vec3, rot: Quat, mirrored: bool) -> Self {
        Self {
            id: def.id.to_string(),
            pos: pos.to_array(),
            rot: rot.to_array(),
            mirrored,
            length: def.dims.z,
            color: def.color,
            section: None,
        }
    }

    pub fn def(&self) -> &'static PartDef {
        parts::part_or_panic(&self.id)
    }

    pub fn pos(&self) -> Vec3 {
        Vec3::from_array(self.pos)
    }

    pub fn rot(&self) -> Quat {
        Quat::from_array(self.rot).normalize()
    }

    /// Local (unrotated) size of this instance.
    pub fn dims(&self) -> Vec3 {
        let def = self.def();
        let mut d = def.dims;
        if def.stretchable() {
            d.z = self.length;
        }
        if let Some([w, h]) = self.section {
            d.x = w;
            d.y = h;
        }
        d
    }

    pub fn cost(&self) -> i64 {
        self.def().cost_for(self.length)
    }

    /// Cross-section area relative to the catalog dims (1.0 without a section override).
    pub fn section_factor(&self) -> f32 {
        let d = self.def().dims;
        match self.section {
            Some([w, h]) => (w * h) / (d.x * d.y).max(0.01),
            None => 1.0,
        }
    }

    pub fn mass(&self) -> f32 {
        self.def().mass_for(self.length) * self.section_factor()
    }

    /// Half extents of the rotated axis-aligned bounding box.
    pub fn half_extents(&self) -> Vec3 {
        rotated_half_extents(self.dims(), self.rot())
    }

    pub fn aabb(&self) -> (Vec3, Vec3) {
        let h = self.half_extents();
        let p = self.pos();
        (p - h, p + h)
    }

    /// Transform of the part in the plane's body frame. Mirroring is baked into the
    /// geometry by the visuals rather than expressed as a negative scale.
    pub fn transform(&self) -> Transform {
        Transform { translation: self.pos(), rotation: self.rot(), scale: Vec3::ONE }
    }

    /// Rotates a local direction into the body frame (accounting for mirroring).
    pub fn local_dir(&self, d: Vec3) -> Vec3 {
        let d = if self.mirrored { Vec3::new(-d.x, d.y, d.z) } else { d };
        self.rot() * d
    }
}

pub fn rotated_half_extents(dims: Vec3, rot: Quat) -> Vec3 {
    let m = Mat3::from_quat(rot);
    let h = dims * 0.5;
    Vec3::new(
        m.x_axis.x.abs() * h.x + m.y_axis.x.abs() * h.y + m.z_axis.x.abs() * h.z,
        m.x_axis.y.abs() * h.x + m.y_axis.y.abs() * h.y + m.z_axis.y.abs() * h.z,
        m.x_axis.z.abs() * h.x + m.y_axis.z.abs() * h.y + m.z_axis.z.abs() * h.z,
    )
}

pub fn snap(v: f32) -> f32 {
    (v / GRID).round() * GRID
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlaneDesign {
    pub name: String,
    pub parts: Vec<PlacedPart>,
}

impl Default for PlaneDesign {
    fn default() -> Self {
        Self::starter()
    }
}

/// Aggregate numbers shown in the builder and used to set up the flight model.
#[derive(Clone, Debug, Default)]
pub struct DesignStats {
    pub cost: i64,
    pub mass_empty: f32,
    pub fuel_capacity: f32,
    pub battery: f32,
    pub cargo_slots: u32,
    /// Cargo weight the gear can take on top of the plane with full fuel (kg); 0 without wheels.
    pub payload: f32,
    pub seats: u32,
    pub thrust: f32,
    pub wing_area: f32,
    pub engines: usize,
    pub wheels: usize,
    /// Sum of the wheels' load limits (kg).
    pub wheel_capacity: f32,
    pub has_elevator: bool,
    pub has_rudder: bool,
    /// Lowest tear-off speed among the wings and tail surfaces (m/s); 0 = none.
    pub speed_limit: f32,
    pub has_aileron: bool,
    /// Center of mass (empty + full fuel) in body frame.
    pub com: Vec3,
    /// Area-weighted center of lift of the main wings in body frame.
    pub col: Option<Vec3>,
    pub stall_speed: f32,
    pub top_speed: f32,
    pub endurance: f32,
    pub range: f32,
    pub warnings: Vec<String>,
}

impl PlaneDesign {
    /// Drops parts whose ids no longer exist in the catalog (old saves had separate
    /// aileron parts). Returns how many were removed.
    pub fn prune_unknown(&mut self) -> usize {
        let before = self.parts.len();
        self.parts.retain(|p| parts::part(&p.id).is_some());
        before - self.parts.len()
    }


    /// The plane every new campaign starts with: cockpit, engine, biplane wings, tail, wheels.
    pub fn starter() -> Self {
        let mut d = PlaneDesign { name: "Starter".into(), parts: Vec::new() };
        let id = Quat::IDENTITY;
        let cockpit = parts::part_or_panic("cockpit_basic");
        d.parts.push(PlacedPart::new(cockpit, Vec3::ZERO, id, false));
        // Engine on the nose.
        let eng = parts::part_or_panic("eng_small");
        d.parts.push(PlacedPart::new(eng, Vec3::new(0.0, 0.0, -1.0 - 0.45), id, false));
        // Fuselage behind, 2 m.
        let fus = parts::part_or_panic("fus_cargo");
        let mut f = PlacedPart::new(fus, Vec3::new(0.0, 0.0, 1.0 + 1.0), id, false);
        f.length = 2.0;
        d.parts.push(f);
        // Tail cone behind that.
        let tc = parts::part_or_panic("tail_cone");
        d.parts.push(PlacedPart::new(tc, Vec3::new(0.0, 0.0, 3.0 + tc.dims.z * 0.5), id, false));
        // Wings on both sides of the cockpit/fuselage junction.
        let wing = parts::part_or_panic("wing_biplane");
        let wx = 0.5 + wing.dims.x * 0.5;
        d.parts.push(PlacedPart::new(wing, Vec3::new(wx, 0.25, 0.75), id, false));
        d.parts.push(PlacedPart::new(wing, Vec3::new(-wx, 0.25, 0.75), id, true));
        // Tail wing behind the tail cone, fin on top of it (both carry their own control
        // surfaces, like the wings carry their ailerons).
        let el = parts::part_or_panic("elevator");
        // Tail panels: one on each side of the tail cone (elevators) and one stood on top
        // with its root down (the fin). Same part, three placements.
        let tz = 3.0 + tc.dims.z * 0.5 + 0.15;
        let ex = tc.dims.x * 0.5 + el.dims.x * 0.5;
        d.parts.push(PlacedPart::new(el, Vec3::new(ex, 0.0, tz), id, false));
        d.parts.push(PlacedPart::new(el, Vec3::new(-ex, 0.0, tz), id, true));
        let fin_rot = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        d.parts.push(PlacedPart::new(el, Vec3::new(0.0, tc.dims.y * 0.5 + el.dims.x * 0.5, tz), fin_rot, false));
        // Tricycle gear: short nose wheel under the cockpit, tall main wheels under the wings
        // (wide track) just behind the center of mass so the plane can rotate for takeoff.
        let nose = parts::part_or_panic("wheel_fixed");
        d.parts.push(PlacedPart::new(nose, Vec3::new(0.0, -0.5 - nose.dims.y * 0.5, -0.5), id, false));
        let main = parts::part_or_panic("wheel_tall");
        let my = 0.25 - wing.dims.y * 0.5 - main.dims.y * 0.5;
        d.parts.push(PlacedPart::new(main, Vec3::new(1.25, my, 1.25), id, false));
        d.parts.push(PlacedPart::new(main, Vec3::new(-1.25, my, 1.25), id, true));
        d
    }

    /// A bare design with the given cockpit part (falls back to the basic one).
    pub fn empty_with(cockpit_id: &str) -> Self {
        let mut d = Self::empty();
        if let Some(def) = parts::part(cockpit_id) {
            if def.category == Category::Cockpit && !d.parts.is_empty() {
                let old = d.parts[0].clone();
                d.parts[0] = PlacedPart::new(def, old.pos(), old.rot(), false);
            }
        }
        d
    }

    pub fn empty() -> Self {
        let cockpit = parts::part_or_panic("cockpit_basic");
        PlaneDesign {
            name: "New Design".into(),
            parts: vec![PlacedPart::new(cockpit, Vec3::ZERO, Quat::IDENTITY, false)],
        }
    }

    pub fn cockpit_index(&self) -> Option<usize> {
        self.parts.iter().position(|p| matches!(p.def().kind, PartKind::Cockpit { .. }))
    }

    pub fn total_cost(&self) -> i64 {
        self.parts.iter().map(|p| p.cost()).sum()
    }

    /// Whether `aabb` overlaps any existing part (with a small tolerance), excluding `skip`.
    /// `internal` boxes may sit inside containers; containers may enclose internal parts.
    pub fn overlaps(&self, min: Vec3, max: Vec3, skip: Option<usize>) -> bool {
        self.overlaps_kind(min, max, skip, false)
    }

    pub fn overlaps_kind(&self, min: Vec3, max: Vec3, skip: Option<usize>, internal: bool) -> bool {
        const TOL: f32 = 0.02;
        self.parts.iter().enumerate().any(|(i, p)| {
            if Some(i) == skip {
                return false;
            }
            let pdef = p.def();
            if internal && pdef.container() {
                return false;
            }
            if !internal && pdef.internal() {
                // A container being placed around an internal part is fine; anything else
                // should not intersect it, but internal parts are always inside a container
                // that blocks the space anyway.
                return false;
            }
            let (pmin, pmax) = p.aabb();
            min.x < pmax.x - TOL
                && max.x > pmin.x + TOL
                && min.y < pmax.y - TOL
                && max.y > pmin.y + TOL
                && min.z < pmax.z - TOL
                && max.z > pmin.z + TOL
        })
    }

    /// Index of a container part whose box fully encloses `aabb`, if any.
    #[allow(dead_code)]
    pub fn container_for(&self, min: Vec3, max: Vec3) -> Option<usize> {
        const TOL: f32 = 0.01;
        self.parts.iter().enumerate().find_map(|(i, p)| {
            if !p.def().container() {
                return None;
            }
            let (pmin, pmax) = p.aabb();
            let inside = min.x >= pmin.x - TOL && max.x <= pmax.x + TOL && min.y >= pmin.y - TOL && max.y <= pmax.y + TOL && min.z >= pmin.z - TOL && max.z <= pmax.z + TOL;
            if inside { Some(i) } else { None }
        })
    }

    /// Indices of parts whose bounding boxes touch part `i`.
    pub fn neighbors(&self, i: usize) -> Vec<usize> {
        const TOL: f32 = 0.03;
        let (amin, amax) = self.parts[i].aabb();
        let amin = amin - Vec3::splat(TOL);
        let amax = amax + Vec3::splat(TOL);
        self.parts
            .iter()
            .enumerate()
            .filter(|(j, p)| {
                if *j == i {
                    return false;
                }
                let (bmin, bmax) = p.aabb();
                amin.x <= bmax.x
                    && amax.x >= bmin.x
                    && amin.y <= bmax.y
                    && amax.y >= bmin.y
                    && amin.z <= bmax.z
                    && amax.z >= bmin.z
            })
            .map(|(j, _)| j)
            .collect()
    }

    /// Parts reachable from the cockpit (by touching bounding boxes), optionally treating
    /// `blocked` as removed.
    pub fn reachable_from_cockpit(&self, blocked: Option<usize>) -> Vec<bool> {
        let mut seen = vec![false; self.parts.len()];
        let Some(start) = self.cockpit_index() else { return seen };
        if Some(start) == blocked {
            return seen;
        }
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            for j in self.neighbors(i) {
                if Some(j) != blocked && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        seen
    }

    /// Removes part `i` and anything that would become disconnected from the cockpit.
    /// Returns the removed parts (for refunds).
    pub fn remove_part(&mut self, i: usize) -> Vec<PlacedPart> {
        if Some(i) == self.cockpit_index() {
            return Vec::new();
        }
        let reach = self.reachable_from_cockpit(Some(i));
        let mut removed = Vec::new();
        let mut kept = Vec::new();
        for (j, p) in self.parts.drain(..).enumerate() {
            if j == i || !reach[j] {
                removed.push(p);
            } else {
                kept.push(p);
            }
        }
        self.parts = kept;
        removed
    }

    /// Changes the length of stretchable part `i`, keeping the cockpit-side face fixed and
    /// shifting everything hanging off the far end.
    /// Replaces the cockpit with `def`, keeping the nose where it is and pushing every part
    /// behind the old cockpit back by the length difference. Returns the old cockpit's cost.
    pub fn swap_cockpit(&mut self, def: &'static PartDef) -> i64 {
        let Some(ci) = self.cockpit_index() else { return 0 };
        let old = self.parts[ci].clone();
        let old_cost = old.cost();
        let old_len = old.dims().z;
        let delta = def.dims.z - old_len;
        let rear = old.pos().z + old_len * 0.5;
        // Nose stays put: the new cockpit's centre moves back by half the growth.
        let pos = old.pos() + Vec3::new(0.0, 0.0, delta * 0.5);
        let (old_min, old_max) = old.aabb();
        let grow_x = (def.dims.x - old.dims().x) * 0.5;
        let grow_y = (def.dims.y - old.dims().y) * 0.5;
        self.parts[ci] = PlacedPart::new(def, pos, old.rot(), false);
        for (k, p) in self.parts.iter_mut().enumerate() {
            if k == ci {
                continue;
            }
            let c = p.pos();
            if c.z > rear - 0.05 {
                // Behind the old cockpit: everything moves back together.
                p.pos[2] += delta;
            } else if c.z > old_min.z - 0.05 {
                // Beside, above or below the old cockpit (wings, wheels, tanks on its faces):
                // step outward so they keep touching the wider or taller new cockpit.
                if c.x > old_max.x - 0.05 {
                    p.pos[0] += grow_x;
                } else if c.x < old_min.x + 0.05 {
                    p.pos[0] -= grow_x;
                }
                if c.y > old_max.y - 0.05 {
                    p.pos[1] += grow_y;
                } else if c.y < old_min.y + 0.05 {
                    p.pos[1] -= grow_y;
                }
            }
        }
        old_cost
    }

    pub fn set_length(&mut self, i: usize, new_len: f32) {
        let def = self.parts[i].def();
        let Some((lo, hi)) = def.stretch else { return };
        let new_len = new_len.clamp(lo, hi);
        let old_len = self.parts[i].length;
        let delta = new_len - old_len;
        if delta.abs() < 1e-4 {
            return;
        }
        // Axis along which the part stretches, in body frame.
        let axis = self.parts[i].local_dir(Vec3::Z);
        let com_side = self.cockpit_index().map(|c| self.parts[c].pos()).unwrap_or(Vec3::ZERO);
        let rel = self.parts[i].pos() - com_side;
        // Grow away from the cockpit.
        let dir = if rel.dot(axis) >= 0.0 { axis } else { -axis };
        // Parts on the far side: reachable from this part's far-face neighbours without
        // passing through this part or the cockpit.
        let far_face = self.parts[i].pos() + dir * (old_len * 0.5);
        let mut far = vec![false; self.parts.len()];
        let cockpit = self.cockpit_index();
        let mut stack: Vec<usize> = self
            .neighbors(i)
            .into_iter()
            .filter(|&j| (self.parts[j].pos() - far_face).dot(dir) > -0.05 && Some(j) != cockpit)
            .collect();
        for &j in &stack {
            far[j] = true;
        }
        while let Some(k) = stack.pop() {
            for j in self.neighbors(k) {
                if j != i && Some(j) != cockpit && !far[j] {
                    far[j] = true;
                    stack.push(j);
                }
            }
        }
        self.parts[i].length = new_len;
        let center_shift = dir * (delta * 0.5);
        let p = self.parts[i].pos() + center_shift;
        self.parts[i].pos = p.to_array();
        for (j, f) in far.iter().enumerate() {
            if *f {
                let np = self.parts[j].pos() + dir * delta;
                self.parts[j].pos = np.to_array();
            }
        }
    }

    pub fn stats(&self) -> DesignStats {
        let mut s = DesignStats::default();
        let mut weighted = Vec3::ZERO;
        let mut total_mass = 0.0;
        let mut lift_weighted = Vec3::ZERO;
        let mut lift_area = 0.0;
        let mut clmax_area = 0.0;
        let cdas = crate::flight::drag_areas(&self.parts);
        let drag_area: f32 = cdas.iter().map(|c| c.z).sum();
        for p in &self.parts {
            let def = p.def();
            s.cost += p.cost();
            let mut m = p.mass();
            s.mass_empty += m;
            match def.kind {
                PartKind::Cockpit { cargo, seats, fuel, max_speed } => {
                    if max_speed > 0.0 && (s.speed_limit == 0.0 || max_speed < s.speed_limit) {
                        s.speed_limit = max_speed;
                    }
                    s.cargo_slots += cargo;
                    s.seats += seats;
                    s.fuel_capacity += fuel;
                    m += fuel * FUEL_KG_PER_L;
                }
                PartKind::Fuselage { cargo_per_m, seats_per_m, .. } => {
                    s.cargo_slots += (cargo_per_m * p.length * p.section_factor()).floor() as u32;
                    s.seats += (seats_per_m * p.length * p.section_factor()).floor() as u32;
                }
                PartKind::Wing { area, cl_max, control, aileron, vertical, max_speed, .. } => {
                    if aileron {
                        s.has_aileron = true;
                    }
                    if max_speed > 0.0 && (s.speed_limit == 0.0 || max_speed < s.speed_limit) {
                        s.speed_limit = max_speed;
                    }
                    // Tail surfaces count as elevator or rudder by how they are mounted.
                    let upright = (p.rot() * Vec3::Y).x.abs() > 0.7;
                    let control = match control {
                        Control::Tail if upright => Control::Rudder,
                        Control::Tail => Control::Elevator,
                        c => c,
                    };
                    match control {
                        Control::None | Control::Tail => {
                            s.wing_area += area;
                            lift_weighted += p.pos() * area;
                            lift_area += area;
                            clmax_area += cl_max * area;
                        }
                        Control::Elevator => s.has_elevator = true,
                        Control::Aileron => s.has_aileron = true,
                        Control::Rudder => s.has_rudder = true,
                    }
                    if vertical {
                        // fins do not carry weight
                    }
                }
                PartKind::Engine { thrust, .. } => {
                    s.thrust += thrust;
                    s.engines += 1;
                }
                PartKind::Wheel { max_load, .. } => {
                    s.wheels += 1;
                    s.wheel_capacity += max_load;
                }
                PartKind::Tank { fuel, .. } => {
                    s.fuel_capacity += fuel;
                    m += fuel * FUEL_KG_PER_L;
                }
                PartKind::Battery { charge } => s.battery += charge,
                _ => {}
            }
            weighted += p.pos() * m;
            total_mass += m;
        }
        if total_mass > 0.0 {
            s.com = weighted / total_mass;
        }
        if lift_area > 0.0 {
            s.col = Some(lift_weighted / lift_area);
        }
        let weight = total_mass * 9.81;
        // What the wheels can carry beyond the fuelled plane: the hard cap on cargo weight (the
        // gear collapses above it). Shown next to the slot count, since slots say nothing about
        // mass and a crate weighs 20 kg or 130 kg depending on the goods.
        s.payload = if s.wheel_capacity > 0.0 { (s.wheel_capacity - total_mass).max(0.0) } else { 0.0 };
        if clmax_area > 0.0 {
            s.stall_speed = (2.0 * weight / (1.225 * clmax_area)).sqrt();
        }
        // Rough top speed: thrust = drag with cruise CL small.
        let cda = drag_area + s.wing_area * 0.03;
        if cda > 0.0 && s.thrust > 0.0 {
            s.top_speed = (2.0 * s.thrust * 0.75 / (1.225 * cda)).sqrt();
        }
        let burn: f32 = self
            .parts
            .iter()
            .filter_map(|p| match p.def().kind {
                PartKind::Engine { burn, electric: false, .. } => Some(burn),
                _ => None,
            })
            .sum();
        if burn > 0.0 {
            s.endurance = s.fuel_capacity / burn;
            s.range = s.endurance * s.top_speed * 0.8;
        }
        if s.engines == 0 {
            s.warnings.push("No engine: this is a glider.".into());
        }
        if s.wheels == 0 {
            s.warnings.push("No landing gear: expect a belly landing.".into());
        } else if total_mass > s.wheel_capacity {
            s.warnings.push("Wheel load too heavy: add more wheels or stronger wheels.".into());
        }
        if s.wing_area <= 0.0 {
            s.warnings.push("No wings.".into());
        }
        if !s.has_elevator {
            s.warnings.push("No elevator: you cannot control pitch.".into());
        }
        if !s.has_aileron {
            s.warnings.push("No ailerons: you cannot control roll.".into());
        }
        // Wheels: the plane sits on the lowest wheel at each end. If the rear wheels hang
        // lower than the front ones it sits nose-down and cannot rotate for take-off.
        let wheels: Vec<(f32, f32)> = self
            .parts
            .iter()
            .filter(|p| p.def().is_wheel())
            .map(|p| {
                let (min, _) = p.aabb();
                (p.pos().z, min.y)
            })
            .collect();
        if wheels.len() >= 2 {
            let front = wheels.iter().min_by(|a, b| a.0.partial_cmp(&b.0).unwrap()).unwrap();
            let rear = wheels.iter().max_by(|a, b| a.0.partial_cmp(&b.0).unwrap()).unwrap();
            if rear.1 < front.1 - 0.08 && rear.0 - front.0 > 0.5 {
                s.warnings.push("Nose-down on the wheels: the rear wheels hang lower than the front ones, so the plane cannot rotate for take-off.".into());
            }
        }
        if s.speed_limit > 0.0 && s.top_speed > s.speed_limit {
            s.warnings.push(format!("Something tears off at {:.0} km/h (wings, tail or an open cockpit): throttle back or upgrade.", s.speed_limit * 3.6));
        }
        if !s.has_rudder {
            s.warnings.push("No rudder: no yaw control or ground steering.".into());
        }
        if let Some(col) = s.col {
            // Body frame: forward is -Z, so "ahead" means smaller z.
            if s.com.z > col.z + 0.3 {
                s.warnings.push("Center of mass is behind the wings: unstable in pitch.".into());
            } else if s.com.z < col.z - 2.5 {
                s.warnings.push("Center of mass is far ahead of the wings: nose-heavy.".into());
            }
        }
        if weight > 0.0 && s.thrust > 0.0 && s.thrust / weight < 0.15 {
            s.warnings.push("Very low thrust-to-weight ratio.".into());
        }
        if s.stall_speed > 45.0 {
            s.warnings.push("Stall speed is very high: add wing area.".into());
        }
        let electric = self.parts.iter().any(|p| matches!(p.def().kind, PartKind::Engine { electric: true, .. }));
        if electric && s.battery <= 0.0 {
            s.warnings.push("Electric motor without a battery pack.".into());
        }
        if total_mass > 0.0 && s.thrust > 0.0 && s.top_speed < s.stall_speed * 1.1 {
            s.warnings.push("Not enough thrust to reach flying speed.".into());
        }
        s
    }
}

/// Snap a quaternion to the nearest of the 24 axis-aligned orientations.
pub fn snap_rotation(q: Quat) -> Quat {
    let m = Mat3::from_quat(q);
    let snap_axis = |v: Vec3| Dir::from_vec(v).vec();
    let x = snap_axis(m.x_axis);
    let mut y = snap_axis(m.y_axis);
    if x.dot(y).abs() > 0.5 {
        // degenerate; pick another axis
        y = if x.y.abs() < 0.5 { Vec3::Y } else { Vec3::Z };
    }
    let z = x.cross(y);
    Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_design_is_connected_and_sane() {
        let d = PlaneDesign::starter();
        let reach = d.reachable_from_cockpit(None);
        assert!(reach.iter().all(|r| *r), "every starter part touches the structure");
        for (i, p) in d.parts.iter().enumerate() {
            let (min, max) = p.aabb();
            assert!(!d.overlaps(min, max, Some(i)), "part {} ({}) overlaps another part", i, p.id);
        }
        let s = d.stats();
        assert_eq!(s.engines, 1);
        assert_eq!(s.wheels, 3);
        assert!(s.has_elevator && s.has_rudder && s.has_aileron);
        assert!(s.wing_area > 6.0);
        assert!(s.stall_speed > 20.0 && s.stall_speed < 35.0, "stall speed {}", s.stall_speed);
        assert!(s.top_speed > s.stall_speed * 1.2, "top {} stall {}", s.top_speed, s.stall_speed);
        assert!(s.cost > 1000 && s.cost < 2000, "cost {}", s.cost);
        assert!(s.warnings.is_empty(), "warnings: {:?}", s.warnings);
        // Payload is what the gear has left after the fuelled plane.
        let fuelled = s.mass_empty + s.fuel_capacity * FUEL_KG_PER_L;
        assert!(s.payload > 100.0, "payload {}", s.payload);
        assert!((s.payload + fuelled - s.wheel_capacity).abs() < 1e-2, "payload {} fuelled {} gear {}", s.payload, fuelled, s.wheel_capacity);
    }

    #[test]
    fn removing_a_fuselage_removes_the_tail_behind_it() {
        let mut d = PlaneDesign::starter();
        let fus = d.parts.iter().position(|p| p.id == "fus_cargo").unwrap();
        let before = d.parts.len();
        let removed = d.remove_part(fus);
        assert!(removed.len() >= 4, "fuselage, tail cone, elevator, rudder, main wheels: {}", removed.len());
        assert_eq!(d.parts.len() + removed.len(), before);
        assert!(d.parts.iter().any(|p| p.id == "cockpit_basic"));
        assert!(!d.parts.iter().any(|p| p.id == "tail_cone"));
        // The cockpit can never be removed.
        let c = d.cockpit_index().unwrap();
        assert!(d.remove_part(c).is_empty());
    }

    #[test]
    fn stretching_shifts_parts_behind() {
        let mut d = PlaneDesign::starter();
        let fus = d.parts.iter().position(|p| p.id == "fus_cargo").unwrap();
        let tail = d.parts.iter().position(|p| p.id == "tail_cone").unwrap();
        let tail_z = d.parts[tail].pos().z;
        let cockpit_z = d.parts[d.cockpit_index().unwrap()].pos().z;
        d.set_length(fus, 3.0);
        assert!((d.parts[fus].length - 3.0).abs() < 1e-4);
        assert!((d.parts[tail].pos().z - (tail_z + 1.0)).abs() < 1e-4, "tail moved back by the extra meter");
        assert!((d.parts[d.cockpit_index().unwrap()].pos().z - cockpit_z).abs() < 1e-4);
        // Still connected and free of overlaps.
        assert!(d.reachable_from_cockpit(None).iter().all(|r| *r));
        for (i, p) in d.parts.iter().enumerate() {
            let (min, max) = p.aabb();
            assert!(!d.overlaps(min, max, Some(i)), "part {} overlaps after stretch", p.id);
        }
        d.set_length(fus, 1.0);
        assert!((d.parts[tail].pos().z - (tail_z - 1.0)).abs() < 1e-4);
    }

    #[test]
    fn design_round_trips_through_json() {
        let d = PlaneDesign::starter();
        let text = serde_json::to_string(&d).unwrap();
        let back: PlaneDesign = serde_json::from_str(&text).unwrap();
        assert_eq!(back.parts.len(), d.parts.len());
        assert_eq!(back.total_cost(), d.total_cost());
    }

    #[test]
    fn rotated_extents_swap_axes() {
        let he = rotated_half_extents(Vec3::new(2.0, 1.0, 4.0), Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        assert!((he.x - 2.0).abs() < 1e-4 && (he.z - 1.0).abs() < 1e-4 && (he.y - 0.5).abs() < 1e-4);
        let q = snap_rotation(Quat::from_rotation_y(1.5));
        let v = q * Vec3::Z;
        assert!((v.x.abs() - 1.0).abs() < 1e-4, "snapped to an axis: {v:?}");
    }
}
