//! Flight: the plane is a single rigid body with forces from wings, engines, drag and wheels.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use bevy_egui::{egui, EguiContextPass, EguiContexts};
use std::f32::consts::{FRAC_PI_2, PI};

use crate::design::{PlacedPart, PlaneDesign, FUEL_KG_PER_L};
use crate::missions::{self, MissionDef, FRAGILE_ALL_BREAK, FRAGILE_MAX_IMPACT};
use crate::parts::{Control, PartDef, PartKind};
use crate::partvis::{self, ChuteCanopy, DragChuteCanopy, GearVisual, PartIndex, Propeller};
use crate::profile::{self, Profile};
use crate::ui::{self, card, pill};
use crate::world::{self, Airport, Rng, Terrain};
use crate::{GameState, MainCamera, MapTexture, MaterialCache, Toasts, UiFocus};

const RHO: f32 = 1.225;
const G: f32 = 9.81;
const SUBSTEP: f32 = 1.0 / 240.0;
const MAX_ELEVATOR: f32 = 0.25;
const MAX_AILERON: f32 = 0.18;
const MAX_RUDDER: f32 = 0.25;
const CL_SLOPE: f32 = 5.2;
/// Lifting wings are mounted with a small built-in incidence so level flight makes lift.
const WING_INCIDENCE: f32 = 3.0 * PI / 180.0;

pub struct FlightPlugin;

impl Plugin for FlightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sim>()
            .init_resource::<FlightUi>()
            .add_systems(OnEnter(GameState::Flight), enter_flight)
            .add_systems(OnExit(GameState::Flight), exit_flight)
            .add_systems(OnEnter(GameState::MainMenu), reset_sim)
            .add_systems(
                Update,
                (flight_input, crate::autotest::flight_override, flight_physics, flight_events, flight_visuals, flight_camera, debris_update)
                    .chain()
                    .run_if(in_state(GameState::Flight)),
            )
            .add_systems(EguiContextPass, flight_ui.run_if(in_state(GameState::Flight)));
    }
}

// ---------------------------------------------------------------------------------------
// simulation data

#[derive(Clone)]
struct Aero {
    area: f32,
    ar: f32,
    cl_max: f32,
    cd0: f32,
    control: Control,
    /// Built-in aileron: a fraction of the wing deflects with the roll input.
    aileron: bool,
    /// Tears off above this airspeed (m/s); 0 = never.
    max_speed: f32,
    /// Rotation from the aero frame (X span, Y lift normal, -Z forward) to the body frame.
    frame: Quat,
}

/// How much of a wing's angle of attack the built-in aileron changes at full stick.
const AILERON_FRAC: f32 = 0.3;

#[derive(Clone)]
struct SimPart {
    def: &'static PartDef,
    pos: Vec3,
    rot: Quat,
    dims: Vec3,
    he: Vec3,
    cda: Vec3,
    aero: Option<Aero>,
    thrust_dir: Vec3,
    mass_dry: f32,
    fuel_cap: f32,
    cargo_slots: f32,
    retractable: bool,
}

#[derive(Clone)]
struct Wheel {
    part: usize,
    contact: Vec3,
    steer_sign: f32,
    in_contact: bool,
    max_impact: f32,
}

#[derive(Clone)]
pub struct CargoItem {
    pub mission: &'static MissionDef,
    pub expires: Option<f64>,
}

#[derive(Debug, Clone)]
enum SimEvent {
    Crash(String),
    Touchdown(f32),
    OutOfFuel,
}

#[derive(Resource, Default)]
pub struct Sim {
    pub active: bool,
    parts: Vec<SimPart>,
    wheels: Vec<Wheel>,
    pub pos: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    pub ang_vel: Vec3,
    pub com: Vec3,
    pub mass: f32,
    inertia: Mat3,
    pub throttle: f32,
    pitch_in: f32,
    roll_in: f32,
    yaw_in: f32,
    pitch_cmd: f32,
    roll_cmd: f32,
    yaw_cmd: f32,
    pub fuel: f32,
    pub fuel_cap: f32,
    pub charge: f32,
    pub charge_cap: f32,
    pub has_fuel_engines: bool,
    pub has_electric: bool,
    pub gear_down: bool,
    pub has_retract: bool,
    pub chute: bool,
    pub has_chute: bool,
    pub drag_chute: bool,
    pub has_drag_chute: bool,
    pub cargo: Vec<CargoItem>,
    pub cargo_slots: u32,
    pub seats: u32,
    pub on_ground: bool,
    pub airspeed: f32,
    pub stalled: bool,
    pub aoa: f32,
    pub crashed: Option<String>,
    pub crash_timer: f32,
    pub stuck: bool,
    pub parked: Option<&'static str>,
    pub time: f64,
    pub distance: f32,
    pub touchdown_impact: f32,
    /// Seconds left before another touchdown counts (bounces do not re-break cargo).
    pub touchdown_cd: f32,
    /// Test Pilot clock: sim time of the first load, and the final time once delivered.
    pub trial_start: Option<f64>,
    pub trial_done: Option<f64>,
    pub oob: bool,
    pub max_extent: f32,
    events: Vec<SimEvent>,
    out_of_fuel_warned: bool,
    /// Fuel carried over from the last flight when the plane was edited at an airport.
    prev_fuel: Option<f32>,
    /// Wall time spent in the last physics update (diagnostics).
    pub physics_ms: f32,
    /// Sum of wheel load limits (kg); above this the gear is overloaded.
    pub wheel_capacity: f32,
    /// A non-wheel part is touching the ground (belly or wingtip skid).
    pub scraping: bool,
    body_contact: Vec<bool>,
}

/// Drag areas (Cd * A) of a part along its local axes.
fn local_drag_area(def: &PartDef, dims: Vec3) -> Vec3 {
    let (w, h, l) = (dims.x, dims.y, dims.z);
    let (cx, cy, cz) = match def.kind {
        PartKind::Cockpit { .. } => (0.9, 0.9, 0.42),
        PartKind::Fuselage { cd_z, .. } => (0.9, 0.9, cd_z),
        PartKind::Cone { cd_z, .. } => (0.5, 0.5, cd_z),
        PartKind::Boom | PartKind::Hub => (0.8, 0.8, 0.6),
        PartKind::Wing { .. } => (0.0, 0.0, 0.0),
        PartKind::Engine { prop, .. } => (0.8, 0.8, if prop { 0.3 } else { 0.25 }),
        PartKind::Wheel { .. } => (0.5, 0.4, 0.25),
        PartKind::Tank { external, .. } => (0.8, 0.8, if external { 0.22 } else { 0.7 }),
        PartKind::Battery { .. } => (0.8, 0.8, 0.8),
        PartKind::Parachute | PartKind::DragChute => (0.8, 0.8, 0.8),
    };
    Vec3::new(h * l * cx, w * l * cy, w * h * cz)
}

/// Body-frame drag areas for every part. Parts sitting directly behind another part have
/// most of their frontal drag shadowed, so a fuselage made of several sections does not pay
/// for its cross-section several times.
pub fn drag_areas(parts: &[PlacedPart]) -> Vec<Vec3> {
    let boxes: Vec<(Vec3, Vec3)> = parts.iter().map(|p| p.aabb()).collect();
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if p.def().internal() {
                return Vec3::ZERO;
            }
            let mut cda = part_drag_area(p);
            if p.def().is_wing() {
                return cda;
            }
            let (min, max) = boxes[i];
            let area = (max.x - min.x) * (max.y - min.y);
            let mut covered = 0.0;
            for (j, (bmin, bmax)) in boxes.iter().enumerate() {
                if j == i || parts[j].def().is_wing() {
                    continue;
                }
                // Only parts just ahead (smaller z) shadow this one.
                if bmax.z > min.z + 0.3 || bmax.z < min.z - 1.5 {
                    continue;
                }
                let ox = (max.x.min(bmax.x) - min.x.max(bmin.x)).max(0.0);
                let oy = (max.y.min(bmax.y) - min.y.max(bmin.y)).max(0.0);
                covered += ox * oy;
            }
            if area > 0.0 {
                let frac = (covered / area).min(1.0);
                cda.z *= 1.0 - 0.85 * frac;
            }
            cda
        })
        .collect()
}

/// Body-frame drag area of a placed part, ignoring shadowing.
pub fn part_drag_area(p: &PlacedPart) -> Vec3 {
    let local = local_drag_area(p.def(), p.dims());
    let m = Mat3::from_quat(p.rot());
    Vec3::new(
        m.x_axis.x.abs() * local.x + m.y_axis.x.abs() * local.y + m.z_axis.x.abs() * local.z,
        m.x_axis.y.abs() * local.x + m.y_axis.y.abs() * local.y + m.z_axis.y.abs() * local.z,
        m.x_axis.z.abs() * local.x + m.y_axis.z.abs() * local.y + m.z_axis.z.abs() * local.z,
    )
}

/// Lift coefficient against angle of attack: linear to the stall angle, a short plateau
/// (about 4 degrees) at max lift, then a gradual loss of a third over the next 20 degrees
/// and a taper to nothing at 90. Forgiving on purpose: Adam found the old cliff too twitchy.
fn cl_curve(a: f32, cl_max: f32) -> f32 {
    const PLATEAU: f32 = 0.07;
    const DROP: f32 = 0.35;
    let a_stall = cl_max / CL_SLOPE;
    let s = a.signum();
    let m = a.abs();
    if m <= a_stall {
        CL_SLOPE * a
    } else if m <= a_stall + PLATEAU {
        s * cl_max
    } else if m <= a_stall + PLATEAU + DROP {
        s * (cl_max - (m - a_stall - PLATEAU) / DROP * cl_max * 0.35)
    } else if m < FRAC_PI_2 {
        let start = a_stall + PLATEAU + DROP;
        s * 0.65 * cl_max * (1.0 - (m - start) / (FRAC_PI_2 - start))
    } else {
        0.0
    }
}

impl Sim {
    pub fn build(&mut self, design: &PlaneDesign) {
        *self = Sim::default();
        self.rot = Quat::IDENTITY;
        self.gear_down = true;
        let stats = design.stats();
        self.cargo_slots = stats.cargo_slots;
        self.seats = stats.seats;
        self.fuel_cap = stats.fuel_capacity;
        self.fuel = self.fuel_cap;
        self.charge_cap = stats.battery;
        self.charge = self.charge_cap;
        let cdas = drag_areas(&design.parts);
        for (p, cda) in design.parts.iter().zip(cdas) {
            let def = p.def();
            let dims = p.dims();
            let rot = p.rot();
            let mut sp = SimPart {
                def,
                pos: p.pos(),
                rot,
                dims,
                he: p.half_extents(),
                cda,
                aero: None,
                thrust_dir: p.local_dir(Vec3::NEG_Z),
                mass_dry: p.mass(),
                fuel_cap: 0.0,
                cargo_slots: 0.0,
                retractable: false,
            };
            match def.kind {
                PartKind::Wing { area, span, cl_max, cd0, control, aileron, vertical, max_speed, .. } => {
                    let mut frame = if vertical { rot * Quat::from_rotation_z(FRAC_PI_2) } else { rot };
                    if control == Control::None {
                        frame *= Quat::from_rotation_x(WING_INCIDENCE);
                    }
                    // Short stubby wings are forgiven a little: induced drag uses an effective
                    // aspect ratio of at least 3.5.
                    sp.aero = Some(Aero { area, ar: (span * span / area).max(3.5), cl_max, cd0, control, aileron, max_speed, frame });
                }
                PartKind::Engine { electric, .. } => {
                    if electric {
                        self.has_electric = true;
                    } else {
                        self.has_fuel_engines = true;
                    }
                }
                PartKind::Cockpit { fuel, cargo, .. } => {
                    sp.fuel_cap = fuel;
                    sp.cargo_slots = cargo as f32;
                }
                PartKind::Fuselage { cargo_per_m, .. } => sp.cargo_slots = cargo_per_m * p.length * p.section_factor(),
                PartKind::Tank { fuel, .. } => sp.fuel_cap = fuel,
                PartKind::Wheel { max_impact, retractable, max_load } => {
                    sp.retractable = retractable;
                    if retractable {
                        self.has_retract = true;
                    }
                    self.wheel_capacity += max_load;
                    self.wheels.push(Wheel {
                        part: self.parts.len(),
                        contact: p.pos() + Vec3::new(0.0, -sp.he.y, 0.0),
                        steer_sign: 0.0,
                        in_contact: false,
                        max_impact,
                    });
                }
                PartKind::Parachute => self.has_chute = true,
                PartKind::DragChute => self.has_drag_chute = true,
                _ => {}
            }
            let ext = p.pos().length() + sp.he.length();
            self.max_extent = self.max_extent.max(ext);
            self.parts.push(sp);
        }
        self.update_mass();
        // Steering: the wheel group farthest from the center of mass steers.
        if self.wheels.len() >= 2 {
            let zs: Vec<f32> = self.wheels.iter().map(|w| w.contact.z - self.com.z).collect();
            let zmin = zs.iter().cloned().fold(f32::MAX, f32::min);
            let zmax = zs.iter().cloned().fold(f32::MIN, f32::max);
            if zmin < -0.2 && zmax > 0.2 {
                let (group_z, sign) = if -zmin > zmax { (zmin, 1.0) } else { (zmax, -1.0) };
                for (w, z) in self.wheels.iter_mut().zip(zs.iter()) {
                    if (z - group_z).abs() < 0.6 {
                        w.steer_sign = sign;
                    }
                }
            } else {
                // Only main wheels (tail or nose dragging): the whole gear steers, like
                // braking one side, so taxiing is still possible.
                for w in self.wheels.iter_mut() {
                    w.steer_sign = 1.0;
                }
            }
        } else if let Some(w) = self.wheels.first_mut() {
            w.steer_sign = 1.0;
        }
        self.active = true;
    }

    pub fn cargo_mass(&self) -> f32 {
        self.cargo.iter().map(|c| c.mission.cargo.mass()).sum()
    }

    fn update_mass(&mut self) {
        let fuel_frac = if self.fuel_cap > 0.0 { self.fuel / self.fuel_cap } else { 0.0 };
        let total_slots: f32 = self.parts.iter().map(|p| p.cargo_slots).sum::<f32>().max(1.0);
        let cargo = self.cargo_mass();
        let mut total = 0.0;
        let mut weighted = Vec3::ZERO;
        let masses: Vec<f32> = self
            .parts
            .iter()
            .map(|p| p.mass_dry + p.fuel_cap * fuel_frac * FUEL_KG_PER_L + cargo * p.cargo_slots / total_slots)
            .collect();
        for (p, m) in self.parts.iter().zip(masses.iter()) {
            total += m;
            weighted += p.pos * *m;
        }
        self.mass = total.max(1.0);
        self.com = weighted / self.mass;
        let mut inertia = Mat3::ZERO;
        for (p, m) in self.parts.iter().zip(masses.iter()) {
            let d = p.dims;
            let local = Mat3::from_diagonal(Vec3::new(
                m / 12.0 * (d.y * d.y + d.z * d.z),
                m / 12.0 * (d.x * d.x + d.z * d.z),
                m / 12.0 * (d.x * d.x + d.y * d.y),
            ));
            let r = Mat3::from_quat(p.rot);
            let rotated = r * local * r.transpose();
            let off = p.pos - self.com;
            let d2 = off.length_squared();
            let par = Mat3::from_diagonal(Vec3::splat(d2)) - Mat3::from_cols(off * off.x, off * off.y, off * off.z);
            inertia = inertia + rotated + par * *m;
        }
        // Keep it well-conditioned.
        inertia = inertia + Mat3::from_diagonal(Vec3::splat(self.mass * 0.05));
        self.inertia = inertia;
    }

