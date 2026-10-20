//! Player profile (money, research, mission progress, current design) and disk persistence.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::design::PlaneDesign;
use crate::missions;
use crate::parts;

pub const STARTING_MONEY: i64 = 250;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MissionProgress {
    pub accepted: bool,
    pub delivered: u32,
    pub completed: bool,
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug)]
pub struct Profile {
    pub creative: bool,
    pub money: i64,
    pub rp: u32,
    pub unlocked: Vec<String>,
    #[serde(default)]
    pub missions: HashMap<String, MissionProgress>,
    pub design: PlaneDesign,
    #[serde(default)]
    pub flights: u32,
    #[serde(default)]
    pub crashes: u32,
    /// Repeatable supply contracts flown.
    #[serde(default)]
    pub contracts_done: u32,
    #[serde(default)]
    pub distance_km: f32,
    #[serde(default)]
    pub assist: bool,
    /// Airport the next flight starts from ("Edit Plane" at an airport keeps you there).
    #[serde(default = "default_spawn")]
    pub spawn_airport: String,
    /// Fog of war: one byte per 1 km map tile (MAP_TILES x MAP_TILES), 1 = seen.
    #[serde(default = "default_explored")]
    pub explored: Vec<u8>,
    /// Test Pilot trial in progress (mission id): never saved over the campaign.
    #[serde(default)]
    pub trial: Option<String>,
}

pub const MAP_TILES: usize = 24;

fn default_explored() -> Vec<u8> {
    let mut v = vec![0u8; MAP_TILES * MAP_TILES];
    // The home island's harbor and its neighbours start revealed.
    let (hx, hz) = tile_of(200.0, 600.0);
    for dz in -1i32..=1 {
        for dx in -1i32..=1 {
            let (x, z) = (hx as i32 + dx, hz as i32 + dz);
            if x >= 0 && z >= 0 && (x as usize) < MAP_TILES && (z as usize) < MAP_TILES {
                v[z as usize * MAP_TILES + x as usize] = 1;
            }
        }
    }
    v
}

/// Map tile containing world (x, z).
pub fn tile_of(x: f32, z: f32) -> (usize, usize) {
    let half = crate::world::WORLD_HALF;
    let size = 2.0 * half / MAP_TILES as f32;
    let tx = (((x + half) / size).floor() as i32).clamp(0, MAP_TILES as i32 - 1) as usize;
    let tz = (((z + half) / size).floor() as i32).clamp(0, MAP_TILES as i32 - 1) as usize;
    (tx, tz)
}

fn default_spawn() -> String {
    "harbor".into()
}

impl Profile {
    /// Saves from the 16-tile world keep their charted tiles in the middle of the 24-tile map.
    pub fn migrate_explored(&mut self) {
        let n = MAP_TILES;
        if self.explored.len() == n * n {
            return;
        }
        let old = (self.explored.len() as f32).sqrt().round() as usize;
        let mut v = default_explored();
        if old > 0 && old * old == self.explored.len() && old < n {
            let off = (n - old) / 2;
            for z in 0..old {
                for x in 0..old {
                    if self.explored[z * old + x] != 0 {
                        v[(z + off) * n + x + off] = 1;
                    }
                }
            }
        }
        self.explored = v;
    }


    pub fn new_campaign() -> Self {
        Profile {
            creative: false,
            // No starter plane (Adam): a bare cockpit and a budget that covers a first plane.
            money: STARTING_MONEY + PlaneDesign::starter().total_cost(),
            rp: 0,
            unlocked: parts::starting_parts(),
            missions: HashMap::new(),
            design: PlaneDesign::empty(),
            flights: 0,
            crashes: 0,
            contracts_done: 0,
            distance_km: 0.0,
            assist: true,
            spawn_airport: "harbor".into(),
            explored: default_explored(),
            trial: None,
        }
    }

    pub fn new_creative() -> Self {
        Profile {
            creative: true,
            money: 0,
            rp: 0,
            unlocked: parts::PARTS.iter().map(|p| p.id.to_string()).collect(),
            missions: HashMap::new(),
            design: PlaneDesign::starter(),
            flights: 0,
            crashes: 0,
            contracts_done: 0,
            distance_km: 0.0,
            assist: true,
            spawn_airport: "harbor".into(),
            explored: vec![1u8; MAP_TILES * MAP_TILES],
            trial: None,
        }
    }

