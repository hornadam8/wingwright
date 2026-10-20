//! Scripted self-test, enabled with `WINGWRIGHT_AUTOTEST="<t>:<cmd> [arg],..."`.
//! Commands: new | creative | launch | hangar | thr X | pitch X | roll X | yaw X | hold DEG | bank DEG |
//! shot NAME | quit. `hold`/`bank` engage simple attitude controllers (use `pitch`/`roll` to disengage).
//! Screenshots are written to `WINGWRIGHT_SHOTDIR` (default: current directory).

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy_egui::EguiRenderOutput;

use crate::flight::Sim;
use crate::parts::Dir;
use crate::profile::Profile;
use crate::world::{self, Airport, Terrain};
use crate::GameState;

pub struct AutotestPlugin;

#[derive(Clone, Debug)]
enum Action {
    New(bool),
    Launch,
    Hangar,
    Throttle(f32),
    Pitch(f32),
    Roll(f32),
    Yaw(f32),
    Hold(f32),
    Alt(f32),
    /// Retract (false) or extend (true) the landing gear.
    Gear(bool),
    Bank(f32),
    Shot(String),
    /// Start a Test Pilot trial for a timed mission.
    Trial(String),
    /// Free camera at (x, y, z) with yaw/pitch in degrees.
    FreeCam(Vec3, f32, f32),
    /// Free camera looking at an airport from `dist` metres along `yaw` degrees, `elev` degrees up.
    Look(String, f32, f32, f32, Vec2),
    /// Free camera DIST m from the plane: yaw relative to its nose (0 = ahead looking back,
    /// 90 = off the right wing), elevation in degrees.
    LookPlane(f32, f32, f32),
    /// Back to the chase camera.
    Chase,
    /// Toggle the cargo (Load Cargo) dialog in flight.
    Cargo,
    /// Block the script until the plane is parked at an airport; later steps keep their
    /// relative timing (the whole remaining schedule shifts by the time spent waiting).
    WaitParked,
    /// Capture the real window (egui included) through the helper named by
    /// `WINGWRIGHT_WINID`; falls back to the in-engine screenshot without it.
    Cap(String),
    /// Write the top-down map (all tiles, no fog) as a PPM into the shot dir.
    MapDump(String),
    Accept(String),
    Load(String),
    Teleport(String),
    Map,
    Missions,
    Tab(String),
    Clear(Option<String>),
    Place(String, usize, Dir, Vec3, usize),
    Hover(String, usize, Dir, Vec3),
    /// Swap the cockpit for the given part id (the palette's Cockpit tab).
    Cockpit(String),
    Stats,
    Speed(f32),
    Nav(String),
    Check(String),
    Refuel,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum NavPhase {
    Takeoff,
    Climb,
    Cruise,
    Approach,
    Flare,
    Rollout,
    Done,
}

/// A small autopilot that flies to an airport and lands on its runway.
struct Nav {
    target: String,
    phase: NavPhase,
    takeoff_dir: Vec2,
    land_dir: Vec2,
    threshold: Vec2,
    elev: f32,
    phase_time: f32,
    reported: bool,
}

#[derive(Resource)]
pub struct Autotest {
    script: Vec<(f32, Action)>,
    next: usize,
    t: f32,
    pitch: f32,
    roll: f32,
    yaw: f32,
    throttle: Option<f32>,
    hold: Option<f32>,
    /// Altitude to hold (m); steers `hold` so the plane levels off and accelerates.
    alt: Option<f32>,
    bank: Option<f32>,
    nav: Option<Nav>,
    verdicts: Vec<String>,
    crash_logged: bool,
    log_timer: f32,
    shot_dir: String,
}

impl Plugin for AutotestPlugin {
    fn build(&self, app: &mut App) {
        let Ok(script) = std::env::var("WINGWRIGHT_AUTOTEST") else { return };
        let mut parsed = Vec::new();
        for item in script.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let Some((t, cmd)) = item.split_once(':') else { continue };
            let t: f32 = t.trim().parse().unwrap_or(0.0);
            let words: Vec<&str> = cmd.split_whitespace().collect();
            let name = words.first().copied().unwrap_or("");
            let arg = words.get(1).copied().unwrap_or("");
            let argf = |i: usize| words.get(i).and_then(|w| w.parse::<f32>().ok()).unwrap_or(0.0);
            let num = || arg.parse::<f32>().unwrap_or(0.0);
            let action = match name {
                "new" => Action::New(false),
                "creative" => Action::New(true),
                "launch" => Action::Launch,
                "hangar" => Action::Hangar,
                "thr" => Action::Throttle(num()),
                "pitch" => Action::Pitch(num()),
                "roll" => Action::Roll(num()),
                "yaw" => Action::Yaw(num()),
                "hold" => Action::Hold(num()),
                "alt" => Action::Alt(num()),
                "gear" => Action::Gear(arg != "up"),
                "bank" => Action::Bank(num()),
                "accept" => Action::Accept(arg.to_string()),
                "load" => Action::Load(arg.to_string()),
                "teleport" => Action::Teleport(arg.to_string()),
                "map" => Action::Map,
                "missions" => Action::Missions,
                "cargo" => Action::Cargo,
                "wait" => Action::WaitParked,
                "cap" => Action::Cap(arg.to_string()),
                "tab" => Action::Tab(arg.to_string()),
                "clear" => Action::Clear(if arg.is_empty() { None } else { Some(arg.to_string()) }),
                "cockpit" => Action::Cockpit(arg.to_string()),
                "hover" => {
                    let host = words.get(2).and_then(|w| w.parse::<usize>().ok()).unwrap_or(0);
                    let face = match words.get(3).copied().unwrap_or("PZ") {
                        "PX" => Dir::PX,
                        "NX" => Dir::NX,
                        "PY" => Dir::PY,
                        "NY" => Dir::NY,
                        "NZ" => Dir::NZ,
                        _ => Dir::PZ,
                    };
                    Action::Hover(arg.to_string(), host, face, Vec3::new(argf(4), argf(5), argf(6)))
                }
                "place" => {
                    // place PART HOST FACE [dx dy dz] [rotation index 0..23]
                    let host = words.get(2).and_then(|w| w.parse::<usize>().ok()).unwrap_or(0);
                    let k = words.get(7).and_then(|w| w.parse::<usize>().ok()).unwrap_or(0);
                    let face = match words.get(3).copied().unwrap_or("PZ") {
                        "PX" => Dir::PX,
                        "NX" => Dir::NX,
                        "PY" => Dir::PY,
                        "NY" => Dir::NY,
                        "NZ" => Dir::NZ,
                        _ => Dir::PZ,
                    };
                    Action::Place(arg.to_string(), host, face, Vec3::new(argf(4), argf(5), argf(6)), k)
                }
                "stats" => Action::Stats,
                "speed" => Action::Speed(num().max(0.1)),
                "nav" => Action::Nav(arg.to_string()),
                "check" => Action::Check(arg.to_string()),
                "refuel" => Action::Refuel,
                "shot" => Action::Shot(arg.to_string()),
                "trial" => Action::Trial(arg.to_string()),
                "freecam" => Action::FreeCam(Vec3::new(argf(1), argf(2), argf(3)), argf(4), argf(5)),
                "look" => Action::Look(arg.to_string(), if argf(2) > 0.0 { argf(2) } else { 400.0 }, argf(3), if argf(4) > 0.0 { argf(4) } else { 20.0 }, Vec2::new(argf(5), argf(6))),
                "chase" => Action::Chase,
                "lookplane" => Action::LookPlane(if argf(1) > 0.0 { argf(1) } else { 8.0 }, argf(2), argf(3)),
                "mapdump" => Action::MapDump(arg.to_string()),
                "quit" => Action::Quit,
                _ => continue,
            };
            parsed.push((t, action));
        }
        parsed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        info!("autotest: {} actions", parsed.len());
        app.insert_resource(Autotest {
            script: parsed,
            next: 0,
            t: 0.0,
            pitch: 0.0,
            roll: 0.0,
            yaw: 0.0,
            throttle: None,
            hold: None,
            alt: None,
            bank: None,
            nav: None,
            verdicts: Vec::new(),
            crash_logged: false,
            log_timer: 0.0,
            shot_dir: std::env::var("WINGWRIGHT_SHOTDIR").unwrap_or_else(|_| ".".into()),
        });
        app.add_systems(Update, (autotest_tick, log_egui));
    }
}