    fn cargo_free(&self, passengers: bool) -> u32 {
        let used = self.cargo.iter().filter(|c| c.mission.cargo.is_passenger() == passengers).count() as u32;
        let cap = if passengers { self.seats } else { self.cargo_slots };
        cap.saturating_sub(used)
    }

    pub fn set_cmds(&mut self, pitch: f32, roll: f32, yaw: f32) {
        self.pitch_cmd = pitch;
        self.roll_cmd = roll;
        self.yaw_cmd = yaw;
    }

    pub fn root_transform(&self) -> Transform {
        Transform { translation: self.pos - self.rot * self.com, rotation: self.rot, scale: Vec3::ONE }
    }

    fn speed(&self) -> f32 {
        self.vel.length()
    }

    /// Places the plane on the ground at `point`, heading `yaw`.
    pub fn place(&mut self, terrain: &Terrain, point: Vec3, yaw: f32) {
        self.rot = Quat::from_rotation_y(yaw);
        self.vel = Vec3::ZERO;
        self.ang_vel = Vec3::ZERO;
        self.throttle = 0.0;
        let lowest = if self.wheels.is_empty() {
            self.parts.iter().map(|p| p.pos.y - p.he.y).fold(f32::MAX, f32::min)
        } else {
            self.wheels.iter().map(|w| w.contact.y).fold(f32::MAX, f32::min)
        };
        let ground = terrain.height(point.x, point.z);
        let root_y = ground + 0.03 - lowest;
        let root = Vec3::new(point.x, root_y, point.z);
        self.pos = root + self.rot * self.com;
        for w in self.wheels.iter_mut() {
            w.in_contact = false;
        }
    }

    fn step(&mut self, dt: f32, terrain: &Terrain, assist: bool) {
        if self.stuck || self.crashed.is_some() {
            return;
        }
        // Smooth the controls.
        let k = (10.0 * dt).min(1.0);
        self.pitch_in += (self.pitch_cmd - self.pitch_in) * k;
        self.roll_in += (self.roll_cmd - self.roll_in) * k;
        self.yaw_in += (self.yaw_cmd - self.yaw_in) * k;

        let rot = self.rot;
        let inv_rot = rot.inverse();
        let v_body = inv_rot * self.vel;
        let w_body = inv_rot * self.ang_vel;
        let com = self.com;
        let airspeed = self.vel.length();
        let mut f_body = Vec3::ZERO;
        let mut t_body = Vec3::ZERO;
        let mut stalled = false;
        let mut aoa_sum = 0.0;
        let mut aoa_n = 0;
        let mut thrust_total = 0.0;
        let mut burn = 0.0;
        let mut charge_use = 0.0;
        let fuel_ok = self.fuel > 0.0;
        let charge_ok = self.charge > 0.0;
        let gear_down = self.gear_down;

        for p in &self.parts {
            let r = p.pos - com;
            let v_point = v_body + w_body.cross(r);
            if p.retractable && !gear_down {
                continue;
            }
            if let Some(a) = &p.aero {
                let inv = a.frame.inverse();
                let va = inv * v_point;
                let q2 = va.y * va.y + va.z * va.z;
                if q2 > 0.05 {
                    let alpha = (-va.y).atan2(-va.z);
                    let up_body = a.frame * Vec3::Y;
                    // A tail surface is an elevator when its lift axis points up and a
                    // rudder when the part stands upright (lift axis sideways).
                    let control = match a.control {
                        Control::Tail => {
                            if up_body.x.abs() > 0.7 {
                                Control::Rudder
                            } else {
                                Control::Elevator
                            }
                        }
                        c => c,
                    };
                    let mut delta = match control {
                        Control::None | Control::Tail => 0.0,
                        Control::Elevator => {
                            let sign = up_body.dot(Vec3::Y);
                            let ahead = if r.z < 0.0 { -1.0 } else { 1.0 };
                            -self.pitch_in * MAX_ELEVATOR * sign * ahead
                        }
                        Control::Aileron => {
                            let sign = up_body.dot(Vec3::Y);
                            let side = if r.x < 0.0 { -1.0 } else { 1.0 };
                            -self.roll_in * MAX_AILERON * sign * side
                        }
                        Control::Rudder => {
                            let sign = up_body.dot(Vec3::X);
                            let ahead = if r.z < 0.0 { -1.0 } else { 1.0 };
                            -self.yaw_in * MAX_RUDDER * sign * ahead
                        }
                    };
                    if a.aileron {
                        // Wings roll the plane themselves: the outer trailing edge deflects.
                        let sign = up_body.dot(Vec3::Y);
                        let side = if r.x < 0.0 { -1.0 } else { 1.0 };
                        delta += -self.roll_in * MAX_AILERON * AILERON_FRAC * sign * side;
                    }
                    let a_eff = alpha + delta;
                    let a_stall = a.cl_max / CL_SLOPE;
                    // Tail surfaces saturate rather than stall: the elevator keeps enough
                    // authority to push the nose down out of a stall (Adam: it must).
                    let cl = if a.control == Control::None { cl_curve(a_eff, a.cl_max) } else { cl_curve(a_eff.clamp(-a_stall, a_stall), a.cl_max) };
                    if a.control == Control::None {
                        aoa_sum += alpha;
                        aoa_n += 1;
                        // Warn once the wing is past its plateau, not at the first degree over.
                        if a_eff.abs() > a_stall + 0.05 && airspeed > 6.0 {
                            stalled = true;
                        }
                    }
                    let post_stall = (a_eff.abs() - a_stall).max(0.0);
                    let cd = a.cd0 + cl * cl / (PI * 0.8 * a.ar) + 1.1 * alpha.sin().powi(2) * (0.4 + post_stall);
                    let speed2 = va.length_squared();
                    let q = 0.5 * RHO * speed2;
                    let v2len = q2.sqrt();
                    let lift_dir = Vec3::new(0.0, -va.z / v2len, va.y / v2len);
                    let drag_dir = -va / speed2.sqrt();
                    let f_aero = lift_dir * (q * a.area * cl) + drag_dir * (q * a.area * cd);
                    let fb = a.frame * f_aero;
                    f_body += fb;
                    t_body += r.cross(fb);
                }
            } else {
                // Bluff-body drag along each body axis (parts are axis-aligned).
                let speed = v_point.length();
                if speed > 0.01 {
                    let fb = -0.5 * RHO * speed * Vec3::new(p.cda.x * v_point.x, p.cda.y * v_point.y, p.cda.z * v_point.z);
                    f_body += fb;
                    t_body += r.cross(fb);
                }
            }
            if let PartKind::Engine { thrust, burn: b, electric, prop, ramjet } = p.def.kind {
                // Ramjets only work with air rammed in: nothing at rest, full push past ~90 m/s.
                // Ramjets keep gaining thrust with speed up to about 780 km/h (Adam: jets
                // should reach F-22-like speeds; the wings and tails allow it now).
                let ram = if ramjet { (self.airspeed / 90.0).min(2.4).powi(2) } else { 1.0 };
                let ok = if electric { charge_ok } else { fuel_ok };
                if ok && self.throttle.abs() > 0.001 {
                    let t = self.throttle;
                    let speed_factor = if prop { (1.0 - 0.3 * airspeed / 100.0).clamp(0.4, 1.0) } else { 1.0 };
                    let mag = if t >= 0.0 { t * thrust * speed_factor * ram } else { t * thrust * 0.5 * ram };
                    let fb = p.thrust_dir * mag;
                    f_body += fb;
                    t_body += r.cross(fb);
                    thrust_total += mag;
                    if electric {
                        charge_use += b * t.abs();
                    } else {
                        burn += b * t.abs();
                    }
                }
            }
        }
        let _ = thrust_total;
        self.stalled = stalled;
        self.aoa = if aoa_n > 0 { aoa_sum / aoa_n as f32 } else { 0.0 };

        // Parachutes.
        if self.chute && self.has_chute {
            let speed = self.vel.length();
            if speed > 0.1 {
                let f = -self.vel / speed * (0.5 * RHO * speed * speed * 38.0);
                let fb = inv_rot * f;
                f_body += fb;
                t_body += (Vec3::new(0.0, 8.0, 0.0)).cross(fb);
                // Keep the plane hanging upright under the canopy.
                t_body += -w_body * self.inertia.x_axis.x.max(1.0) * 2.0;
            }
        }
        if self.drag_chute && self.has_drag_chute {
            let speed = self.vel.length();
            if speed > 0.5 {
                let f = -self.vel / speed * (0.5 * RHO * speed * speed * 9.0);
                f_body += inv_rot * f;
            }
        }

        // Artificial stability: rate damping and (optionally) wing leveling.
        let i_diag = Vec3::new(self.inertia.x_axis.x, self.inertia.y_axis.y, self.inertia.z_axis.z);
        let damp = if assist { 3.0 } else { 1.2 };
        t_body += -w_body * i_diag * damp;
        if assist && airspeed > 12.0 && self.roll_cmd.abs() < 0.05 {
            let right = rot * Vec3::X;
            let bank = (-right.y).asin();
            if bank.abs() < 1.2 {
                t_body.z += bank * i_diag.z * 4.0;
            }
        }

        // Convert to world and add gravity.
        let mut f_world = rot * f_body;
        let mut t_world = rot * t_body;
        f_world.y -= self.mass * G;

        // Wheels.
        let n_w = self.wheels.len().max(1) as f32;
        let k_spring = self.mass * G / (n_w * 0.10);
        let c_damp = 2.0 * (k_spring * self.mass / n_w).sqrt() * 0.8;
        let fwd_world = rot * Vec3::NEG_Z;
        let mut on_ground = false;
        let wheel_capacity = self.wheel_capacity;
        let total_mass = self.mass;
        let Sim { wheels, parts, events, .. } = self;
        for w in wheels.iter_mut() {
            let part = &parts[w.part];
            if part.retractable && !gear_down {
                w.in_contact = false;
                continue;
            }
            let rc = rot * (w.contact - com);
            let pw = self.pos + rc;
            let ground = terrain.height(pw.x, pw.z);
            if ground < -0.5 && pw.y < 0.2 {
                events.push(SimEvent::Crash("Ditched in the sea.".into()));
                continue;
            }
            let pen = ground - pw.y;
            if pen > 0.0 {
                let vc = self.vel + self.ang_vel.cross(rc);
                if !w.in_contact {
                    // Wheel-tip closing speed: what the gear feels (a nose wheel slamming down
                    // during de-rotation). The cargo's touchdown is judged on the plane below.
                    let impact = -vc.y;
                    // Gear collapse is judged on the plane's sink rate, not the wheel tip
                    // (which adds pitch rate times lever arm on a normal de-rotation), and an
                    // overloaded gear loses tolerance with the square root of the overload.
                    let overload = if wheel_capacity > 0.0 { (total_mass / wheel_capacity).max(1.0) } else { 1.0 };
                    let sink = (-self.vel.y).max(0.0);
                    let impact = sink.max(impact * 0.5);
                    if impact > w.max_impact / overload.sqrt() {
                        events.push(SimEvent::Crash(if overload > 1.0 {
                            "Overloaded landing gear collapsed on touchdown.".into()
                        } else {
                            "Landing gear collapsed on a hard touchdown.".into()
                        }));
                    }
                }
                w.in_contact = true;
                on_ground = true;
                let normal = (k_spring * pen.min(0.6) - c_damp * vc.y).max(0.0);
                let mut f = Vec3::new(0.0, normal, 0.0);
                let mut fwd = Vec3::new(fwd_world.x, 0.0, fwd_world.z).normalize_or_zero();
                if fwd.length_squared() < 0.1 {
                    fwd = Vec3::NEG_Z;
                }
                if w.steer_sign != 0.0 {
                    let steer = self.yaw_in * 0.45 * w.steer_sign;
                    fwd = Quat::from_rotation_y(-steer) * fwd;
                }
                let lat = Vec3::Y.cross(fwd).normalize_or_zero();
                let v_lat = vc.dot(lat);
                let v_fwd = vc.dot(fwd);
                let on_runway = terrain.airports.iter().any(|a| a.on_runway(pw));
                let rr = if on_runway { 0.02 } else { 0.07 };
                f += -lat * (1.1 * normal * (v_lat / 0.6).clamp(-1.0, 1.0));
                f += -fwd * (rr * normal * (v_fwd / 0.4).clamp(-1.0, 1.0));
                f_world += f;
                t_world += rc.cross(f);
            } else {
                w.in_contact = false;
            }
        }
        // One touchdown per landing, judged by how fast the plane itself was sinking
        // (smoothness): the wheel tips move faster than the fuselage when the nose drops.
        self.touchdown_cd = (self.touchdown_cd - dt).max(0.0);
        if on_ground && !self.on_ground && self.touchdown_cd <= 0.0 {
            let sink = (-self.vel.y).max(0.0);
            if sink > 0.3 {
                self.events.push(SimEvent::Touchdown(sink));
            }
            self.touchdown_cd = 4.0;
        }
        self.on_ground = on_ground;

        // Body contact: a belly, tail or wing tip touching the ground skids to a stop unless it
        // was slammed down or is still going fast.
        let mut scraping = false;
        let k_body = self.mass * G / 0.06;
        let c_body = 2.0 * (k_body * self.mass).sqrt() * 0.6;
        let m = self.mass;
        let Sim { parts, body_contact, events, .. } = self;
        if body_contact.len() != parts.len() {
            *body_contact = vec![false; parts.len()];
        }
        for (i, p) in parts.iter().enumerate() {
            if p.def.internal() || (p.def.is_wheel() && (gear_down || !p.retractable)) {
                continue;
            }
            let rm = Mat3::from_quat(p.rot);
            let h = p.dims * 0.5;
            let mut deepest: Option<(Vec3, f32)> = None;
            for sx in [-1.0, 1.0] {
                for sy in [-1.0, 1.0] {
                    for sz in [-1.0, 1.0] {
                        let local = rm * Vec3::new(sx * h.x, sy * h.y, sz * h.z) + p.pos;
                        let rc = rot * (local - com);
                        let world = self.pos + rc;
                        let g = terrain.height(world.x, world.z);
                        let pen = g - world.y;
                        if pen > 0.0 && deepest.map_or(true, |(_, d)| pen > d) {
                            deepest = Some((rc, pen));
                        }
                    }
                }
            }
            match deepest {
                Some((rc, pen)) => {
                    let vc = self.vel + self.ang_vel.cross(rc);
                    if !body_contact[i] {
                        body_contact[i] = true;
                        // Judged on the plane's sink rate: a tail or nacelle settling onto the
                        // runway during de-rotation is a scrape, not a slam.
                        let sink = (-self.vel.y).max(0.0).max(-vc.y * 0.5);
                        if sink > 5.0 {
                            events.push(SimEvent::Crash(format!("Slammed the {} into the ground at {:.1} m/s.", p.def.name, sink)));
                            continue;
                        }
                        if self.vel.length() > 45.0 {
                            events.push(SimEvent::Crash(format!("Hit the ground with the {} at speed.", p.def.name)));
                            continue;
                        }
                    }
                    if pen > 0.9 {
                        events.push(SimEvent::Crash(format!("Buried the {} in the ground.", p.def.name)));
                        continue;
                    }
                    scraping = true;
                    let normal = (k_body * pen - c_body * vc.y).max(0.0);
                    let horiz = Vec3::new(vc.x, 0.0, vc.z);
                    let mut f = Vec3::new(0.0, normal, 0.0);
                    if horiz.length() > 0.05 {
                        f += -horiz.normalize() * (0.7 * normal * (horiz.length() / 0.5).clamp(0.0, 1.0));
                    }
                    f_world += f;
                    t_world += rc.cross(f);
                    // Skids bleed rotation too.
                    t_world += -self.ang_vel * m * 0.5;
                }
                None => body_contact[i] = false,
            }
        }
        self.scraping = scraping;

        // Integrate.
        self.vel += f_world / self.mass * dt;
        self.pos += self.vel * dt;
        let r = Mat3::from_quat(rot);
        let i_world = r * self.inertia * r.transpose();
        let inv_i = i_world.inverse();
        let ang_acc = inv_i * (t_world - self.ang_vel.cross(i_world * self.ang_vel));
        self.ang_vel += ang_acc * dt;
        // Cap spin to keep things sane.
        let max_w = 6.0;
        if self.ang_vel.length() > max_w {
            self.ang_vel = self.ang_vel.normalize() * max_w;
        }
        let dq = Quat::from_scaled_axis(self.ang_vel * dt);
        self.rot = (dq * rot).normalize();
        self.airspeed = self.vel.length();

        // Fuel.
        if burn > 0.0 {
            // A loaded plane works its engines harder: burn rises with cargo weight
            // (Adam: cargo must cost thrust, speed and fuel, by total weight).
            let dry: f32 = self.parts.iter().map(|p| p.mass_dry).sum::<f32>().max(1.0);
            let burn = burn * cargo_burn_factor(self.cargo_mass(), dry);
            self.fuel = (self.fuel - burn * dt).max(0.0);
            if self.fuel <= 0.0 && !self.out_of_fuel_warned {
                self.out_of_fuel_warned = true;
                self.events.push(SimEvent::OutOfFuel);
            }
        }
        if charge_use > 0.0 {
            self.charge = (self.charge - charge_use * dt).max(0.0);
        }
        self.time += dt as f64;
    }