    /// A Test Pilot trial: the money and research the campaign has paid out before this
    /// mission, starter parts only (research the rest), the mission accepted, parked at its
    /// pickup, whole map known. Held in memory only, never saved over the campaign.
    pub fn trial_budget(m: &missions::MissionDef) -> (i64, u32) {
        let (rewards, rp) = missions::resources_before(m.id);
        (STARTING_MONEY + PlaneDesign::starter().total_cost() + rewards, rp)
    }

    pub fn new_trial(m: &missions::MissionDef) -> Self {
        let (money, rp) = Self::trial_budget(m);
        let mut missions = HashMap::new();
        missions.insert(m.id.to_string(), MissionProgress { accepted: true, delivered: 0, completed: false });
        Profile {
            creative: false,
            money,
            rp,
            unlocked: parts::starting_parts(),
            missions,
            design: PlaneDesign::empty(),
            flights: 0,
            crashes: 0,
            contracts_done: 0,
            distance_km: 0.0,
            assist: true,
            spawn_airport: m.from.into(),
            explored: vec![1u8; MAP_TILES * MAP_TILES],
            trial: Some(m.id.to_string()),
        }
    }

    /// Reveals the tile under (x, z) and its neighbours. Returns true if anything new was seen.
    pub fn explore(&mut self, x: f32, z: f32) -> bool {
        if self.explored.len() != MAP_TILES * MAP_TILES {
            self.explored = default_explored();
        }
        let (tx, tz) = tile_of(x, z);
        let mut changed = false;
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                let (nx, nz) = (tx as i32 + dx, tz as i32 + dz);
                if nx >= 0 && nz >= 0 && (nx as usize) < MAP_TILES && (nz as usize) < MAP_TILES {
                    let i = nz as usize * MAP_TILES + nx as usize;
                    if self.explored[i] == 0 {
                        self.explored[i] = 1;
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    pub fn is_explored(&self, x: f32, z: f32) -> bool {
        let (tx, tz) = tile_of(x, z);
        self.explored.get(tz * MAP_TILES + tx).copied().unwrap_or(0) != 0
    }

    pub fn is_unlocked(&self, id: &str) -> bool {
        self.creative || self.unlocked.iter().any(|u| u == id)
    }

    pub fn can_afford(&self, cost: i64) -> bool {
        self.creative || self.money >= cost
    }

    pub fn spend(&mut self, cost: i64) {
        if !self.creative {
            self.money -= cost;
        }
    }

    pub fn refund(&mut self, cost: i64) {
        if !self.creative {
            self.money += cost;
        }
    }

    pub fn money_text(&self) -> String {
        if self.creative { "$ unlimited".to_string() } else { format!("$ {}", self.money) }
    }

    pub fn progress(&self, id: &str) -> MissionProgress {
        self.missions.get(id).cloned().unwrap_or_default()
    }

    pub fn progress_mut(&mut self, id: &str) -> &mut MissionProgress {
        self.missions.entry(id.to_string()).or_default()
    }

    pub fn mission_available(&self, id: &str) -> bool {
        let Some(m) = crate::missions::mission(id) else { return false };
        if m.contract {
            // Contracts open up once both ends are on your chart.
            let known = |aid: &str| crate::world::AIRPORTS.iter().find(|a| a.id == aid).map_or(false, |a| self.is_explored(a.pos.x, a.pos.y));
            return known(m.from) && known(m.to);
        }
        m.requires.iter().all(|r| self.progress(r).completed)
    }

    pub fn active_missions(&self) -> Vec<&'static crate::missions::MissionDef> {
        missions::all()
            .iter()
            .filter(|m| {
                let p = self.progress(m.id);
                p.accepted && !p.completed
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn completed_count(&self) -> usize {
        missions::all().iter().filter(|m| self.progress(m.id).completed).count()
    }
}

/// Per-platform save folder: ~/Library/Application Support/Wingwright on macOS,
/// %APPDATA%\Wingwright on Windows, $XDG_DATA_HOME/wingwright (or ~/.local/share) elsewhere.
pub fn save_dir() -> PathBuf {
    let dir = if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join("Library/Application Support/Wingwright")
    } else if cfg!(target_os = "windows") {
        let base = std::env::var("APPDATA").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join("Wingwright")
    } else {
        let base = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            format!("{home}/.local/share")
        });
        PathBuf::from(base).join("wingwright")
    };
    let _ = std::fs::create_dir_all(dir.join("designs"));
    dir
}

fn profile_path() -> PathBuf {
    save_dir().join("profile.json")
}

fn nosave() -> bool {
    std::env::var_os("WINGWRIGHT_NOSAVE").is_some()
}

pub fn profile_exists() -> bool {
    !nosave() && profile_path().exists()
}

pub fn load_profile() -> Option<Profile> {
    if nosave() {
        return None;
    }
    let text = std::fs::read_to_string(profile_path()).ok()?;
    match serde_json::from_str::<Profile>(&text) {
        Ok(mut p) => {
            p.migrate_explored();
            let dropped = p.design.prune_unknown();
            if dropped > 0 {
                warn!("profile design: dropped {dropped} parts that no longer exist");
            }
            Some(p)
        }
        Err(e) => {
            warn!("could not parse profile: {e}");
            None
        }
    }
}

pub fn save_profile(profile: &Profile) {
    if nosave() || profile.trial.is_some() {
        return;
    }
    match serde_json::to_string_pretty(profile) {
        Ok(text) => {
            if let Err(e) = std::fs::write(profile_path(), text) {
                warn!("could not save profile: {e}");
            }
        }
        Err(e) => warn!("could not serialize profile: {e}"),
    }
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() { "design".into() } else { s }
}

pub fn list_designs() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(save_dir().join("designs"))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension().map(|x| x == "json").unwrap_or(false) {
                        p.file_stem().map(|s| s.to_string_lossy().to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

pub fn save_design(name: &str, design: &PlaneDesign) {
    let mut d = design.clone();
    d.name = name.to_string();
    let path = save_dir().join("designs").join(format!("{}.json", sanitize(name)));
    if let Ok(text) = serde_json::to_string_pretty(&d) {
        let _ = std::fs::write(path, text);
    }
}

pub fn load_design(name: &str) -> Option<PlaneDesign> {
    let path = save_dir().join("designs").join(format!("{}.json", sanitize(name)));
    let text = std::fs::read_to_string(path).ok()?;
    let mut d: PlaneDesign = serde_json::from_str(&text).ok()?;
    d.prune_unknown();
    Some(d)
}

pub fn delete_design(name: &str) {
    let path = save_dir().join("designs").join(format!("{}.json", sanitize(name)));
    let _ = std::fs::remove_file(path);
}

// ---------------------------------------------------------------------------------------
// Test Pilot results

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrialResult {
    pub secs: f64,
    pub cost: i64,
    pub parts: String,
    pub when: String,
}

fn trials_path() -> PathBuf {
    save_dir().join("trials.json")
}

pub fn trial_results() -> HashMap<String, Vec<TrialResult>> {
    std::fs::read_to_string(trials_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Appends a run to trials.json (written even with WINGWRIGHT_NOSAVE: it is the point).
pub fn record_trial(id: &str, secs: f64, design: &PlaneDesign) {
    let mut all = trial_results();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for part in &design.parts {
        *counts.entry(part.id.as_str()).or_default() += 1;
    }
    let mut names: Vec<String> = counts.iter().map(|(id, n)| {
        let name = parts::part(id).map_or(*id, |p| p.name);
        if *n > 1 { format!("{n}x {name}") } else { name.to_string() }
    }).collect();
    names.sort();
    let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs().to_string()).unwrap_or_default();
    all.entry(id.to_string()).or_default().push(TrialResult { secs, cost: design.total_cost(), parts: names.join(", "), when });
    if let Ok(text) = serde_json::to_string_pretty(&all) {
        let _ = std::fs::write(trials_path(), text);
    }
}