#[allow(clippy::too_many_arguments)]
fn autotest_tick(
    mut commands: Commands,
    time: Res<Time>,
    mut at: ResMut<Autotest>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: EventWriter<AppExit>,
    mut profile: Option<ResMut<Profile>>,
    mut sim: ResMut<Sim>,
    mut flight_ui: ResMut<crate::flight::FlightUi>,
    mut builder: ResMut<crate::builder::Builder>,
    terrain: Res<Terrain>,
    mut radio: ResMut<crate::Radio>,
    mut toasts: ResMut<crate::Toasts>,
    mut vtime: ResMut<Time<Virtual>>,
) {
    at.t += time.delta_secs();
    while at.next < at.script.len() && at.script[at.next].0 <= at.t {
        if matches!(at.script[at.next].1, Action::WaitParked) {
            if sim.crashed.is_some() {
                warn!("autotest t={:.1}: wait aborted, the plane crashed", at.t);
            } else if !sim.active || sim.parked.is_none() {
                break;
            }
            let late = (at.t - at.script[at.next].0).max(0.0);
            for k in at.next + 1..at.script.len() {
                at.script[k].0 += late;
            }
            info!("autotest t={:.1}: wait over, parked={:?} (waited {late:.1} s, later steps shifted)", at.t, sim.parked);
            at.next += 1;
            continue;
        }
        let action = at.script[at.next].1.clone();
        at.next += 1;
        info!("autotest t={:.1}: {:?}", at.t, action);
        match action {
            Action::New(creative) => {
                let p = if creative { Profile::new_creative() } else { Profile::new_campaign() };
                commands.insert_resource(p);
                next.set(GameState::Hangar);
            }
            Action::Launch => next.set(GameState::Flight),
            Action::Trial(id) => match crate::missions::mission(&id) {
                Some(m) => {
                    let p = Profile::new_trial(m);
                    info!("autotest: trial {} with ${} and {} RP", m.title, p.money, p.rp);
                    commands.insert_resource(p);
                    next.set(GameState::Hangar);
                }
                None => error!("autotest: unknown mission {id}"),
            },
            Action::Accept(id) => {
                if let Some(p) = profile.as_deref_mut() {
                    p.progress_mut(&id).accepted = true;
                    // Same radio story the mission board shows, so the card can be observed.
                    if let Some(m) = crate::missions::all().iter().find(|m| m.id == id) {
                        radio.say(m.story);
                    }
                }
            }
            Action::Load(id) => {
                if let (Some(p), Some(m)) = (profile.as_deref(), crate::missions::mission(&id)) {
                    // Just what the mission still needs (spares are the player's choice).
                    let (_, due, _) = crate::flight::loadable(&sim, p, m);
                    let n = crate::flight::load_cargo(&mut sim, p, m, due);
                    info!("autotest: loaded {n} for {id}");
                }
            }
            Action::Teleport(id) => {
                let ap = terrain.airport(&id);
                sim.place(&terrain, ap.spawn_point(), ap.yaw());
            }
            Action::Map => flight_ui.show_map = !flight_ui.show_map,
            Action::Missions => flight_ui.show_missions = !flight_ui.show_missions,
            Action::Cargo => flight_ui.show_cargo = !flight_ui.show_cargo,
            Action::WaitParked => {}
            Action::Cap(name) => {
                let path = format!("{}/{}.png", at.shot_dir, name);
                match std::env::var("WINGWRIGHT_WINID") {
                    Ok(helper) => {
                        std::thread::spawn(move || {
                            let id = std::process::Command::new(&helper)
                                .arg("wingwright")
                                .output()
                                .ok()
                                .and_then(|o| String::from_utf8(o.stdout).ok())
                                .map(|s| s.trim().to_string())
                                .unwrap_or_default();
                            if id.is_empty() {
                                warn!("autotest: cap {name}: game window not found (other Space?)");
                                return;
                            }
                            let status = std::process::Command::new("screencapture").args(["-x", "-l", &id, &path]).status();
                            info!("autotest: cap {name} -> {path} ({status:?})");
                        });
                    }
                    Err(_) => {
                        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
                    }
                }
            }
            Action::Tab(name) => builder.debug_set_tab(&name),
            Action::Clear(cockpit) => {
                if let Some(p) = profile.as_deref_mut() {
                    builder.debug_clear(p, cockpit.as_deref());
                    info!("autotest: cleared design, ${}", p.money);
                }
            }
            Action::Hover(id, host, face, off) => builder.debug_hover(&id, host, face, off),
            Action::Cockpit(id) => {
                if let (Some(p), Some(def)) = (profile.as_deref_mut(), crate::parts::part(&id)) {
                    crate::builder::swap_cockpit(&mut builder, p, &mut toasts, def);
                }
            }
            Action::Place(id, host, face, off, k) => {
                if let Some(p) = profile.as_deref_mut() {
                    match builder.debug_place(p, &id, host, face, off, k) {
                        Ok(msg) => info!("autotest: {msg}"),
                        Err(e) => {
                            error!("autotest: could not place {id} on {host} {face:?}: {e}");
                            at.verdicts.push(format!("FAIL place {id}: {e}"));
                        }
                    }
                }
            }
            Action::Stats => {
                let st = builder.design.stats();
                let money = profile.as_deref().map(|p| p.money).unwrap_or(0);
                info!(
                    "autotest: design parts={} cost=${} money=${} mass={:.0} slots={} payload={:.0} thrust={:.0} wing={:.1} stall={:.1} top={:.1} warnings={:?}",
                    builder.design.parts.len(),
                    st.cost,
                    money,
                    st.mass_empty,
                    st.cargo_slots,
                    st.payload,
                    st.thrust,
                    st.wing_area,
                    st.stall_speed,
                    st.top_speed,
                    st.warnings
                );
                if money < 0 {
                    at.verdicts.push("FAIL: money went negative".into());
                }
                if !st.warnings.is_empty() {
                    at.verdicts.push(format!("FAIL: design warnings {:?}", st.warnings));
                }
            }
            Action::Speed(x) => vtime.set_relative_speed(x),
            Action::Nav(id) => {
                let ap = terrain.airport(&id);
                let fwd = sim.rot * Vec3::NEG_Z;
                let takeoff_dir = Vec2::new(fwd.x, fwd.z).normalize_or_zero();
                at.nav = Some(Nav {
                    target: id.clone(),
                    phase: if sim.on_ground { NavPhase::Takeoff } else { NavPhase::Cruise },
                    takeoff_dir,
                    land_dir: ap.dir,
                    threshold: ap.def.pos,
                    elev: ap.elevation,
                    phase_time: 0.0,
                    reported: false,
                });
                info!("autotest: nav to {} ({})", ap.name(), id);
            }
            Action::Refuel => {
                if let (Some(p), Some(ap)) = (profile.as_deref_mut(), sim.parked) {
                    let ap = terrain.airport(ap);
                    let msg = crate::flight::refuel(&mut sim, p, ap);
                    info!("autotest: {msg} (fuel {:.1}/{:.0})", sim.fuel, sim.fuel_cap);
                } else {
                    at.verdicts.push("FAIL: refuel requested while not parked".into());
                }
            }
            Action::Check(id) => {
                let done = profile.as_deref().map(|p| p.progress(&id).completed).unwrap_or(false);
                let money = profile.as_deref().map(|p| p.money).unwrap_or(0);
                if done && at.verdicts.is_empty() {
                    info!("WALKTHROUGH OK: mission {id} completed, ${money}, crashes={}", profile.as_deref().map(|p| p.crashes).unwrap_or(0));
                } else {
                    error!("WALKTHROUGH FAIL: mission {id} completed={done}, issues={:?}, crashed={:?}, pos={:?}", at.verdicts, sim.crashed, sim.pos);
                }
            }
            Action::Hangar => next.set(GameState::Hangar),
            Action::Throttle(v) => at.throttle = Some(v),
            Action::Pitch(v) => {
                at.pitch = v;
                at.hold = None;
            }
            Action::Roll(v) => {
                at.roll = v;
                at.bank = None;
            }
            Action::Hold(v) => {
                at.hold = Some(v);
                at.alt = None;
            }
            Action::Alt(v) => at.alt = Some(v),
            Action::Gear(down) => sim.gear_down = down,
            Action::Bank(v) => at.bank = Some(v),
            Action::Yaw(v) => at.yaw = v,
            Action::Shot(name) => {
                let path = format!("{}/{}.png", at.shot_dir, name);
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            }
            Action::FreeCam(pos, yaw, pitch) => {
                flight_ui.free = true;
                flight_ui.free_pos = pos;
                flight_ui.free_yaw = yaw.to_radians();
                flight_ui.free_pitch = pitch.to_radians();
            }
            Action::Look(id, dist, yaw, elev, uv) => {
                // Optional runway-local offset (u along, v across) to frame the settlement.
                let ap = terrain.airport(&id);
                let target = if uv == Vec2::ZERO { ap.center() } else { ap.world3(uv.x, uv.y, 0.0) };
                // Yaw is relative to the runway: 0 puts the camera past the far end looking
                // back down the strip, 180 on the approach, 90 out on the apron side.
                let (yr, er) = (yaw.to_radians(), elev.to_radians());
                let horiz = ap.dir * yr.cos() + ap.right() * yr.sin();
                let offset = Vec3::new(horiz.x * er.cos(), er.sin(), horiz.y * er.cos()) * dist;
                let mut pos = target + offset;
                let ground = terrain.height(pos.x, pos.z).max(0.0);
                if pos.y < ground + 12.0 {
                    pos.y = ground + 12.0;
                }
                let d = (target - pos).normalize();
                flight_ui.free = true;
                flight_ui.free_pos = pos;
                flight_ui.free_yaw = (-d.x).atan2(-d.z);
                flight_ui.free_pitch = d.y.asin();
            }
            Action::LookPlane(dist, yaw, elev) => {
                let fwd = sim.rot * Vec3::NEG_Z;
                let fwd2 = Vec2::new(fwd.x, fwd.z).normalize_or_zero();
                let right2 = Vec2::new(-fwd2.y, fwd2.x);
                let (yr, er) = (yaw.to_radians(), elev.to_radians());
                let horiz = fwd2 * yr.cos() + right2 * yr.sin();
                let target = sim.pos + Vec3::Y * 0.6;
                let pos = target + Vec3::new(horiz.x * er.cos(), er.sin(), horiz.y * er.cos()) * dist;
                let d = (target - pos).normalize();
                flight_ui.free = true;
                flight_ui.free_pos = pos;
                flight_ui.free_yaw = (-d.x).atan2(-d.z);
                flight_ui.free_pitch = d.y.asin();
            }
            Action::Chase => flight_ui.free = false,
            Action::MapDump(name) => {
                const S: usize = 1024;
                let px = terrain.map_pixels(S);
                let mut out = format!("P6 {S} {S} 255\n").into_bytes();
                for p in &px {
                    out.extend_from_slice(&p[..3]);
                }
                let path = format!("{}/{}.ppm", at.shot_dir, name);
                match std::fs::write(&path, out) {
                    Ok(()) => info!("autotest: map written to {path}"),
                    Err(e) => error!("autotest: map dump failed: {e}"),
                }
            }
            Action::Quit => {
                exit.write(AppExit::Success);
                // The winit loop can hang on the way out (macOS): make sure we leave.
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(2000));
                    std::process::exit(0);
                });
            }
        }
    }
}