    /// Body/ground collision and out-of-bounds checks, once per frame.
    fn check_collisions(&mut self, terrain: &Terrain) {
        if self.crashed.is_some() || self.stuck {
            return;
        }
        let rot = self.rot;
        let com = self.com;
        let mut water = false;
        for p in &self.parts {
            if p.def.internal() {
                continue;
            }
            let m = Mat3::from_quat(p.rot);
            let h = p.dims * 0.5;
            for sx in [-1.0, 1.0] {
                for sy in [-1.0, 1.0] {
                    for sz in [-1.0, 1.0] {
                        let local = m * Vec3::new(sx * h.x, sy * h.y, sz * h.z) + p.pos;
                        let world = self.pos + rot * (local - com);
                        let g = terrain.height(world.x, world.z);
                        if g < -0.5 && world.y < -0.2 {
                            water = true;
                        }
                    }
                }
            }
            if water {
                break;
            }
        }
        if water {
            self.events.push(SimEvent::Crash("Ditched in the sea.".into()));
        }
        // Under a parachute, coming to rest on the ground is a "landing".
        if self.chute && self.has_chute && self.scraping && self.vel.length() < 1.0 {
            self.stuck = true;
            self.vel = Vec3::ZERO;
            self.ang_vel = Vec3::ZERO;
        }
        // The world is a square: measure the way out along whichever axis is further
        // (the circle we used before cut the corner islands off the map).
        let dist = self.pos.x.abs().max(self.pos.z.abs());
        self.oob = dist > world::WARN_RADIUS;
        if dist > world::KILL_RADIUS {
            self.events.push(SimEvent::Crash("Flew out of bounds and broke apart.".into()));
        }
        if self.pos.y > 6000.0 {
            self.events.push(SimEvent::Crash("Climbed too high: the wings came off in the thin air.".into()));
        }
        // Wings have a speed limit (wood tears off first): a real reason to research metal.
        let airspeed = self.vel.length();
        if let Some(p) = self.parts.iter().find(|p| p.aero.as_ref().map_or(false, |a| a.max_speed > 0.0 && airspeed > a.max_speed)) {
            self.events.push(SimEvent::Crash(format!("The {} tore off at {:.0} km/h.", p.def.name, airspeed * 3.6)));
        }
        if let Some(p) = self.parts.iter().find(|p| matches!(p.def.kind, PartKind::Cockpit { max_speed, .. } if max_speed > 0.0 && airspeed > max_speed)) {
            self.events.push(SimEvent::Crash(format!("{} at {:.0} km/h: the pilot could not hold on. Fit an enclosed cockpit.", p.def.name, airspeed * 3.6)));
        }
    }
}

// ---------------------------------------------------------------------------------------
// cargo logic (pure, so it can be unit tested)

/// Loads up to `n` units for `mission`, bounded by what is left to deliver and by free
/// capacity. Returns how many were loaded.
pub fn load_cargo(sim: &mut Sim, profile: &Profile, m: &'static MissionDef, n: u32) -> u32 {
    // Spares are allowed (take 5 when 3 are due, in case some break): only the hold limits it.
    let _ = profile;
    let free = sim.cargo_free(m.cargo.is_passenger());
    let n = n.min(free);
    // Trials run against the clock, not the mission timer.
    let expires = if profile.trial.is_some() { None } else { m.time_limit.map(|t| sim.time + t as f64) };
    if n > 0 && profile.trial.is_some() && sim.trial_start.is_none() {
        sim.trial_start = Some(sim.time);
        info!("trial clock started at sim time {:.1}", sim.time);
    }
    for _ in 0..n {
        sim.cargo.push(CargoItem { mission: m, expires });
    }
    if n > 0 {
        sim.update_mass();
    }
    n
}

/// Buys as much fuel as the airport sells and the player can afford. Returns a message.
pub fn refuel(sim: &mut Sim, profile: &mut Profile, ap: &Airport) -> String {
    let need = (sim.fuel_cap - sim.fuel).max(0.0);
    if need <= 0.5 {
        return "Tanks are already full.".into();
    }
    let price = ap.def.fuel_price;
    let cost = (need * price).ceil() as i64;
    sim.out_of_fuel_warned = false;
    if profile.can_afford(cost) {
        profile.spend(cost);
        sim.fuel = sim.fuel_cap;
        sim.update_mass();
        if cost > 0 { format!("Refuelled {need:.0} L for ${cost}.") } else { format!("Refuelled {need:.0} L.") }
    } else if profile.money > 0 && price > 0.0 {
        let liters = (profile.money as f32 / price).floor().min(need);
        let c = (liters * price).ceil() as i64;
        profile.spend(c);
        sim.fuel += liters;
        sim.update_mass();
        format!("Could only afford {liters:.0} L (${c}).")
    } else {
        "Not enough money for fuel.".into()
    }
}

/// Units of `mission` that could still be loaded right now.
/// (still needed, how many of those fit, how many fit at all incl. spares).
pub fn loadable(sim: &Sim, profile: &Profile, m: &MissionDef) -> (u32, u32, u32) {
    let p = profile.progress(m.id);
    let aboard = sim.cargo.iter().filter(|c| c.mission.id == m.id).count() as u32;
    let remaining = m.count.saturating_sub(p.delivered + aboard);
    let free = sim.cargo_free(m.cargo.is_passenger());
    (remaining, remaining.min(free), free)
}

/// Delivers everything aboard that is destined for `airport`, paying out per unit and
/// completing missions. Returns messages for the player; `true` if the profile changed.
pub fn deliver_cargo(sim: &mut Sim, profile: &mut Profile, airport: &str) -> (Vec<String>, bool) {
    let mut delivered: Vec<&'static MissionDef> = Vec::new();
    sim.cargo.retain(|c| {
        if c.mission.to == airport {
            delivered.push(c.mission);
            false
        } else {
            true
        }
    });
    let mut msgs = Vec::new();
    if delivered.is_empty() {
        return (msgs, false);
    }
    for m in delivered {
        let per_unit = (m.reward as f32 / m.count as f32).round() as i64;
        let p = profile.progress_mut(m.id);
        if p.delivered >= m.count {
            // A spare: unloaded, not paid for.
            msgs.push(format!("Unloaded a spare {} of {}.", m.cargo.unit(), m.cargo.name()));
            continue;
        }
        p.delivered += 1;
        let done = p.delivered >= m.count && !p.completed;
        if done {
            p.completed = true;
        }
        profile.refund(per_unit);
        msgs.push(format!("Delivered 1 {} of {} for \"{}\": +${}", m.cargo.unit(), m.cargo.name(), m.title, per_unit));
        if done && profile.trial.as_deref() == Some(m.id) {
            let secs = sim.time - sim.trial_start.unwrap_or(sim.time);
            sim.trial_done = Some(secs);
            profile::record_trial(m.id, secs, &profile.design);
            msgs.push(format!("Trial time {}: {} in a ${} plane. Recorded.", fmt_time_tenths(secs as f32), m.title, profile.design.total_cost()));
        }
        if done {
            profile.rp += m.rp;
            msgs.push(format!("Mission complete: {} (+{} research points)", m.title, m.rp));
            if m.contract {
                // Contracts repeat: clear the slot so the board offers the run again.
                let p = profile.progress_mut(m.id);
                p.accepted = false;
                p.delivered = 0;
                p.completed = false;
                profile.contracts_done += 1;
            }
        }
    }
    sim.update_mass();
    (msgs, true)
}

// ---------------------------------------------------------------------------------------
// entities & UI state

#[derive(Component)]
struct PlaneRoot;

#[derive(Component)]
struct FlightEntity;

#[derive(Component)]
struct Debris {
    vel: Vec3,
    ang_vel: Vec3,
    settled: bool,
}

#[derive(Component)]
struct Puff {
    life: f32,
    grow: f32,
}

#[derive(Resource)]
pub struct FlightUi {
    pub paused: bool,
    pub show_map: bool,
    pub show_missions: bool,
    pub show_help: bool,
    pub show_cargo: bool,
    pub show_research: bool,
    go_edit: bool,
    cam_yaw: f32,
    cam_pitch: f32,
    cam_dist: f32,
    cam_pos: Vec3,
    cam_init: bool,
    /// Free camera (F): fly around the world to look at it; the sim pauses meanwhile.
    pub free: bool,
    pub free_pos: Vec3,
    pub free_yaw: f32,
    pub free_pitch: f32,
    restart: bool,
    go_hangar: bool,
    go_menu: bool,
    crash_shown: bool,
}

impl Default for FlightUi {
    fn default() -> Self {
        FlightUi {
            paused: false,
            show_map: false,
            show_missions: false,
            show_help: false,
            show_cargo: false,
            show_research: false,
            go_edit: false,
            cam_yaw: 0.0,
            cam_pitch: 0.0,
            cam_dist: 18.0,
            cam_pos: Vec3::ZERO,
            cam_init: false,
            free: false,
            free_pos: Vec3::ZERO,
            free_yaw: 0.0,
            free_pitch: 0.0,
            restart: false,
            go_hangar: false,
            go_menu: false,
            crash_shown: false,
        }
    }
}

fn spawn_plane(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cache: &mut MaterialCache,
    design: &PlaneDesign,
) -> Entity {
    let root = commands.spawn((Transform::IDENTITY, Visibility::default(), PlaneRoot, FlightEntity)).id();
    let layers = RenderLayers::default();
    for (i, p) in design.parts.iter().enumerate() {
        let e = partvis::spawn_part_visual(commands, meshes, materials, cache, p, i, &layers);
        commands.entity(e).insert(FlightEntity);
        commands.entity(root).add_child(e);
    }
    root
}

/// Builds the sim for the current design at the profile's spawn airport, carrying over any
/// cargo and fuel from a previous flight that ended with "Edit Plane".
fn start_flight(sim: &mut Sim, profile: &Profile, terrain: &Terrain) {
    let carried = std::mem::take(&mut sim.cargo);
    let prev_fuel = sim.prev_fuel.take();
    sim.build(&profile.design);
    for c in carried {
        let free = sim.cargo_free(c.mission.cargo.is_passenger());
        if free > 0 {
            sim.cargo.push(c);
        }
    }
    let ap = terrain.airports.iter().find(|a| a.id() == profile.spawn_airport).unwrap_or(terrain.home());
    if let Some(f) = prev_fuel {
        if !ap.is_home() {
            sim.fuel = f.min(sim.fuel_cap);
        }
    }
    sim.update_mass();
    sim.place(terrain, ap.spawn_point(), ap.yaw());
}

fn enter_flight(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<MaterialCache>,
    mut sim: ResMut<Sim>,
    mut ui: ResMut<FlightUi>,
    mut profile: ResMut<Profile>,
    terrain: Res<Terrain>,
    mut toasts: ResMut<Toasts>,
) {
    *ui = FlightUi::default();
    start_flight(&mut sim, &profile, &terrain);
    ui.cam_dist = (sim.max_extent * 2.3).clamp(10.0, 60.0);
    spawn_plane(&mut commands, &mut meshes, &mut materials, &mut cache, &profile.design);
    profile.flights += 1;
    if !sim.cargo.is_empty() {
        toasts.push(format!("{} cargo units still aboard.", sim.cargo.len()));
    }
    toasts.push("Space/Shift throttle up, Ctrl down. W/S pitch, A/D roll, Q/E rudder. H for help.");
}

fn reset_sim(mut sim: ResMut<Sim>) {
    sim.cargo.clear();
    sim.prev_fuel = None;
    sim.active = false;
}

fn exit_flight(
    mut commands: Commands,
    q: Query<Entity, (With<FlightEntity>, Without<ChildOf>)>,
    mut sim: ResMut<Sim>,
    profile: Res<Profile>,
) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
    sim.active = false;
    sim.prev_fuel = Some(sim.fuel);
    profile::save_profile(&profile);
}

// ---------------------------------------------------------------------------------------
// input

fn flight_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    focus: Res<UiFocus>,
    mut sim: ResMut<Sim>,
    mut ui: ResMut<FlightUi>,
    mut toasts: ResMut<Toasts>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        if ui.show_map || ui.show_missions || ui.show_help || ui.show_cargo || ui.show_research {
            ui.show_map = false;
            ui.show_missions = false;
            ui.show_help = false;
            ui.show_cargo = false;
            ui.show_research = false;
        } else if sim.crashed.is_none() {
            ui.paused = !ui.paused;
        }
    }
    if focus.keyboard {
        return;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        ui.show_map = !ui.show_map;
    }
    if keys.just_pressed(KeyCode::Tab) {
        ui.show_missions = !ui.show_missions;
    }
    if keys.just_pressed(KeyCode::KeyH) {
        ui.show_help = !ui.show_help;
    }
    if keys.just_pressed(KeyCode::KeyF) && sim.crashed.is_none() {
        ui.free = !ui.free;
        if ui.free {
            ui.free_pos = ui.cam_pos;
            let d = (sim.pos - ui.cam_pos).normalize_or_zero();
            ui.free_yaw = (-d.x).atan2(-d.z);
            ui.free_pitch = d.y.asin();
            toasts.push("Free camera: W/A/S/D move, Q/E down/up, right-drag to look, Shift fast. F returns to the plane.".to_string());
        }
    }
    if ui.free {
        // The plane sits still while you look around.
        return;
    }
    if ui.paused || ui.free || sim.crashed.is_some() || !sim.active {
        return;
    }
    let dt = time.delta_secs();
    let axis = |neg: &[KeyCode], pos: &[KeyCode]| -> f32 {
        let n = neg.iter().any(|k| keys.pressed(*k));
        let p = pos.iter().any(|k| keys.pressed(*k));
        (p as i32 - n as i32) as f32
    };
    // S / down = nose up (pull back).
    sim.pitch_cmd = axis(&[KeyCode::KeyW, KeyCode::ArrowUp], &[KeyCode::KeyS, KeyCode::ArrowDown]);
    sim.roll_cmd = axis(&[KeyCode::KeyA, KeyCode::ArrowLeft], &[KeyCode::KeyD, KeyCode::ArrowRight]);
    sim.yaw_cmd = axis(&[KeyCode::KeyQ], &[KeyCode::KeyE]);
    let th = axis(&[KeyCode::ControlLeft, KeyCode::ControlRight], &[KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::Space]);
    if th != 0.0 {
        sim.throttle = (sim.throttle + th * dt * 0.8).clamp(-1.0, 1.0);
    }
    if keys.just_pressed(KeyCode::KeyX) {
        sim.throttle = 0.0;
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        sim.throttle = 1.0;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        if sim.has_retract {
            sim.gear_down = !sim.gear_down;
            toasts.push(if sim.gear_down { "Gear down." } else { "Gear up." });
        } else {
            toasts.push("This plane has fixed landing gear.");
        }
    }
    if keys.just_pressed(KeyCode::KeyP) {
        if sim.has_chute && !sim.chute {
            sim.chute = true;
            toasts.push("Parachute deployed!");
        } else if !sim.has_chute {
            toasts.push("No parachute fitted.");
        }
    }
    if keys.just_pressed(KeyCode::KeyB) {
        if sim.has_drag_chute {
            sim.drag_chute = !sim.drag_chute;
            toasts.push(if sim.drag_chute { "Drag chute out." } else { "Drag chute cut." });
        } else {
            toasts.push("No drag chute fitted. Use reverse thrust (Ctrl) to brake.");
        }
    }
}

// ---------------------------------------------------------------------------------------
// physics & events

/// Fuel burn multiplier for `cargo_kg` aboard a plane of `dry_kg`: +60% burn at a load equal
/// to the empty weight, on top of the extra throttle a heavy plane already needs.
pub fn cargo_burn_factor(cargo_kg: f32, dry_kg: f32) -> f32 {
    1.0 + 0.6 * (cargo_kg / dry_kg.max(1.0)).clamp(0.0, 3.0)
}

/// Trees are solid: flying a wing tip, nose or tail into a crown or trunk ends the flight.
fn check_trees(sim: &mut Sim, trees: &world::TreeColliders) {
    if !sim.active || sim.crashed.is_some() || sim.parked.is_some() {
        return;
    }
    let right = sim.rot * Vec3::X;
    let fwd = sim.rot * Vec3::NEG_Z;
    let e = sim.max_extent.max(1.0);
    let pts = [sim.pos, sim.pos + right * e * 0.85, sim.pos - right * e * 0.85, sim.pos + fwd * e * 0.6, sim.pos - fwd * e * 0.6];
    if let Some(what) = pts.iter().find_map(|p| trees.hit(*p)) {
        let verb = if sim.on_ground { "Ran into" } else { "Flew into" };
        sim.events.push(SimEvent::Crash(format!("{verb} {what}.")));
    }
}

fn flight_physics(
    time: Res<Time>,
    terrain: Res<Terrain>,
    trees: Option<Res<world::TreeColliders>>,
    profile: Res<Profile>,
    ui: Res<FlightUi>,
    mut sim: ResMut<Sim>,
) {
    if ui.paused || ui.free || !sim.active {
        return;
    }
    if sim.crashed.is_some() {
        sim.crash_timer += time.delta_secs();
        return;
    }
    let started = std::time::Instant::now();
    let dt = time.delta_secs().min(0.1);
    let n = ((dt / SUBSTEP).ceil() as usize).max(1);
    let h = dt / n as f32;
    sim.update_mass();
    let last = sim.pos;
    for _ in 0..n {
        sim.step(h, &terrain, profile.assist);
        if let Some(t) = trees.as_deref() {
            check_trees(&mut sim, t);
        }
    }
    sim.check_collisions(&terrain);
    sim.physics_ms = started.elapsed().as_secs_f32() * 1000.0;
    let moved = (sim.pos - last).length();
    sim.distance += moved;
    // Parking detection: on the ground anywhere in the airport's zone at taxi speed. No
    // throttle condition (a reverse-thrust rollout rarely ends at exactly zero).
    let pos = sim.pos;
    if sim.on_ground && sim.speed() < 5.0 {
        sim.parked = terrain.airport_at(pos).map(|a| a.id());
    } else {
        sim.parked = None;
    }
}

fn flight_events(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<MaterialCache>,
    terrain: Res<Terrain>,
    mut sim: ResMut<Sim>,
    mut ui: ResMut<FlightUi>,
    mut profile: ResMut<Profile>,
    mut toasts: ResMut<Toasts>,
    mut radio: ResMut<crate::Radio>,
    mut next: ResMut<NextState<GameState>>,
    roots: Query<Entity, With<PlaneRoot>>,
    parts: Query<(Entity, &GlobalTransform), With<PartIndex>>,
    debris: Query<Entity, Or<(With<Debris>, With<Puff>)>>,
) {
    if !sim.active {
        return;
    }
    // Reveal the map as the plane moves; announce airports whose tile just appeared.
    if sim.crashed.is_none() {
        let (x, z) = (sim.pos.x, sim.pos.z);
        let before: Vec<bool> = terrain.airports.iter().map(|a| profile.is_explored(a.def.pos.x, a.def.pos.y)).collect();
        if profile.explore(x, z) {
            for (a, was) in terrain.airports.iter().zip(before) {
                if !was && !a.is_home() && profile.is_explored(a.def.pos.x, a.def.pos.y) {
                    toasts.push(format!("Discovered {}.", a.name()));
                }
            }
        }
    }

    // Cargo timers.
    let now = sim.time;
    let mut expired = Vec::new();
    sim.cargo.retain(|c| {
        if let Some(e) = c.expires {
            if now > e {
                expired.push(c.mission.cargo.name());
                return false;
            }
        }
        true
    });
    for name in expired {
        toasts.push(format!("{name} spoiled before delivery."));
    }

    let events = std::mem::take(&mut sim.events);
    for ev in events {
        match ev {
            SimEvent::Touchdown(impact) => {
                sim.touchdown_impact = impact;
                // Graded breakage: nothing under the soft limit, a growing share up to the
                // hard limit (where the gear gives way too), everything above it.
                let fragile_aboard = sim.cargo.iter().filter(|c| c.mission.fragile).count();
                // Always say how hard it was, so the feel of a landing can be calibrated.
                if fragile_aboard == 0 || impact <= FRAGILE_MAX_IMPACT {
                    let word = if impact < 1.5 { "Greaser" } else if impact < 3.5 { "Touchdown" } else if impact < 5.0 { "Firm touchdown" } else { "Hard touchdown" };
                    toasts.push(format!("{word}: {impact:.1} m/s down."));
                }
                if fragile_aboard > 0 && impact > FRAGILE_MAX_IMPACT {
                    let frac = ((impact - FRAGILE_MAX_IMPACT) / (FRAGILE_ALL_BREAK - FRAGILE_MAX_IMPACT)).clamp(0.0, 1.0);
                    let broken = ((fragile_aboard as f32 * frac).ceil() as usize).clamp(1, fragile_aboard);
                    let mut left = broken;
                    let mut name = "";
                    sim.cargo.retain(|c| {
                        if c.mission.fragile && left > 0 {
                            left -= 1;
                            name = c.mission.cargo.name();
                            false
                        } else {
                            true
                        }
                    });
                    if broken == fragile_aboard {
                        toasts.push(format!("Hard landing ({impact:.1} m/s): all {broken} of the {name} shattered."));
                    } else {
                        toasts.push(format!("Bumpy landing ({impact:.1} m/s): {broken} of {fragile_aboard} {name} broke."));
                    }
                }
            }
            SimEvent::OutOfFuel => toasts.push("Out of fuel! Glide to a runway."),
            SimEvent::Crash(reason) => {
                if sim.crashed.is_none() {
                    sim.crashed = Some(reason.clone());
                    sim.crash_timer = 0.0;
                    sim.cargo.clear();
                    profile.crashes += 1;
                    toasts.push(format!("Crashed: {reason}"));
                    // Break the plane apart.
                    let mut rng = Rng(0x1234_5678_9ABC_DEF1 ^ (now.to_bits()));
                    let base_vel = sim.vel * 0.4;
                    for (e, gt) in parts.iter() {
                        let tf = gt.compute_transform();
                        let v = base_vel + Vec3::new(rng.range(-6.0, 6.0), rng.range(2.0, 9.0), rng.range(-6.0, 6.0));
                        let w = Vec3::new(rng.range(-4.0, 4.0), rng.range(-4.0, 4.0), rng.range(-4.0, 4.0));
                        commands.entity(e).remove::<ChildOf>().insert((tf, Debris { vel: v, ang_vel: w, settled: false }));
                    }
                    for root in roots.iter() {
                        commands.entity(root).despawn();
                    }
                    let fire = materials.add(StandardMaterial {
                        base_color: Color::srgba(1.0, 0.45, 0.1, 0.85),
                        emissive: LinearRgba::new(4.0, 1.2, 0.2, 1.0),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        ..default()
                    });
                    let smoke = materials.add(StandardMaterial {
                        base_color: Color::srgba(0.25, 0.25, 0.27, 0.7),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        ..default()
                    });
                    let sphere = meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap());
                    for k in 0..10 {
                        let off = Vec3::new(rng.range(-2.0, 2.0), rng.range(0.0, 2.5), rng.range(-2.0, 2.0));
                        commands.spawn((
                            Mesh3d(sphere.clone()),
                            MeshMaterial3d(if k < 5 { fire.clone() } else { smoke.clone() }),
                            Transform::from_translation(sim.pos + off).with_scale(Vec3::splat(0.5)),
                            Puff { life: rng.range(0.8, 1.8), grow: rng.range(3.0, 7.0) },
                            FlightEntity,
                        ));
                    }
                }
            }
        }
    }

    // Deliveries when parked.
    if let Some(ap) = sim.parked {
        let (msgs, changed) = deliver_cargo(&mut sim, &mut profile, ap);
        for m in &msgs {
            if let Some(title) = m.strip_prefix("Mission complete: ") {
                radio.say(format!("{} says thanks! {} Research points added to your account.", terrain.airport(ap).name(), title));
            }
            toasts.push(m.clone());
        }
        if changed {
            profile::save_profile(&profile);
        }
    }

    // UI-driven transitions.
    if ui.go_hangar {
        ui.go_hangar = false;
        sim.cargo.clear();
        profile.spawn_airport = "harbor".into();
        next.set(GameState::Hangar);
    }
    if ui.go_edit {
        ui.go_edit = false;
        if let Some(ap) = sim.parked {
            profile.spawn_airport = ap.to_string();
        }
        next.set(GameState::Hangar);
    }
    if ui.go_menu {
        ui.go_menu = false;
        sim.cargo.clear();
        profile::save_profile(&profile);
        next.set(GameState::MainMenu);
    }
    if ui.restart {
        ui.restart = false;
        for e in debris.iter() {
            commands.entity(e).despawn();
        }
        for root in roots.iter() {
            commands.entity(root).despawn();
        }
        sim.cargo.clear();
        sim.prev_fuel = None;
        start_flight(&mut sim, &profile, &terrain);
        spawn_plane(&mut commands, &mut meshes, &mut materials, &mut cache, &profile.design);
        ui.paused = false;
        ui.crash_shown = false;
        profile.flights += 1;
    }
}

// ---------------------------------------------------------------------------------------
// visuals

fn flight_visuals(
    time: Res<Time>,
    sim: Res<Sim>,
    ui: Res<FlightUi>,
    mut roots: Query<&mut Transform, (With<PlaneRoot>, Without<Propeller>, Without<GearVisual>)>,
    mut props: Query<&mut Transform, (With<Propeller>, Without<PlaneRoot>, Without<GearVisual>)>,
    mut gear: Query<(&mut Transform, &mut GearVisual), (Without<PlaneRoot>, Without<Propeller>)>,
    mut chutes: Query<&mut Visibility, (With<ChuteCanopy>, Without<DragChuteCanopy>)>,
    mut drag_chutes: Query<&mut Visibility, (With<DragChuteCanopy>, Without<ChuteCanopy>)>,
) {
    if !sim.active {
        return;
    }
    for mut t in roots.iter_mut() {
        *t = sim.root_transform();
    }
    if ui.paused {
        return;
    }
    let dt = time.delta_secs();
    let powered = (sim.has_fuel_engines && sim.fuel > 0.0) || (sim.has_electric && sim.charge > 0.0);
    let rate = if powered && sim.crashed.is_none() { 6.0 + sim.throttle.abs() * 50.0 } else { 0.0 };
    for mut t in props.iter_mut() {
        t.rotation = t.rotation * Quat::from_rotation_z(rate * dt);
    }
    // Retractable gear folds backwards and up about the strut top, far enough that the
    // wheel ends up inside the body rather than flush under it.
    let target = if sim.gear_down { 0.0 } else { 2.4 };
    for (mut t, mut g) in gear.iter_mut() {
        g.angle += (target - g.angle) * (3.5 * dt).min(1.0);
        t.rotation = Quat::from_rotation_x(g.angle);
    }
    for mut v in chutes.iter_mut() {
        *v = if sim.chute { Visibility::Inherited } else { Visibility::Hidden };
    }
    for mut v in drag_chutes.iter_mut() {
        *v = if sim.drag_chute { Visibility::Inherited } else { Visibility::Hidden };
    }
}