/// Frame-time statistics for one logging window.
#[derive(Default)]
struct FrameStats {
    frames: u32,
    max_ms: f32,
    over_25: u32,
    over_40: u32,
}

fn log_egui(time: Res<Time<Real>>, mut timer: Local<f32>, mut stats: Local<FrameStats>, outputs: Query<&EguiRenderOutput>) {
    let dt = time.delta_secs() * 1000.0;
    stats.frames += 1;
    stats.max_ms = stats.max_ms.max(dt);
    if dt > 25.0 {
        stats.over_25 += 1;
    }
    if dt > 40.0 {
        stats.over_40 += 1;
    }
    if dt > 20.0 {
        info!("slow frame {dt:.1} ms at real t={:.2}", time.elapsed_secs());
    }
    *timer += time.delta_secs();
    if *timer < 2.0 {
        return;
    }
    *timer = 0.0;
    info!("frame real dt={:.1} ms  window: frames={} max={:.1} ms over25={} over40={}", dt, stats.frames, stats.max_ms, stats.over_25, stats.over_40);
    *stats = FrameStats::default();
    for o in outputs.iter() {
        let tris: usize = o.paint_jobs.iter().map(|p| match &p.primitive {
            bevy_egui::egui::epaint::Primitive::Mesh(m) => m.indices.len() / 3,
            _ => 0,
        }).sum();
        info!("egui paint jobs={} triangles={}", o.paint_jobs.len(), tris);
    }
}