fn debris_update(
    mut commands: Commands,
    time: Res<Time>,
    terrain: Res<Terrain>,
    ui: Res<FlightUi>,
    mut debris: Query<(&mut Transform, &mut Debris), Without<Puff>>,
    mut puffs: Query<(Entity, &mut Transform, &mut Puff), Without<Debris>>,
) {
    if ui.paused {
        return;
    }
    let dt = time.delta_secs().min(0.05);
    for (mut t, mut d) in debris.iter_mut() {
        if d.settled {
            continue;
        }
        d.vel.y -= G * dt;
        d.vel *= 1.0 - 0.3 * dt;
        t.translation += d.vel * dt;
        t.rotation = (Quat::from_scaled_axis(d.ang_vel * dt) * t.rotation).normalize();
        let g = terrain.height(t.translation.x, t.translation.z).max(0.0);
        if t.translation.y < g + 0.3 {
            t.translation.y = g + 0.3;
            if d.vel.y < -3.0 {
                d.vel.y = -d.vel.y * 0.3;
                d.vel.x *= 0.5;
                d.vel.z *= 0.5;
                d.ang_vel *= 0.5;
            } else {
                d.settled = true;
            }
        }
    }
    for (e, mut t, mut p) in puffs.iter_mut() {
        p.life -= dt;
        t.scale += Vec3::splat(p.grow * dt);
        t.translation.y += 2.0 * dt;
        if p.life <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

fn flight_camera(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    focus: Res<UiFocus>,
    terrain: Res<Terrain>,
    sim: Res<Sim>,
    mut ui: ResMut<FlightUi>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
) {
    if !sim.active {
        return;
    }
    let dt = time.delta_secs();
    if ui.free {
        if !focus.pointer && mouse.pressed(MouseButton::Right) {
            ui.free_yaw -= motion.delta.x * 0.0035;
            ui.free_pitch = (ui.free_pitch - motion.delta.y * 0.003).clamp(-1.5, 1.5);
        }
        let rot = Quat::from_rotation_y(ui.free_yaw) * Quat::from_rotation_x(ui.free_pitch);
        if !focus.keyboard {
            let fwd = rot * Vec3::NEG_Z;
            let right = rot * Vec3::X;
            let mut mv = Vec3::ZERO;
            if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) { mv += fwd; }
            if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) { mv -= fwd; }
            if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) { mv += right; }
            if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) { mv -= right; }
            if keys.pressed(KeyCode::KeyE) { mv += Vec3::Y; }
            if keys.pressed(KeyCode::KeyQ) { mv -= Vec3::Y; }
            let speed = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) { 160.0 } else { 30.0 };
            ui.free_pos += mv.normalize_or_zero() * speed * dt;
        }
        let ground = terrain.height(ui.free_pos.x, ui.free_pos.z).max(0.0);
        if ui.free_pos.y < ground + 1.5 {
            ui.free_pos.y = ground + 1.5;
        }
        if let Ok(mut t) = cam.single_mut() {
            *t = Transform::from_translation(ui.free_pos).with_rotation(rot);
        }
        return;
    }
    if !focus.pointer {
        if mouse.pressed(MouseButton::Right) {
            ui.cam_yaw -= motion.delta.x * 0.005;
            ui.cam_pitch = (ui.cam_pitch + motion.delta.y * 0.004).clamp(-0.8, 1.2);
        }
        let s = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y * 1.0,
            MouseScrollUnit::Pixel => scroll.delta.y * 0.02,
        };
        if s != 0.0 {
            ui.cam_dist = (ui.cam_dist * (1.0 - s * 0.1)).clamp(6.0, 120.0);
        }
    }
    if !mouse.pressed(MouseButton::Right) {
        // Ease back behind the plane.
        let k = (2.5 * dt).min(1.0);
        ui.cam_yaw += (0.0 - ui.cam_yaw) * k;
        ui.cam_pitch += (0.0 - ui.cam_pitch) * k;
    }
    let focus_point = if sim.crashed.is_some() { sim.pos } else { sim.pos };
    let fwd = sim.rot * Vec3::NEG_Z;
    let mut fwd_h = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
    if fwd_h.length_squared() < 0.1 {
        fwd_h = Vec3::NEG_Z;
    }
    let yaw = Quat::from_rotation_y(ui.cam_yaw);
    let dir = yaw * fwd_h;
    let elev = 0.2 + ui.cam_pitch;
    let desired = focus_point - dir * (ui.cam_dist * elev.cos()) + Vec3::Y * (ui.cam_dist * elev.sin());
    if !ui.cam_init {
        ui.cam_pos = desired;
        ui.cam_init = true;
    }
    let k = (7.0 * dt).min(1.0);
    let cp = ui.cam_pos;
    ui.cam_pos = cp + (desired - cp) * k;
    let ground = terrain.height(ui.cam_pos.x, ui.cam_pos.z).max(0.0);
    if ui.cam_pos.y < ground + 1.6 {
        ui.cam_pos.y = ground + 1.6;
    }
    if let Ok(mut t) = cam.single_mut() {
        *t = Transform::from_translation(ui.cam_pos).looking_at(focus_point + Vec3::Y * 0.8, Vec3::Y);
    }
}

// ---------------------------------------------------------------------------------------
// UI

fn fmt_time(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn fmt_time_tenths(s: f32) -> String {
    let s = s.max(0.0);
    format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0)
}

/// Capability badges for a mission: (text, highlighted). Speed is what it takes to make the
/// timer with a minute for take-off and landing; long hauls flag fuel.
fn mission_demands(m: &MissionDef, dist_km: f32) -> Vec<(String, bool)> {
    let mut v = Vec::new();
    let mass = m.count as f32 * m.cargo.mass();
    v.push((format!("{} x {:.0} kg = {:.0} kg", m.count, m.cargo.mass(), mass), mass >= 500.0));
    if m.cargo.is_passenger() {
        v.push((format!("{} seats", m.count), true));
    }
    v.push((format!("{dist_km:.1} km"), dist_km >= 8.0));
    if dist_km >= 8.0 {
        v.push(("long haul: fuel".into(), true));
    }
    if let Some(t) = m.time_limit {
        let need = dist_km * 1000.0 / (t - 60.0).max(30.0) * 3.6;
        v.push((format!("timer {}: needs ~{:.0} km/h", fmt_time(t), need), need >= 150.0));
    }
    if m.fragile {
        v.push(("fragile: gentle landing".into(), true));
    }
    v
}

fn mission_status(profile: &Profile, m: &MissionDef) -> String {
    let p = profile.progress(m.id);
    if p.completed {
        "Completed".into()
    } else if p.accepted {
        format!("Active: {}/{} delivered", p.delivered, m.count)
    } else if profile.mission_available(m.id) {
        "Available".into()
    } else {
        if m.contract {
            return "Locked: discover both airports".into();
        }
        let req: Vec<&str> = m.requires.iter().filter(|r| !profile.progress(r).completed).filter_map(|r| missions::mission(r).map(|m| m.title)).collect();
        format!("Locked: complete {}", req.join(", "))
    }
}

/// Small plane silhouette used for map and waypoint markers.
pub fn plane_glyph(painter: &egui::Painter, c: egui::Pos2, s: f32, color: egui::Color32) {
    let pts = [
        (0.0, -9.0), (2.5, -3.0), (10.0, 1.0), (10.0, 3.5), (2.5, 1.5), (2.0, 6.0), (5.0, 8.0), (5.0, 9.5),
        (0.0, 8.0), (-5.0, 9.5), (-5.0, 8.0), (-2.0, 6.0), (-2.5, 1.5), (-10.0, 3.5), (-10.0, 1.0), (-2.5, -3.0),
    ];
    let poly: Vec<egui::Pos2> = pts.iter().map(|(x, y)| egui::pos2(c.x + x * s, c.y + y * s)).collect();
    painter.add(egui::Shape::convex_polygon(poly, color, egui::Stroke::NONE));
}

/// White pictogram for an airport's kind, drawn inside its map circle of radius `r`.
pub fn kind_glyph(painter: &egui::Painter, c: egui::Pos2, r: f32, kind: world::AirportKind) {
    use world::AirportKind as K;
    let s = r / 11.0;
    let w = egui::Color32::WHITE;
    let stroke = egui::Stroke::new((2.0 * s).max(1.2), w);
    let p = |x: f32, y: f32| egui::pos2(c.x + x * s, c.y + y * s);
    let poly = |pts: &[(f32, f32)]| egui::Shape::convex_polygon(pts.iter().map(|(x, y)| p(*x, *y)).collect(), w, egui::Stroke::NONE);
    match kind {
        K::Home => plane_glyph(painter, c, 0.7 * s, w),
        K::Town => {
            painter.add(poly(&[(-7.0, -1.0), (0.0, -8.0), (7.0, -1.0)]));
            painter.rect_filled(egui::Rect::from_min_max(p(-5.0, -1.0), p(5.0, 7.0)), 0.0, w);
        }
        K::Farm => {
            // A stalk of wheat.
            painter.line_segment([p(0.0, 8.0), p(0.0, -8.0)], stroke);
            for k in 0..3 {
                let y = -6.0 + k as f32 * 4.0;
                painter.line_segment([p(0.0, y + 2.0), p(-5.0, y - 1.0)], stroke);
                painter.line_segment([p(0.0, y + 2.0), p(5.0, y - 1.0)], stroke);
            }
        }
        K::Hospital => {
            painter.rect_filled(egui::Rect::from_min_max(p(-2.5, -8.0), p(2.5, 8.0)), 0.0, w);
            painter.rect_filled(egui::Rect::from_min_max(p(-8.0, -2.5), p(8.0, 2.5)), 0.0, w);
        }
        K::Lighthouse => {
            painter.add(poly(&[(-3.0, -6.0), (3.0, -6.0), (5.0, 8.0), (-5.0, 8.0)]));
            painter.line_segment([p(-5.0, -8.0), p(-9.0, -10.0)], stroke);
            painter.line_segment([p(5.0, -8.0), p(9.0, -10.0)], stroke);
            painter.circle_filled(p(0.0, -8.0), 2.2 * s, w);
        }
        K::Base => {
            let pts: Vec<egui::Pos2> = (0..10)
                .map(|k| {
                    let a = k as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
                    let rr = if k % 2 == 0 { 9.0 } else { 4.0 };
                    p(a.cos() * rr, a.sin() * rr)
                })
                .collect();
            painter.add(egui::Shape::closed_line(pts, egui::Stroke::new((2.2 * s).max(1.2), w)));
        }
        K::Outpost => {
            painter.add(poly(&[(0.0, -8.0), (9.0, 8.0), (-9.0, 8.0)]));
            painter.add(egui::Shape::convex_polygon(vec![p(0.0, 0.0), p(3.0, 8.0), p(-3.0, 8.0)], egui::Color32::from_rgb(30, 32, 40), egui::Stroke::NONE));
        }
        K::Oasis => {
            painter.line_segment([p(0.0, 9.0), p(1.0, -2.0)], stroke);
            for (dx, dy) in [(-8.0, -6.0), (-6.0, -10.0), (2.0, -11.0), (8.0, -8.0), (9.0, -3.0)] {
                painter.line_segment([p(1.0, -2.0), p(dx, dy)], stroke);
            }
        }
        K::Hermit => {
            painter.add(poly(&[(-9.0, 0.0), (0.0, -8.0), (9.0, 0.0)]));
            painter.rect_filled(egui::Rect::from_min_max(p(-6.0, 0.0), p(6.0, 8.0)), 0.0, w);
            painter.rect_filled(egui::Rect::from_min_max(p(-1.5, 2.0), p(1.5, 8.0)), 0.0, egui::Color32::from_rgb(30, 32, 40));
        }
        K::Prison => {
            painter.rect_stroke(egui::Rect::from_min_max(p(-8.0, -7.0), p(8.0, 7.0)), 1.0, stroke, egui::StrokeKind::Middle);
            for x in [-4.0, 0.0, 4.0] {
                painter.line_segment([p(x, -7.0), p(x, 7.0)], stroke);
            }
        }
        K::Wreck => {
            painter.circle_stroke(p(0.0, -6.0), 2.5 * s, stroke);
            painter.line_segment([p(0.0, -3.5), p(0.0, 9.0)], stroke);
            painter.line_segment([p(-5.0, -1.0), p(5.0, -1.0)], stroke);
            painter.line_segment([p(-8.0, 3.0), p(0.0, 9.0)], stroke);
            painter.line_segment([p(8.0, 3.0), p(0.0, 9.0)], stroke);
        }
        K::Resort => {
            // Beach umbrella.
            painter.add(poly(&[(-9.0, 0.0), (0.0, -8.0), (9.0, 0.0)]));
            painter.line_segment([p(0.0, 0.0), p(0.0, 9.0)], stroke);
        }
        K::FishingVillage => {
            // Fish: body + tail.
            painter.add(poly(&[(-8.0, 0.0), (-2.0, -5.0), (5.0, -4.0), (8.0, 0.0), (5.0, 4.0), (-2.0, 5.0)]));
            painter.add(poly(&[(-8.0, 0.0), (-11.0, -5.0), (-11.0, 5.0)]));
        }
        K::Research => {
            // Dish on a mast.
            painter.circle_stroke(p(0.0, -3.0), 6.0 * s, stroke);
            painter.line_segment([p(0.0, 3.0), p(0.0, 9.0)], stroke);
            painter.line_segment([p(-4.0, 9.0), p(4.0, 9.0)], stroke);
        }
        K::LumberCamp => {
            // Stacked logs.
            for (x, y) in [(-4.0, 4.0), (4.0, 4.0), (0.0, -3.0)] {
                painter.circle_stroke(p(x, y), 3.6 * s, stroke);
            }
        }
        K::Village => {
            painter.add(poly(&[(-8.0, 0.0), (-4.0, -5.0), (0.0, 0.0)]));
            painter.rect_filled(egui::Rect::from_min_max(p(-7.0, 0.0), p(-1.0, 6.0)), 0.0, w);
            painter.add(poly(&[(0.0, 1.0), (4.0, -4.0), (8.0, 1.0)]));
            painter.rect_filled(egui::Rect::from_min_max(p(1.0, 1.0), p(7.0, 7.0)), 0.0, w);
        }
        K::Mine => {
            // Headframe: a-frame with a wheel on top.
            painter.line_segment([p(-7.0, 9.0), p(0.0, -6.0)], stroke);
            painter.line_segment([p(7.0, 9.0), p(0.0, -6.0)], stroke);
            painter.line_segment([p(-4.0, 3.0), p(4.0, 3.0)], stroke);
            painter.circle_stroke(p(0.0, -6.0), 2.8 * s, stroke);
        }
        K::City => {
            // Skyline.
            painter.rect_filled(egui::Rect::from_min_max(p(-8.0, -2.0), p(-3.0, 8.0)), 0.0, w);
            painter.rect_filled(egui::Rect::from_min_max(p(-2.0, -8.0), p(2.0, 8.0)), 0.0, w);
            painter.rect_filled(egui::Rect::from_min_max(p(3.0, 0.0), p(8.0, 8.0)), 0.0, w);
        }
        K::Canyon => {
            // A gorge: two walls with the strip between.
            painter.add(poly(&[(-9.0, -7.0), (-3.0, 7.0), (-9.0, 7.0)]));
            painter.add(poly(&[(9.0, -7.0), (9.0, 7.0), (3.0, 7.0)]));
            painter.line_segment([p(-2.0, 7.0), p(2.0, 7.0)], stroke);
        }
        K::Mesa => {
            // Flat-topped hill.
            painter.add(poly(&[(-9.0, 7.0), (-4.0, -5.0), (4.0, -5.0), (9.0, 7.0)]));
        }
    }
}

fn goods_list(goods: &[crate::missions::CargoKind]) -> String {
    goods.iter().map(|g| g.name()).collect::<Vec<_>>().join(", ")
}

/// Draws the island map: explored 1 km tiles show terrain, the rest stay blank. Airports
/// appear once their tile is explored (or when they matter to an active mission).
pub fn draw_map(
    ui: &mut egui::Ui,
    tex: &egui::TextureHandle,
    terrain: &Terrain,
    profile: &Profile,
    size: f32,
    plane: Option<(Vec3, f32)>,
    highlight: &[(&str, egui::Color32)],
) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(252, 252, 252));
    let n = profile::MAP_TILES;
    let tile = size / n as f32;
    for tz in 0..n {
        for tx in 0..n {
            let seen = profile.explored.get(tz * n + tx).copied().unwrap_or(0) != 0;
            let r = egui::Rect::from_min_size(egui::pos2(rect.min.x + tx as f32 * tile, rect.min.y + tz as f32 * tile), egui::vec2(tile, tile));
            if seen {
                let uv = egui::Rect::from_min_max(
                    egui::pos2(tx as f32 / n as f32, tz as f32 / n as f32),
                    egui::pos2((tx + 1) as f32 / n as f32, (tz + 1) as f32 / n as f32),
                );
                painter.image(tex.id(), r, uv, egui::Color32::WHITE);
            }
        }
    }
    let grid = egui::Stroke::new(1.0_f32, egui::Color32::from_rgba_unmultiplied(120, 120, 130, 40));
    for k in 0..=n {
        let x = rect.min.x + k as f32 * tile;
        let y = rect.min.y + k as f32 * tile;
        painter.line_segment([egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)], grid);
        painter.line_segment([egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)], grid);
    }
    let to_screen = |x: f32, z: f32| -> egui::Pos2 {
        egui::pos2(
            rect.min.x + (x + world::WORLD_HALF) / (2.0 * world::WORLD_HALF) * size,
            rect.min.y + (z + world::WORLD_HALF) / (2.0 * world::WORLD_HALF) * size,
        )
    };
    // Markers scale with the tiles so one landmark never crowds a square.
    let mr = (tile * 0.30).clamp(7.0, 15.0);
    let hover = resp.hover_pos();
    let mut hovered: Option<&Airport> = None;
    for a in &terrain.airports {
        let hl = highlight.iter().find(|(id, _)| *id == a.id()).map(|(_, c)| *c);
        let known = a.is_home() || hl.is_some() || profile.is_explored(a.def.pos.x, a.def.pos.y);
        if !known {
            continue;
        }
        let p = to_screen(a.def.pos.x, a.def.pos.y);
        // Plain airports are amber; the active mission's pick-up and delivery are dark.
        let ring = if hl.is_some() { egui::Color32::from_rgb(30, 32, 40) } else { egui::Color32::from_rgb(214, 140, 36) };
        painter.circle_filled(p, mr, ring);
        painter.circle_stroke(p, mr, egui::Stroke::new(1.5_f32, egui::Color32::WHITE));
        kind_glyph(&painter, p, mr * 0.78, a.def.kind);
        if let Some(col) = hl {
            painter.circle_stroke(p, mr + 2.0, egui::Stroke::new(2.0_f32, col));
        }
        let near = hover.map_or(false, |h| h.distance(p) <= mr + 2.0);
        if near {
            hovered = Some(a);
        }
        if hl.is_some() || near {
            let galley = painter.layout_no_wrap(a.name().to_string(), egui::FontId::proportional(11.5), ui::TEXT);
            let lp = p + egui::vec2(mr + 4.0, -galley.size().y * 0.5);
            painter.rect_filled(egui::Rect::from_min_size(lp, galley.size()).expand(2.0), 4.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 210));
            painter.galley(lp, galley, ui::TEXT);
        }
    }
    if let Some((pos, heading)) = plane {
        // The player is a small dark arrow, like the reference map.
        let p = to_screen(pos.x, pos.z);
        let pr = mr * 0.95;
        let d = egui::vec2(heading.sin(), -heading.cos());
        let side = egui::vec2(-d.y, d.x);
        let tip = p + d * pr;
        let l = p - d * pr * 0.7 + side * pr * 0.6;
        let r = p - d * pr * 0.7 - side * pr * 0.6;
        let notch = p - d * pr * 0.35;
        painter.add(egui::Shape::convex_polygon(vec![tip, l, notch], egui::Color32::from_rgb(30, 32, 40), egui::Stroke::new(1.2_f32, egui::Color32::WHITE)));
        painter.add(egui::Shape::convex_polygon(vec![tip, notch, r], egui::Color32::from_rgb(30, 32, 40), egui::Stroke::new(1.2_f32, egui::Color32::WHITE)));
    }
    if let Some(a) = hovered {
        egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), egui::Id::new("map-airport-tip"), |ui| {
            ui.label(egui::RichText::new(a.name()).strong());
            if !a.def.produces.is_empty() {
                ui.label(egui::RichText::new(format!("Produces: {}", goods_list(a.def.produces))).size(12.0));
            }
            if !a.def.needs.is_empty() {
                ui.label(egui::RichText::new(format!("Needs: {}", goods_list(a.def.needs))).size(12.0));
            }
        });
    }
    let km = tile;
    let a = egui::pos2(rect.min.x + 10.0, rect.max.y - 12.0);
    painter.line_segment([a, a + egui::vec2(km, 0.0)], egui::Stroke::new(2.0_f32, ui::TEXT));
    painter.text(a + egui::vec2(km + 4.0, 0.0), egui::Align2::LEFT_CENTER, "1 km", egui::FontId::proportional(11.0), ui::TEXT);
}