/// Runs between flight input and physics so scripted controls win over the keyboard.
pub fn flight_override(time: Res<Time>, at: Option<ResMut<Autotest>>, mut sim: ResMut<Sim>, terrain: Res<crate::world::Terrain>) {
    let Some(mut at) = at else { return };
    if !sim.active {
        return;
    }
    if let Some(t) = at.throttle.take() {
        sim.throttle = t;
    }
    let fwd = sim.rot * Vec3::NEG_Z;
    let right = sim.rot * Vec3::X;
    let w_body = sim.rot.inverse() * sim.ang_vel;
    if at.nav.is_some() {
        autopilot(&mut at, &mut sim, &terrain, time.delta_secs());
    }
    let mut pitch_cmd = at.pitch;
    let mut roll_cmd = at.roll;
    if let Some(target_alt) = at.alt {
        // Level-off controller: pitch a little for altitude error, damp the climb rate.
        let err = target_alt - sim.pos.y;
        at.hold = Some((err * 0.04 - sim.vel.y * 0.6).clamp(-6.0, 8.0));
    }
    if let Some(target) = at.hold {
        let pitch = fwd.y.asin().to_degrees();
        let q = w_body.x.to_degrees();
        pitch_cmd = ((target - pitch) * 0.18 - q * 0.05).clamp(-1.0, 1.0);
    }
    if let Some(target) = at.bank {
        let bank = (-right.y).asin().to_degrees();
        let p = (-w_body.z).to_degrees();
        roll_cmd = ((target - bank) * 0.05 - p * 0.02).clamp(-1.0, 1.0);
    }
    sim.set_cmds(pitch_cmd, roll_cmd, at.yaw);
    at.log_timer += time.delta_secs();
    if sim.crashed.is_none() {
        at.crash_logged = false;
    } else if at.crash_logged {
        return;
    } else {
        at.crash_logged = true;
        at.log_timer = 1.0;
    }
    if at.log_timer >= 1.0 {
        at.log_timer = 0.0;
        info!(
            "sim fps={:.0} phys={:.1}ms t={:.0} pos=({:.0},{:.1},{:.0}) agl={:.1} spd={:.1} vs={:+.1} hdg={:.0} pitch={:+.0} aoa={:+.1} ground={} stall={} thr={:.2} fuel={:.1} crashed={:?} parked={:?}",
            1.0 / time.delta_secs().max(1e-4),
            sim.physics_ms,
            sim.time,
            sim.pos.x,
            sim.pos.y,
            sim.pos.z,
            sim.pos.y - terrain.height(sim.pos.x, sim.pos.z),
            sim.airspeed,
            sim.vel.y,
            crate::world::heading_deg(fwd),
            fwd.y.asin().to_degrees(),
            sim.aoa.to_degrees(),
            sim.on_ground,
            sim.stalled,
            sim.throttle,
            sim.fuel,
            sim.crashed,
            sim.parked
        );
    }
}