#[allow(clippy::too_many_arguments)]
fn flight_ui(
    mut contexts: EguiContexts,
    mut sim: ResMut<Sim>,
    mut ui_state: ResMut<FlightUi>,
    mut profile: ResMut<Profile>,
    terrain: Res<Terrain>,
    mut toasts: ResMut<Toasts>,
    mut map_tex: ResMut<MapTexture>,
    mut radio: ResMut<crate::Radio>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    if !sim.active {
        return;
    }
    let ctx = contexts.ctx_mut();
    let screen = ctx.screen_rect();
    if ui_state.free {
        egui::Area::new(egui::Id::new("freecam-hint")).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -14.0)).interactable(false).show(ctx, |ui| {
            let p = ui_state.free_pos;
            ui::shadow_label(ui, &format!("Free camera   x {:.0}  y {:.0}  z {:.0}   |   W A S D  Q E  Shift  right-drag   |   F: back to the plane", p.x, p.y, p.z), 13.0, egui::Color32::WHITE);
        });
        return;
    }
    let heading = world::heading_deg(sim.rot * Vec3::NEG_Z) % 360.0;
    let kmh = sim.airspeed * 3.6;
    // Test Pilot clock.
    if profile.trial.is_some() {
        let text = match (sim.trial_done, sim.trial_start) {
            (Some(t), _) => format!("TRIAL  {}  done", fmt_time_tenths(t as f32)),
            (None, Some(t0)) => format!("TRIAL  {}", fmt_time_tenths((sim.time - t0) as f32)),
            (None, None) => "TRIAL  clock starts when you load".to_string(),
        };
        egui::Area::new(egui::Id::new("trial-clock")).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 14.0)).interactable(false).show(ctx, |ui| {
            ui::shadow_label(ui, &text, 26.0, egui::Color32::from_rgb(255, 225, 120));
        });
    }

    // ---- world markers: the mission objective ("!") and known airports nearby
    let active = profile.active_missions();
    if let Ok((cam, cam_tf)) = cams.single() {
        // world_to_viewport gives logical pixels; the egui overlay is scaled (ui_scale), so
        // convert to egui points or every marker lands at the wrong spot.
        let to_points = cam.logical_viewport_size().map_or(1.0, |v| screen.width() / v.x.max(1.0));
        let project = move |world_pos: Vec3| -> Option<egui::Pos2> {
            let in_front = (world_pos - cam_tf.translation()).dot(cam_tf.forward().as_vec3()) > 0.0;
            if !in_front {
                return None;
            }
            cam.world_to_viewport(cam_tf, world_pos).ok().map(|v| egui::pos2(v.x * to_points, v.y * to_points))
        };
        let clamp = |p: Option<egui::Pos2>, world_pos: Vec3| -> egui::Pos2 {
            let margin = 48.0;
            match p {
                Some(p) if screen.contains(p) => p,
                Some(p) => egui::pos2(p.x.clamp(screen.min.x + margin, screen.max.x - margin), p.y.clamp(screen.min.y + margin + 70.0, screen.max.y - margin - 140.0)),
                None => {
                    let rel = cam_tf.compute_transform().rotation.inverse() * (world_pos - cam_tf.translation());
                    let x = if rel.x < 0.0 { screen.min.x + margin } else { screen.max.x - margin };
                    egui::pos2(x, screen.min.y + margin + 70.0)
                }
            }
        };
        let plane2 = Vec2::new(sim.pos.x, sim.pos.z);
        // Objective marker.
        let objective: Option<&Airport> = active.first().map(|m| {
            let aboard = sim.cargo.iter().any(|c| c.mission.id == m.id);
            terrain.airport(if aboard { m.to } else { m.from })
        });
        // Markers fade out over the last 1.4 km so they never sit on top of the runway you
        // are lining up on (Adam: the name tags made it hard to see the strip when landing).
        // Fully visible beyond 2.5 km, gone inside 800 m: the whole final approach is flown
        // with the runway unobstructed.
        let near_fade = |d: f32| ((d - 800.0) / 1700.0).clamp(0.0, 1.0);
        // Markers hang 120 m above the field, so on a 3 degree final they float above the
        // strip rather than on it.
        const MARKER_LIFT: f32 = 120.0;
        if let Some(target) = objective {
            let world_pos = target.center() + Vec3::new(0.0, MARKER_LIFT, 0.0);
            let dist = (target.def.pos - plane2).length();
            let dist_m = dist.round() as u32;
            let fade = near_fade(dist);
            let p = clamp(project(world_pos), world_pos);
            if fade > 0.0 {
                // Name pill on top, then the distance, then the pin whose tip marks the field.
                egui::Area::new(egui::Id::new("waypoint-objective")).fixed_pos(p - egui::vec2(70.0, 96.0)).interactable(false).show(ctx, |ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(140.0, 118.0), egui::Sense::hover());
                    let painter = ui.painter();
                    let c = egui::pos2(r.center().x, r.min.y + 92.0);
                    let dark = egui::Color32::from_rgb(30, 32, 40).gamma_multiply(fade);
                    let white = egui::Color32::WHITE.gamma_multiply(fade);
                    let pill = egui::Rect::from_center_size(egui::pos2(r.center().x, r.min.y + 12.0), egui::vec2(136.0, 22.0));
                    painter.rect_filled(pill, 11.0, egui::Color32::from_rgba_unmultiplied(40, 44, 60, (170.0 * fade) as u8));
                    painter.text(pill.center(), egui::Align2::CENTER_CENTER, target.name(), egui::FontId::proportional(13.0), white);
                    // Distance in metres with a thousands separator, like the reference pin.
                    let text = if dist_m >= 1000 { format!("{},{:03}", dist_m / 1000, dist_m % 1000) } else { dist_m.to_string() };
                    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(14.0), white);
                    let tp = egui::pos2(r.center().x - galley.size().x * 0.5, r.min.y + 30.0);
                    painter.galley(tp + egui::vec2(1.0, 1.5), galley.clone(), egui::Color32::from_black_alpha(120).gamma_multiply(fade));
                    painter.galley(tp, galley, white);
                    painter.circle_filled(c - egui::vec2(0.0, 26.0), 15.0, dark);
                    painter.circle_stroke(c - egui::vec2(0.0, 26.0), 15.0, egui::Stroke::new(1.5_f32, white));
                    if fade > 0.6 {
                        kind_glyph(&painter, c - egui::vec2(0.0, 26.0), 11.0, target.def.kind);
                    }
                    let tri = [egui::pos2(c.x, c.y), egui::pos2(c.x - 5.0, c.y - 11.0), egui::pos2(c.x + 5.0, c.y - 11.0)];
                    painter.add(egui::Shape::convex_polygon(tri.to_vec(), dark, egui::Stroke::NONE));
                });
            }
        }
        // Nearby known airports as name pills (up to three, within 9 km).
        let mut near: Vec<(&Airport, f32)> = terrain
            .airports
            .iter()
            .filter(|a| a.is_home() || profile.is_explored(a.def.pos.x, a.def.pos.y) || active.iter().any(|m| m.from == a.id() || m.to == a.id()))
            .map(|a| (a, (a.def.pos - plane2).length()))
            .filter(|(a, d)| *d < 9000.0 && *d > 800.0 && objective.map_or(true, |o| o.id() != a.id()))
            .collect();
        near.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        for (k, (a, d)) in near.iter().take(3).enumerate() {
            let world_pos = a.center() + Vec3::new(0.0, MARKER_LIFT, 0.0);
            let Some(p) = project(world_pos) else { continue };
            if !screen.contains(p) {
                continue;
            }
            let fade = near_fade(*d);
            if fade <= 0.0 {
                continue;
            }
            let white = egui::Color32::WHITE.gamma_multiply(fade);
            egui::Area::new(egui::Id::new(("waypoint-airport", k))).fixed_pos(p - egui::vec2(60.0, 14.0)).interactable(false).show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(120.0, 44.0), egui::Sense::hover());
                let painter = ui.painter();
                painter.text(egui::pos2(r.center().x, r.min.y + 6.0), egui::Align2::CENTER_CENTER, format!("{:.3}", d / 1000.0), egui::FontId::proportional(12.0), white);
                let pill = egui::Rect::from_center_size(egui::pos2(r.center().x, r.min.y + 26.0), egui::vec2(110.0, 22.0));
                painter.rect_filled(pill, 11.0, egui::Color32::from_rgba_unmultiplied(40, 44, 60, (170.0 * fade) as u8));
                painter.text(pill.center(), egui::Align2::CENTER_CENTER, a.name(), egui::FontId::proportional(13.0), white);
            });
        }
    }

    // ---- warnings (top center): title + hint cards
    let red = egui::Color32::from_rgb(200, 60, 55);
    let amber = egui::Color32::from_rgb(210, 140, 30);
    let mut warnings: Vec<(&str, &str, egui::Color32)> = Vec::new();
    if sim.stalled && sim.crashed.is_none() {
        warnings.push(("Stall", "Lower the nose and add power", red));
    }
    if sim.has_fuel_engines && sim.fuel <= 0.0 {
        warnings.push(("Out of Fuel", "Glide to a runway", red));
    } else if sim.has_fuel_engines && sim.fuel_cap > 0.0 && sim.fuel < (sim.fuel_cap * 0.15).max(2.5) {
        warnings.push(("Low Fuel", "Find a place to land", red));
    }
    if sim.has_electric && sim.charge <= 0.0 {
        warnings.push(("Battery Empty", "Glide to a runway", red));
    }
    if sim.wheel_capacity > 0.0 && sim.mass > sim.wheel_capacity {
        warnings.push(("Wheel load too heavy", "Add more wheels or add stronger wheels", amber));
    }
    if sim.scraping && sim.crashed.is_none() {
        warnings.push(("Scraping the ground", "Slow down and stop", amber));
    }
    if sim.oob {
        warnings.push(("Leaving the map", "Turn back", red));
    }
    if sim.pos.y > 5000.0 {
        warnings.push(("Too high", "Descend", amber));
    }
    if !warnings.is_empty() {
        egui::Area::new(egui::Id::new("hud-warn")).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 16.0)).interactable(false).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                for (title, hint, c) in warnings {
                    egui::Frame::new().fill(c).corner_radius(10.0).inner_margin(egui::Margin::symmetric(16, 6)).show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new(title).size(19.0).strong().color(egui::Color32::WHITE));
                            ui.label(egui::RichText::new(hint).size(12.0).italics().color(egui::Color32::from_rgb(255, 235, 230)));
                        });
                    });
                }
            });
        });
    }

    // ---- bottom left: throttle, bars, odometers
    egui::Area::new(egui::Id::new("hud-left")).anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(16.0, -16.0)).interactable(false).show(ctx, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Throttle").size(17.0).strong().color(egui::Color32::WHITE));
                ui.label(egui::RichText::new("(Space / Ctrl)").size(11.0).italics().color(egui::Color32::from_gray(230)));
            });
            let (rect, _) = ui.allocate_exact_size(egui::vec2(250.0, 18.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 9.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 210));
            let zero_x = rect.min.x + rect.width() * 0.25;
            let x = if sim.throttle >= 0.0 { zero_x + (rect.max.x - zero_x) * sim.throttle } else { zero_x + (zero_x - rect.min.x) * sim.throttle };
            let (x0, x1) = if x >= zero_x { (zero_x, x) } else { (x, zero_x) };
            p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, rect.min.y + 3.0), egui::pos2(x1, rect.max.y - 3.0)), 6.0, if sim.throttle >= 0.0 { egui::Color32::from_rgb(80, 180, 90) } else { egui::Color32::from_rgb(230, 130, 60) });
            p.line_segment([egui::pos2(zero_x, rect.min.y), egui::pos2(zero_x, rect.max.y)], egui::Stroke::new(1.0_f32, egui::Color32::from_gray(120)));
            p.circle_filled(egui::pos2(x, rect.center().y), 8.0, egui::Color32::WHITE);
            p.circle_stroke(egui::pos2(x, rect.center().y), 8.0, egui::Stroke::new(2.0_f32, egui::Color32::from_gray(60)));
            ui.add_space(6.0);
            if sim.has_electric {
                let frac = if sim.charge_cap > 0.0 { sim.charge / sim.charge_cap } else { 0.0 };
                ui::bar(ui, "Electricity", frac, egui::Color32::from_rgb(235, 180, 40));
            }
            if sim.fuel_cap > 0.0 {
                let frac = sim.fuel / sim.fuel_cap;
                let col = if sim.fuel < (sim.fuel_cap * 0.15).max(2.5) { egui::Color32::from_rgb(214, 62, 48) } else { egui::Color32::from_rgb(232, 138, 40) };
                ui::bar(ui, &format!("Fuel  {:.0} L", sim.fuel), frac, col);
            }
            // Cargo as weight (what the plane feels) over a bar of hold space used.
            let crates = sim.cargo.len();
            let slots = sim.cargo_slots.max(1) as usize;
            let cargo_col = if crates > slots { egui::Color32::from_rgb(214, 62, 48) } else { egui::Color32::from_rgb(96, 128, 196) };
            ui::bar(ui, &format!("Cargo  {:.0} kg  ({crates}/{slots})", sim.cargo_mass()), crates as f32 / slots as f32, cargo_col);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui::odometer(ui, "Speed", kmh.max(0.0).round() as u32, 4);
                ui.add_space(10.0);
                ui::odometer(ui, "Altitude", sim.pos.y.max(0.0).round() as u32, 4);
            });
            let gear = if sim.has_retract { if sim.gear_down { "  Gear down" } else { "  Gear up" } } else { "" };
            ui.label(egui::RichText::new(format!("HDG {:03.0}   V/S {:+.1} m/s{gear}", heading, sim.vel.y)).size(12.0).monospace().color(egui::Color32::from_gray(235)));
            ui.label(egui::RichText::new("Tab missions   M map   H help   Esc pause").size(11.0).color(egui::Color32::from_gray(210)));
        });
    });

    // ---- bottom right: money and mission cards
    egui::Area::new(egui::Id::new("hud-right")).anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0)).interactable(false).show(ctx, |ui| {
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
            pill(ui, &profile.money_text(), 20.0);
            for m in active.iter().take(3) {
                let p = profile.progress(m.id);
                let aboard: Vec<&CargoItem> = sim.cargo.iter().filter(|c| c.mission.id == m.id).collect();
                let from = terrain.airport(m.from);
                let to = terrain.airport(m.to);
                let target: &Airport = if aboard.is_empty() { from } else { to };
                let d = (target.def.pos - Vec2::new(sim.pos.x, sim.pos.z)).length() / 1000.0;
                ui.add_space(6.0);
                card(ui, |ui| {
                    ui.set_width(300.0);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), 10.0, egui::Color32::from_rgb(245, 190, 40));
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "!", egui::FontId::proportional(15.0), egui::Color32::WHITE);
                        ui.label(egui::RichText::new(format!("Delivering {}", m.cargo.name())).size(17.0).strong().color(ui::TEXT));
                    });
                    let body = if aboard.is_empty() {
                        format!("Collect {} at {} ({d:.1} km)", m.cargo.name().to_lowercase(), from.name())
                    } else {
                        format!("Deliver {} to {} ({d:.1} km)", m.cargo.name().to_lowercase(), to.name())
                    };
                    egui::Frame::new().fill(egui::Color32::from_rgb(236, 237, 242)).corner_radius(8.0).inner_margin(8.0).show(ui, |ui| {
                        ui.set_width(276.0);
                        ui.label(egui::RichText::new(body).size(14.0).color(ui::TEXT));
                        ui.label(egui::RichText::new(format!("{}/{} delivered, {} aboard", p.delivered, m.count, aboard.len())).size(12.0).color(egui::Color32::from_gray(110)));
                        if let Some(soonest) = aboard.iter().filter_map(|c| c.expires).fold(None, |acc: Option<f64>, e| Some(acc.map_or(e, |a| a.min(e)))) {
                            let left = (soonest - sim.time) as f32;
                            let col = if left < 30.0 { egui::Color32::from_rgb(220, 60, 60) } else { egui::Color32::from_rgb(200, 130, 20) };
                            ui.label(egui::RichText::new(format!("Time left {}", fmt_time(left))).size(13.0).strong().color(col));
                        }
                        if m.fragile && !aboard.is_empty() {
                            ui.label(egui::RichText::new("Fragile: land under 3.5 m/s (some breaks up to 7)").size(12.0).color(egui::Color32::from_rgb(200, 130, 20)));
                        }
                    });
                });
            }
        });
    });

    // ---- top right: round buttons (menu, missions, map) like the reference game
    if sim.crashed.is_none() {
        egui::Area::new(egui::Id::new("hud-corner")).anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 12.0)).show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                if ui::round_icon_btn(ui, ui::glyph_house, "Pause / menu (Esc)").clicked() {
                    ui_state.paused = true;
                }
                if ui::round_icon_btn(ui, ui::glyph_box, "Missions (Tab)").clicked() {
                    ui_state.show_missions = !ui_state.show_missions;
                }
                if ui::round_icon_btn(ui, ui::glyph_pin, "Map (M)").clicked() {
                    ui_state.show_map = !ui_state.show_map;
                }
            });
        });
    }

    // ---- top left: airport card when parked
    if let (Some(ap_id), None) = (sim.parked, sim.crashed.as_ref()) {
        let ap = terrain.airport(ap_id);
        egui::Area::new(egui::Id::new("airport-card")).anchor(egui::Align2::LEFT_TOP, egui::vec2(16.0, 16.0)).show(ctx, |ui| {
            card(ui, |ui| {
                ui.set_width(250.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(ap.name()).size(20.0).strong().color(ui::TEXT));
                    ui.label(egui::RichText::new("Airport").size(12.0).color(egui::Color32::from_gray(120)));
                });
                ui.separator();
                if !ap.def.produces.is_empty() {
                    ui.label(egui::RichText::new(format!("Produces: {}", goods_list(ap.def.produces))).size(12.0).color(egui::Color32::from_gray(110)));
                }
                if !ap.def.needs.is_empty() {
                    ui.label(egui::RichText::new(format!("Needs: {}", goods_list(ap.def.needs))).size(12.0).color(egui::Color32::from_gray(110)));
                }
                ui.add_space(2.0);
                let w = ui.available_width();
                let loads_here = active.iter().any(|m| m.from == ap_id);
                if loads_here || !sim.cargo.is_empty() {
                    if ui::icon_row_btn(ui, "📦", "Load Cargo", w, true).clicked() {
                        ui_state.show_cargo = true;
                    }
                }
                let need = (sim.fuel_cap - sim.fuel).max(0.0);
                if sim.fuel_cap > 0.0 {
                    let price = ap.def.fuel_price;
                    let cost = (need * price).ceil() as i64;
                    // Fuel is free: the only question is whether you have enough.
                    let label = if need <= 0.5 { "Refuel (full)".to_string() } else if price <= 0.0 { format!("Refuel  {need:.0} L") } else { format!("Refuel  {need:.0} L  ${cost}") };
                    if ui::icon_row_btn(ui, "⛽", &label, w, need > 0.5).clicked() {
                        let msg = refuel(&mut sim, &mut profile, ap);
                        toasts.push(msg);
                    }
                }
                if sim.has_electric && sim.charge < sim.charge_cap && ui::icon_row_btn(ui, "🔋", "Recharge (free)", w, true).clicked() {
                    sim.charge = sim.charge_cap;
                }
                if ui::icon_row_btn(ui, "📋", "Missions", w, true).clicked() {
                    ui_state.show_missions = true;
                }
                if ui::icon_row_btn(ui, "🔬", "Research", w, true).clicked() {
                    ui_state.show_research = true;
                }
                if ui::icon_row_btn(ui, "🔧", "Edit Plane", w, true).clicked() {
                    ui_state.go_edit = true;
                }
            });
        });
    }

    // ---- stuck under parachute
    if sim.stuck && sim.crashed.is_none() {
        egui::Window::new("Landed under parachute").anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label("The plane is down in one piece, but it is not going anywhere.");
            ui.horizontal(|ui| {
                if ui.add(ui::pill_btn("Return to hangar")).clicked() {
                    ui_state.go_hangar = true;
                }
                if ui.add(ui::pill_btn("Try again")).clicked() {
                    ui_state.restart = true;
                }
            });
        });
    }

    // ---- crash dialog
    if let Some(reason) = sim.crashed.clone() {
        if sim.crash_timer > 1.2 {
            egui::Window::new("Crashed").anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(egui::RichText::new(&reason).size(17.0));
                ui.label(egui::RichText::new("Any cargo aboard was lost. The plane is repaired for free.").color(egui::Color32::from_gray(120)));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(ui::pill_btn("Try again")).clicked() {
                        ui_state.restart = true;
                    }
                    if ui.add(ui::pill_btn("Edit plane")).clicked() {
                        ui_state.go_hangar = true;
                    }
                });
            });
        }
    }

    // ---- pause menu
    if ui_state.paused {
        egui::Window::new("Paused").anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_min_width(260.0);
            if ui.add(ui::pill_btn("Resume")).clicked() {
                ui_state.paused = false;
            }
            let mut assist = profile.assist;
            if ui.checkbox(&mut assist, "Stability assist (auto-level, extra damping)").changed() {
                profile.assist = assist;
            }
            ui.separator();
            let cargo_warn = if sim.cargo.is_empty() { "" } else { " (cargo aboard is lost)" };
            if ui.add(ui::pill_btn(&format!("Return to hangar{cargo_warn}"))).clicked() {
                ui_state.go_hangar = true;
            }
            if ui.add(ui::pill_btn("Restart at the runway")).clicked() {
                ui_state.restart = true;
            }
            if ui.add(ui::pill_btn("Main menu")).clicked() {
                ui_state.go_menu = true;
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("Flight time {}   Distance {:.1} km", fmt_time(sim.time as f32), sim.distance / 1000.0)).size(12.0).color(egui::Color32::from_gray(120)));
        });
    }

    // ---- cargo dialog
    if ui_state.show_cargo {
        let mut open = true;
        egui::Window::new("Load Cargo").open(&mut open).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(300.0);
                    let here = sim.parked;
                    let mut any = false;
                    for m in active.iter().filter(|m| Some(m.from) == here) {
                        any = true;
                        let (remaining, n, fits) = loadable(&sim, &profile, m);
                        egui::Frame::new().fill(egui::Color32::from_rgb(236, 237, 242)).corner_radius(10.0).inner_margin(10.0).show(ui, |ui| {
                            ui.set_width(280.0);
                            ui.vertical_centered(|ui| {
                                ui.label(egui::RichText::new(m.cargo.name()).size(16.0).strong().color(ui::TEXT));
                            });
                            ui.label(egui::RichText::new(format!("{} of {} still to load  |  {:.0} kg each", remaining, m.count, m.cargo.mass())).size(13.0).color(egui::Color32::from_gray(90)));
                            if let Some(t) = m.time_limit {
                                ui.label(egui::RichText::new(format!("Spoils {} after loading", fmt_time(t))).size(12.0).color(egui::Color32::from_rgb(200, 130, 20)));
                            }
                            if m.fragile {
                                ui.label(egui::RichText::new("Fragile").size(12.0).color(egui::Color32::from_rgb(200, 130, 20)));
                            }
                            if n == 0 && remaining > 0 {
                                ui.label(egui::RichText::new(if m.cargo.is_passenger() { "No free seats" } else { "No free cargo slots" }).size(12.0).color(egui::Color32::from_rgb(200, 60, 60)));
                            }
                            // Pick how many to take: starts at what is due, up to what fits.
                            let key = egui::Id::new(("take-count", m.id));
                            let mut take: u32 = ui.data(|d| d.get_temp(key)).unwrap_or(n).clamp(if fits > 0 { 1 } else { 0 }, fits);
                            ui.horizontal(|ui| {
                                if ui.add_enabled(take > 1, ui::pill_btn("-")).clicked() {
                                    take -= 1;
                                }
                                let extra = take.saturating_sub(remaining);
                                let text = if extra > 0 { format!("{take}  ({extra} spare)") } else { format!("{take}") };
                                ui.label(egui::RichText::new(text).size(16.0).strong().color(ui::TEXT));
                                if ui.add_enabled(take < fits, ui::pill_btn("+")).clicked() {
                                    take += 1;
                                }
                            });
                            ui.data_mut(|d| d.insert_temp(key, take));
                            let wide = egui::vec2(ui.available_width(), 30.0);
                            if ui.add_enabled(take > 0, egui::Button::new(egui::RichText::new(format!("Take {take}")).size(15.0).strong().color(ui::TEXT)).fill(egui::Color32::WHITE).corner_radius(15.0).min_size(wide)).clicked() {
                                let got = load_cargo(&mut sim, &profile, m, take);
                                toasts.push(format!("Loaded {got} x {} ({:.0} kg).", m.cargo.name(), got as f32 * m.cargo.mass()));
                                ui.data_mut(|d| d.remove::<u32>(key));
                            }
                        });
                        ui.add_space(6.0);
                    }
                    if !any {
                        ui.label(egui::RichText::new("No active mission starts at this airport.").color(egui::Color32::from_gray(110)));
                    }
                });
                ui.add_space(8.0);
                egui::Frame::new().fill(egui::Color32::from_rgb(236, 237, 242)).corner_radius(10.0).inner_margin(10.0).show(ui, |ui| {
                  // The frame sits in a horizontal row; its contents must stack vertically.
                  ui.vertical(|ui| {
                    ui.set_width(220.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("Inventory").size(16.0).strong().color(ui::TEXT));
                    });
                    let crates = sim.cargo.iter().filter(|c| !c.mission.cargo.is_passenger()).count();
                    let pax = sim.cargo.iter().filter(|c| c.mission.cargo.is_passenger()).count();
                    ui.label(egui::RichText::new(format!("{:.0} kg aboard", sim.cargo_mass())).size(13.0).color(ui::TEXT));
                    ui.label(egui::RichText::new(format!("Cargo {crates} / {}", sim.cargo_slots)).size(13.0).color(ui::TEXT));
                    if sim.seats > 0 || pax > 0 {
                        ui.label(egui::RichText::new(format!("Passengers {pax} / {}", sim.seats)).size(13.0).color(ui::TEXT));
                    }
                    let mut groups: Vec<(&str, usize)> = Vec::new();
                    for c in &sim.cargo {
                        if let Some(g) = groups.iter_mut().find(|g| g.0 == c.mission.id) {
                            g.1 += 1;
                        } else {
                            groups.push((c.mission.id, 1));
                        }
                    }
                    for (id, n) in groups {
                        if let Some(m) = missions::mission(id) {
                            ui.label(egui::RichText::new(format!("{} x {}  ->  {}", n, m.cargo.name(), terrain.airport(m.to).name())).size(12.0).color(egui::Color32::from_gray(90)));
                        }
                    }
                    ui.add_space(6.0);
                    let wide = egui::vec2(ui.available_width(), 30.0);
                    if ui.add_enabled(!sim.cargo.is_empty(), egui::Button::new(egui::RichText::new("Clear Inventory").size(14.0).strong().color(ui::TEXT)).fill(egui::Color32::WHITE).corner_radius(15.0).min_size(wide)).clicked() {
                        sim.cargo.clear();
                        sim.update_mass();
                        toasts.push("Cargo unloaded here. Reload it at its origin.");
                    }
                  });
                });
            });
        });
        ui_state.show_cargo = open;
    }

    // ---- research (from the airport card)
    if ui_state.show_research {
        let mut open = true;
        egui::Window::new("Research").open(&mut open).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).resizable(false).collapsible(false).default_width(420.0).show(ctx, |ui| {
            crate::builder::research_panel(ui, &mut profile, &mut toasts);
        });
        ui_state.show_research = open;
    }

    // ---- map
    if ui_state.show_map {
        let tex = map_tex.get(ctx, &terrain);
        let mut open = true;
        let hl: Vec<(&str, egui::Color32)> = active
            .iter()
            .flat_map(|m| [(m.from, egui::Color32::from_rgb(70, 150, 240)), (m.to, egui::Color32::from_rgb(60, 180, 90))])
            .collect();
        let map_size = (screen.height() - 130.0).clamp(360.0, 1000.0).min(screen.width() - 420.0);
        egui::Window::new("Map").open(&mut open).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).resizable(false).collapsible(false).title_bar(false).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    draw_map(ui, &tex, &terrain, &profile, map_size, Some((sim.pos, heading.to_radians())), &hl);
                    ui.label(egui::RichText::new("Fly over the grey tiles to chart them. Blue: pick-up. Green: destination.").size(12.0).color(egui::Color32::from_gray(110)));
                });
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.set_width(250.0);
                    if active.is_empty() {
                        ui.label(egui::RichText::new("No active missions").strong());
                        ui.label(egui::RichText::new("Open the mission board (Tab).").size(12.0).color(egui::Color32::from_gray(110)));
                    }
                    for m in active.iter() {
                        let p = profile.progress(m.id);
                        let aboard = sim.cargo.iter().filter(|c| c.mission.id == m.id).count() as u32;
                        ui.label(egui::RichText::new(format!("Delivering {}", m.cargo.name())).strong().size(16.0));
                        egui::Frame::new().fill(egui::Color32::from_rgb(236, 237, 242)).corner_radius(8.0).inner_margin(8.0).show(ui, |ui| {
                            ui.set_width(230.0);
                            let per_unit = (m.reward as f32 / m.count as f32).round() as i64;
                            let left = m.count.saturating_sub(p.delivered);
                            ui.label(egui::RichText::new(format!("Deliver {} {} to {}", left, m.cargo.name(), terrain.airport(m.to).name())).size(13.0));
                            ui.label(egui::RichText::new(format!("${per_unit} each  |  {aboard} aboard  |  +{} RP on completion", m.rp)).size(12.0).color(egui::Color32::from_gray(110)));
                        });
                        ui.add_space(4.0);
                    }
                });
            });
        });
        ui_state.show_map = open;
    }

    // ---- mission board
    if ui_state.show_missions {
        let mut open = true;
        egui::Window::new("Missions").open(&mut open).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).resizable(true).default_width(520.0).show(ctx, |ui| {
            mission_board(ui, &mut profile, &terrain, &mut toasts, &mut radio);
        });
        ui_state.show_missions = open;
    }

    // ---- help
    if ui_state.show_help {
        let mut open = true;
        egui::Window::new("Controls").open(&mut open).anchor(egui::Align2::LEFT_CENTER, egui::vec2(16.0, 0.0)).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.monospace("Space / Shift    throttle up");
            ui.monospace("Ctrl             throttle down (below zero = reverse thrust)");
            ui.monospace("Z / X            full throttle / cut throttle");
            ui.monospace("W / S  (arrows)  nose down / nose up");
            ui.monospace("A / D  (arrows)  roll left / right");
            ui.monospace("Q / E            rudder left / right (steers on the ground)");
            ui.monospace("G                retract / extend landing gear");
            ui.monospace("P                deploy parachute");
            ui.monospace("B                drag chute");
            ui.monospace("Right mouse      look around   Scroll: zoom");
            ui.monospace("Tab              missions");
            ui.monospace("M                map");
            ui.monospace("F                free camera (look around the world)");
            ui.monospace("Esc              pause");
            ui.separator();
            ui.label("There are no wheel brakes: use reverse thrust to stop. Roll to a near stop (under 18 km/h) anywhere on an airport's strip for the airport card:\nrefuel, load cargo, or edit the plane.");
        });
        ui_state.show_help = open;
    }
}

/// Mission list with accept / abandon buttons. Shared with the hangar.
pub fn mission_board(ui: &mut egui::Ui, profile: &mut Profile, terrain: &Terrain, toasts: &mut Toasts, radio: &mut crate::Radio) {
    let max_h = (ui.ctx().screen_rect().height() - 230.0).max(240.0);
    egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
        let mut header_done = false;
        for m in missions::all() {
            if m.contract && !header_done {
                header_done = true;
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Contracts").size(16.0).strong().color(ui::TEXT));
                ui.label(egui::RichText::new("Supply runs between places you have discovered. They repeat: fly them for money and research.").size(12.0).color(egui::Color32::from_gray(110)));
            }
            if m.contract && (profile.trial.is_some() || (!profile.mission_available(m.id) && !profile.progress(m.id).accepted)) {
                continue;
            }
            let status = mission_status(profile, m);
            let p = profile.progress(m.id);
            let available = profile.mission_available(m.id);
            let color = if p.completed {
                egui::Color32::from_gray(150)
            } else if p.accepted {
                egui::Color32::from_rgb(40, 140, 70)
            } else if available {
                ui::TEXT
            } else {
                egui::Color32::from_gray(150)
            };
            egui::Frame::new().fill(egui::Color32::from_rgb(236, 237, 242)).corner_radius(10.0).inner_margin(8.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(m.title).strong().size(16.0).color(color));
                        ui.label(egui::RichText::new(status).size(12.0).color(color));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if p.completed {
                            ui.label(egui::RichText::new("Done").color(egui::Color32::from_gray(130)));
                        } else if p.accepted {
                            if ui.add(ui::pill_btn("Abandon")).clicked() {
                                let pr = profile.progress_mut(m.id);
                                pr.accepted = false;
                                pr.delivered = 0;
                                profile::save_profile(profile);
                                toasts.push(format!("Abandoned {}.", m.title));
                            }
                        } else if available {
                            if ui.add(ui::pill_btn("Accept")).clicked() {
                                profile.progress_mut(m.id).accepted = true;
                                profile::save_profile(profile);
                                radio.say(m.story);
                                toasts.push(format!("Accepted {}. Load the cargo at {}.", m.title, terrain.airport(m.from).name()));
                            }
                        }
                    });
                });
                if available || p.accepted || p.completed {
                    ui.label(egui::RichText::new(m.story).size(13.0).color(egui::Color32::from_gray(90)));
                    let from = terrain.airport(m.from);
                    let to = terrain.airport(m.to);
                    let dist = (from.def.pos - to.def.pos).length() / 1000.0;
                    let mut line = format!(
                        "{} x {} ({:.0} kg each)  |  {} -> {}  ({:.1} km)  |  ${}  +{} RP",
                        m.count,
                        m.cargo.name(),
                        m.cargo.mass(),
                        from.name(),
                        to.name(),
                        dist,
                        m.reward,
                        m.rp
                    );
                    if let Some(t) = m.time_limit {
                        line.push_str(&format!("  |  {} per load", fmt_time(t)));
                    }
                    if m.fragile {
                        line.push_str("  |  FRAGILE");
                    }
                    if m.cargo.is_passenger() {
                        line.push_str("  |  needs seats");
                    }
                    ui.label(egui::RichText::new(line).size(12.5).strong().color(egui::Color32::from_gray(95)));
                    // What the job demands of the plane.
                    ui.horizontal_wrapped(|ui| {
                        for (text, hot) in mission_demands(m, dist) {
                            let fill = if hot { egui::Color32::from_rgb(255, 214, 120) } else { egui::Color32::from_rgb(222, 228, 240) };
                            egui::Frame::new().fill(fill).corner_radius(12.0).inner_margin(egui::Margin::symmetric(9, 3)).show(ui, |ui| {
                                ui.label(egui::RichText::new(text).size(11.5).strong().color(ui::TEXT));
                            });
                        }
                    });
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::cargo_burn_factor;

    #[test]
    fn heavier_cargo_burns_more_fuel() {
        assert!((cargo_burn_factor(0.0, 500.0) - 1.0).abs() < 1e-6);
        assert!(cargo_burn_factor(250.0, 500.0) > cargo_burn_factor(100.0, 500.0));
        assert!((cargo_burn_factor(500.0, 500.0) - 1.6).abs() < 1e-6);
        assert!(cargo_burn_factor(5000.0, 500.0) <= 2.8 + 1e-6, "capped");
    }
    use super::*;

    fn starter_sim() -> Sim {
        let mut sim = Sim::default();
        sim.build(&PlaneDesign::starter());
        sim
    }

    #[test]
    fn starter_plane_mass_and_balance() {
        let sim = starter_sim();
        assert!(sim.mass > 350.0 && sim.mass < 600.0, "mass {}", sim.mass);
        // Center of mass sits between the nose wheel and the main gear.
        let nose = sim.wheels.iter().map(|w| w.contact.z).fold(f32::MAX, f32::min);
        let main = sim.wheels.iter().map(|w| w.contact.z).fold(f32::MIN, f32::max);
        assert!(sim.com.z > nose && sim.com.z < main, "com z {} nose {} main {}", sim.com.z, nose, main);
        // The nose wheel steers.
        assert!(sim.wheels.iter().any(|w| w.steer_sign > 0.0));
        assert_eq!(sim.cargo_slots, 6);
        assert_eq!(sim.seats, 0);
        assert!(sim.has_fuel_engines && !sim.has_electric);
    }

    #[test]
    fn cargo_loads_and_delivers_with_rewards() {
        let mut sim = starter_sim();
        let mut profile = Profile::new_campaign();
        let m = missions::mission("first_flight").unwrap();
        profile.progress_mut(m.id).accepted = true;
        let money = profile.money;
        assert_eq!(load_cargo(&mut sim, &profile, m, 2), 2, "spares may be taken beyond the one due");
        assert_eq!(sim.cargo.len(), 2);
        let empty = sim.mass;
        sim.update_mass();
        assert!((sim.mass - empty).abs() < 1.0);
        // Wrong airport: nothing happens.
        let (msgs, changed) = deliver_cargo(&mut sim, &mut profile, m.from);
        assert!(msgs.is_empty() && !changed);
        let (msgs, changed) = deliver_cargo(&mut sim, &mut profile, m.to);
        assert!(changed && msgs.len() == 3, "{msgs:?}");
        assert!(msgs[1].contains("Mission complete"));
        assert!(msgs[2].contains("spare"), "the extra unloads unpaid: {msgs:?}");
        assert!(profile.progress(m.id).completed);
        assert_eq!(profile.money, money + m.reward);
        assert_eq!(profile.rp, m.rp);
        assert!(sim.cargo.is_empty());
        assert!(profile.mission_available("grocery_run"));
    }

    #[test]
    fn capacity_limits_loading() {
        let mut sim = starter_sim();
        let mut profile = Profile::new_campaign();
        let lumber = missions::mission("storm_damage").unwrap(); // 8 crates
        profile.progress_mut(lumber.id).accepted = true;
        assert_eq!(load_cargo(&mut sim, &profile, lumber, 8), 6, "six cargo slots on the starter");
        assert_eq!(loadable(&sim, &profile, lumber), (2, 0, 0));
        let base = sim.mass;
        sim.cargo.clear();
        sim.update_mass();
        assert!(base - sim.mass > 6.0 * 60.0, "lumber weighs something");
        let pax = missions::mission("passenger_charter").unwrap();
        profile.progress_mut(pax.id).accepted = true;
        assert_eq!(load_cargo(&mut sim, &profile, pax, 2), 0, "no seats on the starter");
        // Perishables get a deadline.
        let apples = missions::mission("apple_harvest").unwrap();
        profile.progress_mut(apples.id).accepted = true;
        assert_eq!(load_cargo(&mut sim, &profile, apples, 3), 3);
        assert!(sim.cargo.iter().all(|c| c.expires.is_some()));
    }

    #[test]
    fn lift_curve_stalls_and_recovers_sign() {
        assert!(cl_curve(0.1, 1.6) > 0.5);
        assert!(cl_curve(-0.1, 1.6) < -0.5);
        let peak = cl_curve(1.6 / CL_SLOPE, 1.6);
        assert!((peak - 1.6).abs() < 1e-4);
        assert!(cl_curve(0.6, 1.6) < peak && cl_curve(0.6, 1.6) > 0.0);
        assert_eq!(cl_curve(2.0, 1.6), 0.0);
    }

    #[test]
    fn shadowed_fuselage_has_less_drag_than_bare() {
        let d = PlaneDesign::starter();
        let areas = drag_areas(&d.parts);
        let fus = d.parts.iter().position(|p| p.id == "fus_cargo").unwrap();
        let bare = part_drag_area(&d.parts[fus]).z;
        assert!(areas[fus].z < bare * 0.5, "shadowed {} vs bare {}", areas[fus].z, bare);
        let total: f32 = areas.iter().map(|a| a.z).sum();
        assert!(total < 1.2, "total frontal drag area {}", total);
    }
}