fn wrap(a: f32) -> f32 {
    let mut a = a;
    while a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    }
    while a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}

fn bearing(from: Vec2, to: Vec2) -> f32 {
    let d = to - from;
    d.x.atan2(-d.y)
}

/// Drives `at.hold`, `at.bank`, `at.yaw` and the throttle to take off, fly to the nav target,
/// land on its runway and stop.
fn autopilot(at: &mut Autotest, sim: &mut Sim, terrain: &Terrain, dt: f32) {
    let fwd = sim.rot * Vec3::NEG_Z;
    let heading = fwd.x.atan2(-fwd.z);
    let pos2 = Vec2::new(sim.pos.x, sim.pos.z);
    let speed = sim.airspeed;
    let alt = sim.pos.y;
    let ground = terrain.height(sim.pos.x, sim.pos.z).max(0.0);
    let agl = alt - ground;
    let vs = sim.vel.y;
    let on_ground = sim.on_ground;
    let crashed = sim.crashed.is_some();
    let parked = sim.parked;
    let touchdown = sim.touchdown_impact;
    let Some(nav) = at.nav.as_mut() else { return };
    let ap: &Airport = terrain.airport(&nav.target);
    nav.phase_time += dt;
    if crashed {
        if !nav.reported {
            nav.reported = true;
            at.verdicts.push(format!("FAIL: crashed during nav in phase {:?}", nav.phase));
        }
        return;
    }
    let steer_to = |dir: Vec2, xtrack: f32| -> f32 {
        let target = dir.x.atan2(-dir.y) - (xtrack * 0.02).clamp(-0.35, 0.35);
        (wrap(target - heading) * 2.5).clamp(-1.0, 1.0)
    };
    let prev = nav.phase;
    // Runway-local position: u along the landing direction (negative before the threshold),
    // v across it (positive to the right).
    let rel = pos2 - nav.threshold;
    let u = rel.dot(nav.land_dir);
    let v = rel.dot(Vec2::new(-nav.land_dir.y, nav.land_dir.x));
    let land_hdg = nav.land_dir.x.atan2(-nav.land_dir.y);
    // Highest ground in the next 800 m along track: fly over hills, not into them.
    let fwd2 = Vec2::new(fwd.x, fwd.z).normalize_or_zero();
    let ground_ahead = (0..=8).map(|k| terrain.height(pos2.x + fwd2.x * k as f32 * 100.0, pos2.y + fwd2.y * k as f32 * 100.0)).fold(ground, f32::max);
    const SLOPE: f32 = 0.06; // 3.4 degree glide
    const AIM_U: f32 = 220.0; // aim point past the threshold (the path is flown a little low)
    // (pitch hold deg, bank deg, yaw cmd, throttle)
    let (hold, bank, yaw, thr) = match nav.phase {
        NavPhase::Takeoff => {
            // Wheels off: pick the landing end and turn onto the route straight away
            // (Adam: "you can start turning as soon as your wheels are off the ground").
            if agl > 3.0 && !on_ground && nav.phase_time > 1.0 {
                let to_field = (ap.def.pos - pos2).normalize_or_zero();
                let d = if ap.dir.dot(to_field) >= 0.0 { ap.dir } else { -ap.dir };
                nav.land_dir = d;
                nav.threshold = ap.def.pos - d * (ap.def.length * 0.5 - 40.0);
                nav.phase = NavPhase::Cruise;
                info!("nav: airborne at {:.1} m/s, direct to {} landing heading {:.0}", speed, ap.name(), world::heading_deg(Vec3::new(d.x, 0.0, d.y)));
            }
            (if speed > 32.0 { 9.0 } else { 0.0 }, 0.0, steer_to(nav.takeoff_dir, 0.0), 1.0)
        }
        NavPhase::Climb => {
            // Go-around: wings level to 25 m, then back onto the route.
            if agl > 25.0 {
                // Land on the end that gives a straight-in from where we are: the runway
                // direction that points the same way as our bearing to the field.
                let to_field = (ap.def.pos - pos2).normalize_or_zero();
                let d = if ap.dir.dot(to_field) >= 0.0 { ap.dir } else { -ap.dir };
                nav.land_dir = d;
                nav.threshold = ap.def.pos - d * (ap.def.length * 0.5 - 40.0);
                nav.phase = NavPhase::Cruise;
                info!("nav: cruise toward {} landing heading {:.0}", ap.name(), world::heading_deg(Vec3::new(d.x, 0.0, d.y)));
            }
            (8.0, 0.0, 0.0, 1.0)
        }
        NavPhase::Cruise => {
            // Join the extended centreline at an angle of 45 degrees or less, at least 1500 m
            // out (no detour to a fixed fix behind us); descend so we reach the join point at
            // glide altitude. Already in the funnel: straight onto final.
            let u_join = (u + v.abs()).min(-1500.0);
            let fix = nav.threshold + nav.land_dir * u_join;
            let dist = (fix - pos2).length();
            let err = wrap(bearing(pos2, fix) - heading);
            let max_bank = if agl < 30.0 { 15.0 } else if agl < 120.0 { 22.0 } else { 30.0 };
            let bank = (err.to_degrees() * 1.2).clamp(-max_bank, max_bank);
            let fix_alt = nav.elev + 2.0 + (AIM_U - u_join) * SLOPE;
            let cruise_alt = (nav.elev + 230.0).max(ground_ahead + 120.0);
            let alt_t = (fix_alt + (dist - 400.0).max(0.0) * SLOPE).min(cruise_alt).max(ground_ahead + 60.0);
            // Well below the target: pitch for best climb (the starter needs 8-9 degrees at
            // 37 m/s; at 5 degrees it just accelerates and never climbs).
            let hold = (3.0 + (alt_t - alt) * 0.15 - vs * 0.7).clamp(-6.0, 9.0);
            let thr = (0.6 + (42.0 - speed) * 0.06 + (alt_t - alt).clamp(0.0, 80.0) * 0.005).clamp(0.25, 1.0);
            let aligned = wrap(land_hdg - heading).abs() < 0.6;
            if dist < 350.0 || (u > -4500.0 && u < -1200.0 && v.abs() < 250.0 && aligned) {
                nav.phase = NavPhase::Approach;
                info!("nav: final, {:.0} m from threshold, xtrack {:.0} m, alt {:.0} (glide {:.0})", -u, v, alt, nav.elev + 2.0 + (AIM_U - u) * SLOPE);
            }
            (hold, bank, 0.0, thr)
        }
        NavPhase::Approach => {
            // Final: track the centreline, ride the glide to the aim point. Throttle holds the
            // path (low = more power), pitch trims speed and rate of descent.
            let target_hdg = land_hdg - (v * 0.0015).clamp(-0.5, 0.5);
            let err = wrap(target_hdg - heading);
            let bank = (err.to_degrees() * 1.3).clamp(-25.0, 25.0);
            let glide = nav.elev + 2.0 + (AIM_U - u).max(0.0) * SLOPE;
            let alt_t = if u < -400.0 { glide.max(ground_ahead + 25.0) } else { glide };
            let hold = (2.0 + (alt_t - alt) * 0.06 - vs * 0.5).clamp(-8.0, 8.0);
            let thr = (0.45 + (alt_t - alt) * 0.03 + (35.0 - speed) * 0.04).clamp(0.0, 1.0);
            if agl < 6.0 && u > -30.0 {
                nav.phase = NavPhase::Flare;
                info!("nav: flare at {:.1} m/s, {:.0} m past threshold, xtrack {:.0} m", speed, u, v);
            } else if on_ground && nav.phase_time > 2.0 {
                nav.phase = NavPhase::Rollout;
                warn!("nav: touched down early, {:.0} m before the threshold, xtrack {:.0} m", -u, v);
                at.verdicts.push(format!("FAIL: landed short at {} ({:.0} m before the threshold)", ap.name(), -u));
            } else if u > 250.0 && !on_ground {
                nav.phase = NavPhase::Climb;
                info!("nav: go around");
            }
            (hold, bank, 0.0, thr)
        }
        NavPhase::Flare => {
            if on_ground && nav.phase_time > 0.3 {
                nav.phase = NavPhase::Rollout;
                info!("nav: touchdown at {:.1} m/s, impact {:.1} m/s, {:.0} m past threshold", speed, touchdown, u);
            }
            let err = wrap(land_hdg - heading);
            (5.0, (err.to_degrees() * 1.0).clamp(-8.0, 8.0), 0.0, 0.0)
        }
        NavPhase::Rollout => {
            let yaw = steer_to(nav.land_dir, v) * 0.6;
            // Let the nose settle before pulling reverse.
            let mut thr = if nav.phase_time < 1.0 { 0.0 } else if speed > 2.5 { -0.7 } else { 0.0 };
            if speed < 1.0 && nav.phase_time > 1.0 {
                thr = 0.0;
                nav.phase = NavPhase::Done;
                info!("nav: stopped at {} ({:.0} m past threshold)", ap.name(), u);
            }
            (0.0, 0.0, yaw, thr)
        }
        NavPhase::Done => {
            if !nav.reported && nav.phase_time > 2.0 {
                nav.reported = true;
                info!("nav: parked={:?}", parked);
                if parked != Some(nav.target.as_str()) {
                    at.verdicts.push(format!("FAIL: stopped but not parked at {} (parked={:?})", nav.target, parked));
                }
            }
            (0.0, 0.0, 0.0, 0.0)
        }
    };
    if nav.phase != prev {
        nav.phase_time = 0.0;
    }
    at.hold = Some(hold);
    at.bank = Some(bank);
    at.yaw = yaw;
    at.throttle = Some(thr);
}
