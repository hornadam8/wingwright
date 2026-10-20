//! The island world: procedural heightmap terrain, airports, scenery, lighting and the map.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::pbr::{CascadeShadowConfigBuilder, NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, Face, TextureDimension, TextureFormat};


use crate::meshes;
use crate::missions::CargoKind;

pub const WORLD_HALF: f32 = 12000.0;
/// Horizon haze (sRGB): the distance fog, the sky dome's horizon band and the clear colour
/// all use it so terrain fades into the same tone the sky meets it with.
pub const HAZE: [f32; 3] = [0.80, 0.85, 0.93];
/// Grid spacing. Big cells give the chunky low-poly facets of the reference game; the
/// collision height follows the same triangles, so what you see is what you land on.
pub const CELL: f32 = 25.0;
pub const N: usize = 961;
pub const CHUNK: usize = 40;
/// Near the camera the terrain is rebuilt at half the cell size with rugged detail relief
/// (the coarse 25 m mesh stays for the distance). Physics samples the fine surface.
pub const FINE: f32 = CELL * 0.5;
pub const FINE_RADIUS: f32 = 2600.0;
pub const FINE_DROP: f32 = 3400.0;
/// Distance from the world center where the out-of-bounds warning starts.
/// Half-size of the square you may fly in before the warning (the terrain ends at WORLD_HALF).
pub const WARN_RADIUS: f32 = 11500.0;
/// Distance beyond which the plane is torn apart.
pub const KILL_RADIUS: f32 = 12600.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biome {
    Temperate = 0,
    Snow = 1,
    Desert = 2,
    /// Palms, white sand, turquoise shallows.
    Tropical = 3,
    /// Dark basalt, rust slopes, black sand, almost bare.
    Volcanic = 4,
}

pub struct IslandDef {
    #[allow(dead_code)]
    pub name: &'static str,
    pub center: Vec2,
    pub radius: f32,
    pub biome: Biome,
    pub amp: f32,
    pub freq: f32,
    pub seed: u32,
}

pub static ISLANDS: &[IslandDef] = &[
    // Home cluster (the original 16 km world sits in the middle of the 24 km map).
    IslandDef { name: "Verdant Isle", center: Vec2::new(0.0, 0.0), radius: 2900.0, biome: Biome::Temperate, amp: 150.0, freq: 0.0007, seed: 11 },
    IslandDef { name: "Frostholm", center: Vec2::new(-1500.0, -5200.0), radius: 1800.0, biome: Biome::Snow, amp: 300.0, freq: 0.0011, seed: 23 },
    IslandDef { name: "Dunmar", center: Vec2::new(4800.0, 4200.0), radius: 2000.0, biome: Biome::Desert, amp: 90.0, freq: 0.0007, seed: 37 },
    IslandDef { name: "Hermit Isle", center: Vec2::new(5200.0, -3000.0), radius: 450.0, biome: Biome::Temperate, amp: 25.0, freq: 0.002, seed: 41 },
    IslandDef { name: "Gull Rock", center: Vec2::new(-5000.0, 1500.0), radius: 520.0, biome: Biome::Temperate, amp: 45.0, freq: 0.002, seed: 53 },
    IslandDef { name: "Wreck Rock", center: Vec2::new(2600.0, -4200.0), radius: 380.0, biome: Biome::Temperate, amp: 15.0, freq: 0.002, seed: 67 },
    // Outer ring (2.0): long hauls, new biomes, new trades.
    IslandDef { name: "Palm Cay", center: Vec2::new(8600.0, -2400.0), radius: 1400.0, biome: Biome::Tropical, amp: 60.0, freq: 0.0012, seed: 71 },
    IslandDef { name: "Coral Atoll", center: Vec2::new(10600.0, -300.0), radius: 650.0, biome: Biome::Tropical, amp: 12.0, freq: 0.002, seed: 73 },
    IslandDef { name: "Blackpeak", center: Vec2::new(-8200.0, 5600.0), radius: 1600.0, biome: Biome::Volcanic, amp: 420.0, freq: 0.0009, seed: 79 },
    IslandDef { name: "Cape Wolds", center: Vec2::new(7000.0, -8600.0), radius: 2300.0, biome: Biome::Temperate, amp: 210.0, freq: 0.0008, seed: 83 },
    IslandDef { name: "Marsh Delta", center: Vec2::new(-9000.0, -3300.0), radius: 1500.0, biome: Biome::Temperate, amp: 22.0, freq: 0.0015, seed: 89 },
    IslandDef { name: "Ironholm", center: Vec2::new(2600.0, 9000.0), radius: 1300.0, biome: Biome::Temperate, amp: 280.0, freq: 0.001, seed: 97 },
    IslandDef { name: "Windward Isle", center: Vec2::new(-3600.0, 9400.0), radius: 750.0, biome: Biome::Tropical, amp: 40.0, freq: 0.0016, seed: 101 },
    IslandDef { name: "Leeward Isle", center: Vec2::new(-1500.0, 10500.0), radius: 520.0, biome: Biome::Tropical, amp: 30.0, freq: 0.002, seed: 103 },
    // Hard landings (2.0): a city hemmed in by towers, a gorge, a mesa top.
    IslandDef { name: "Kingsport", center: Vec2::new(9200.0, 6500.0), radius: 1500.0, biome: Biome::Temperate, amp: 40.0, freq: 0.001, seed: 107 },
    IslandDef { name: "Redgorge", center: Vec2::new(-7800.0, -8200.0), radius: 1900.0, biome: Biome::Desert, amp: 240.0, freq: 0.0009, seed: 109 },
    IslandDef { name: "Skyreach", center: Vec2::new(-8600.0, 9600.0), radius: 900.0, biome: Biome::Temperate, amp: 80.0, freq: 0.0012, seed: 113 },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AirportKind {
    Home,
    Town,
    Farm,
    Hospital,
    Lighthouse,
    Base,
    Outpost,
    Oasis,
    Hermit,
    Prison,
    Wreck,
    /// 2.0 places
    Resort,
    FishingVillage,
    Research,
    LumberCamp,
    Village,
    Mine,
    /// 2.0 hard landings: towers on the approach, a strip in a gorge, a short strip on a mesa.
    City,
    Canyon,
    Mesa,
}

pub struct AirportDef {
    pub id: &'static str,
    pub name: &'static str,
    pub pos: Vec2,
    /// Runway direction in degrees clockwise from north.
    pub heading_deg: f32,
    pub length: f32,
    pub width: f32,
    pub kind: AirportKind,
    /// Fuel price per liter.
    pub fuel_price: f32,
    /// Goods this place ships out (missions pick up here) ...
    pub produces: &'static [CargoKind],
    /// ... and goods it asks for (missions deliver here).
    pub needs: &'static [CargoKind],
}

pub static AIRPORTS: &[AirportDef] = &[
    AirportDef { id: "harbor", name: "Harbor Field", pos: Vec2::new(200.0, 600.0), heading_deg: 0.0, length: 800.0, width: 30.0, kind: AirportKind::Home, fuel_price: 0.0, produces: &[CargoKind::Supplies, CargoKind::FuelDrums, CargoKind::Machinery, CargoKind::Passengers], needs: &[CargoKind::Mail, CargoKind::Apples, CargoKind::Fish, CargoKind::Scrap, CargoKind::Ore] },
    AirportDef { id: "meadowvale", name: "Meadowvale Airport", pos: Vec2::new(-1800.0, -300.0), heading_deg: 90.0, length: 600.0, width: 30.0, kind: AirportKind::Town, fuel_price: 0.0, produces: &[CargoKind::Mail, CargoKind::Supplies, CargoKind::Glassware, CargoKind::Passengers], needs: &[CargoKind::Supplies, CargoKind::Lumber, CargoKind::Apples, CargoKind::Medicine] },
    AirportDef { id: "northwood", name: "Northwood Farm Strip", pos: Vec2::new(1500.0, -1500.0), heading_deg: 45.0, length: 450.0, width: 22.0, kind: AirportKind::Farm, fuel_price: 0.0, produces: &[CargoKind::Apples, CargoKind::Supplies, CargoKind::Lumber], needs: &[CargoKind::Machinery, CargoKind::FuelDrums] },
    AirportDef { id: "stmarys", name: "St. Mary's Hospital", pos: Vec2::new(-600.0, 1900.0), heading_deg: 120.0, length: 450.0, width: 22.0, kind: AirportKind::Hospital, fuel_price: 0.0, produces: &[CargoKind::Medicine], needs: &[CargoKind::Glassware, CargoKind::Supplies, CargoKind::Passengers] },
    AirportDef { id: "lighthouse", name: "Lighthouse Point", pos: Vec2::new(2300.0, 400.0), heading_deg: 0.0, length: 400.0, width: 22.0, kind: AirportKind::Lighthouse, fuel_price: 0.0, produces: &[CargoKind::Supplies], needs: &[CargoKind::Mail, CargoKind::Medicine, CargoKind::Lumber, CargoKind::Passengers] },
    AirportDef { id: "frostholm", name: "Frostholm Base", pos: Vec2::new(-1700.0, -4800.0), heading_deg: 30.0, length: 700.0, width: 30.0, kind: AirportKind::Base, fuel_price: 0.0, produces: &[CargoKind::Medicine, CargoKind::Glassware], needs: &[CargoKind::Supplies, CargoKind::Machinery, CargoKind::FuelDrums] },
    AirportDef { id: "glacier", name: "Glacier Outpost", pos: Vec2::new(-900.0, -5900.0), heading_deg: 100.0, length: 400.0, width: 22.0, kind: AirportKind::Outpost, fuel_price: 0.0, produces: &[CargoKind::Fish], needs: &[CargoKind::Medicine, CargoKind::Supplies] },
    AirportDef { id: "oasis", name: "Oasis Strip", pos: Vec2::new(4200.0, 3600.0), heading_deg: 60.0, length: 500.0, width: 25.0, kind: AirportKind::Oasis, fuel_price: 0.0, produces: &[CargoKind::Water], needs: &[CargoKind::FuelDrums, CargoKind::Supplies] },
    AirportDef { id: "dunmar", name: "Dunmar Airbase", pos: Vec2::new(5400.0, 4900.0), heading_deg: 150.0, length: 900.0, width: 40.0, kind: AirportKind::Base, fuel_price: 0.0, produces: &[CargoKind::FuelDrums], needs: &[CargoKind::Water, CargoKind::Supplies, CargoKind::Glassware, CargoKind::Passengers] },
    AirportDef { id: "hermit", name: "Hermit Isle", pos: Vec2::new(5200.0, -3000.0), heading_deg: 20.0, length: 320.0, width: 18.0, kind: AirportKind::Hermit, fuel_price: 0.0, produces: &[], needs: &[CargoKind::Supplies] },
    AirportDef { id: "gullrock", name: "Gull Rock Prison", pos: Vec2::new(-5000.0, 1500.0), heading_deg: 80.0, length: 400.0, width: 22.0, kind: AirportKind::Prison, fuel_price: 0.0, produces: &[CargoKind::Passengers], needs: &[CargoKind::Passengers, CargoKind::Supplies] },
    AirportDef { id: "wreck", name: "Wreck Rock", pos: Vec2::new(2600.0, -4200.0), heading_deg: 140.0, length: 300.0, width: 18.0, kind: AirportKind::Wreck, fuel_price: 0.0, produces: &[CargoKind::Scrap], needs: &[] },
    // ---- 2.0 outer ring
    AirportDef { id: "palmcay", name: "Palm Cay Resort", pos: Vec2::new(8400.0, -2600.0), heading_deg: 70.0, length: 550.0, width: 25.0, kind: AirportKind::Resort, fuel_price: 0.0, produces: &[CargoKind::Passengers], needs: &[CargoKind::Supplies, CargoKind::Apples, CargoKind::Passengers, CargoKind::Mail, CargoKind::Fish] },
    AirportDef { id: "atoll", name: "Coral Atoll Fishery", pos: Vec2::new(10500.0, -300.0), heading_deg: 160.0, length: 380.0, width: 20.0, kind: AirportKind::FishingVillage, fuel_price: 0.0, produces: &[CargoKind::Fish], needs: &[CargoKind::FuelDrums, CargoKind::Supplies, CargoKind::Medicine] },
    AirportDef { id: "blackpeak", name: "Blackpeak Observatory", pos: Vec2::new(-7700.0, 4900.0), heading_deg: 40.0, length: 420.0, width: 22.0, kind: AirportKind::Research, fuel_price: 0.0, produces: &[CargoKind::Glassware], needs: &[CargoKind::Machinery, CargoKind::Supplies, CargoKind::Water, CargoKind::Glassware] },
    AirportDef { id: "wolds", name: "Cape Wolds Aerodrome", pos: Vec2::new(6600.0, -8200.0), heading_deg: 110.0, length: 750.0, width: 30.0, kind: AirportKind::Town, fuel_price: 0.0, produces: &[CargoKind::Mail, CargoKind::Supplies, CargoKind::Passengers, CargoKind::Machinery], needs: &[CargoKind::Lumber, CargoKind::Fish, CargoKind::Ore, CargoKind::Apples] },
    AirportDef { id: "sawmill", name: "Wolds Lumber Camp", pos: Vec2::new(8100.0, -9300.0), heading_deg: 20.0, length: 380.0, width: 20.0, kind: AirportKind::LumberCamp, fuel_price: 0.0, produces: &[CargoKind::Lumber], needs: &[CargoKind::Medicine, CargoKind::Supplies, CargoKind::FuelDrums, CargoKind::Machinery] },
    AirportDef { id: "delta", name: "Marsh Delta Village", pos: Vec2::new(-8800.0, -3100.0), heading_deg: 85.0, length: 420.0, width: 22.0, kind: AirportKind::Village, fuel_price: 0.0, produces: &[CargoKind::Fish, CargoKind::Water], needs: &[CargoKind::Medicine, CargoKind::Lumber, CargoKind::Supplies, CargoKind::Mail] },
    AirportDef { id: "ironholm", name: "Ironholm Mine", pos: Vec2::new(2300.0, 8700.0), heading_deg: 150.0, length: 500.0, width: 25.0, kind: AirportKind::Mine, fuel_price: 0.0, produces: &[CargoKind::Ore, CargoKind::Scrap], needs: &[CargoKind::Machinery, CargoKind::FuelDrums, CargoKind::Supplies, CargoKind::Water, CargoKind::Lumber] },
    AirportDef { id: "windward", name: "Windward Village", pos: Vec2::new(-3500.0, 9300.0), heading_deg: 60.0, length: 400.0, width: 22.0, kind: AirportKind::Village, fuel_price: 0.0, produces: &[CargoKind::Fish, CargoKind::Passengers], needs: &[CargoKind::Supplies, CargoKind::Mail, CargoKind::Medicine, CargoKind::Water, CargoKind::Passengers] },
    // Hard landings.
    AirportDef { id: "kingsport", name: "Kingsport City", pos: Vec2::new(9200.0, 6500.0), heading_deg: 30.0, length: 520.0, width: 30.0, kind: AirportKind::City, fuel_price: 0.0, produces: &[CargoKind::Mail, CargoKind::Machinery, CargoKind::Medicine, CargoKind::Glassware, CargoKind::Passengers], needs: &[CargoKind::Fish, CargoKind::Apples, CargoKind::Lumber, CargoKind::Ore, CargoKind::Supplies, CargoKind::Passengers, CargoKind::Glassware] },
    AirportDef { id: "ravine", name: "Redgorge Quarry", pos: Vec2::new(-7800.0, -8200.0), heading_deg: 120.0, length: 560.0, width: 26.0, kind: AirportKind::Canyon, fuel_price: 0.0, produces: &[CargoKind::Ore, CargoKind::Scrap, CargoKind::Passengers], needs: &[CargoKind::FuelDrums, CargoKind::Machinery, CargoKind::Supplies, CargoKind::Water, CargoKind::Medicine] },
    AirportDef { id: "mesa", name: "Skyreach Relay", pos: Vec2::new(-8600.0, 9600.0), heading_deg: 0.0, length: 460.0, width: 22.0, kind: AirportKind::Mesa, fuel_price: 0.0, produces: &[CargoKind::Mail], needs: &[CargoKind::Supplies, CargoKind::Machinery, CargoKind::Medicine, CargoKind::Water, CargoKind::FuelDrums] },
];

#[derive(Clone)]
pub struct Airport {
    pub def: &'static AirportDef,
    pub elevation: f32,
    /// Unit vector along the runway (x, z).
    pub dir: Vec2,
}

impl Airport {
    pub fn id(&self) -> &'static str {
        self.def.id
    }

    pub fn name(&self) -> &'static str {
        self.def.name
    }

    pub fn is_home(&self) -> bool {
        self.def.kind == AirportKind::Home
    }

    /// Right-hand perpendicular of the runway direction.
    pub fn right(&self) -> Vec2 {
        Vec2::new(-self.dir.y, self.dir.x)
    }

    pub fn center(&self) -> Vec3 {
        Vec3::new(self.def.pos.x, self.elevation, self.def.pos.y)
    }

    /// Runway-local coordinates (u along the runway, v across it).
    pub fn local(&self, p: Vec2) -> Vec2 {
        let d = p - self.def.pos;
        Vec2::new(d.dot(self.dir), d.dot(self.right()))
    }

    pub fn world(&self, u: f32, v: f32) -> Vec2 {
        self.def.pos + self.dir * u + self.right() * v
    }

    pub fn world3(&self, u: f32, v: f32, y: f32) -> Vec3 {
        let w = self.world(u, v);
        Vec3::new(w.x, self.elevation + y, w.y)
    }

    /// Inside the runway plus an apron margin.
    pub fn in_zone(&self, p: Vec3) -> bool {
        let l = self.local(Vec2::new(p.x, p.z));
        l.x.abs() <= self.def.length * 0.5 + 120.0 && l.y.abs() <= self.def.width * 0.5 + 140.0
    }

    /// On the paved runway surface itself.
    pub fn on_runway(&self, p: Vec3) -> bool {
        let l = self.local(Vec2::new(p.x, p.z));
        l.x.abs() <= self.def.length * 0.5 && l.y.abs() <= self.def.width * 0.5
    }

    pub fn yaw(&self) -> f32 {
        yaw_from_forward(self.dir)
    }

    /// Where a plane is placed for takeoff: near the start of the runway, facing along it.
    pub fn spawn_point(&self) -> Vec3 {
        self.world3(-self.def.length * 0.5 + 60.0, 0.0, 0.0)
    }

    pub fn hangar_center(&self) -> Vec3 {
        self.world3(-self.def.length * 0.5 + 140.0, self.def.width * 0.5 + 70.0, 0.0)
    }

    /// Facing of a plane parked inside the hangar (toward the runway).
    pub fn hangar_yaw(&self) -> f32 {
        yaw_from_forward(-self.right())
    }

    pub fn dist_xz(&self, p: Vec3) -> f32 {
        (Vec2::new(p.x, p.z) - self.def.pos).length()
    }
}

/// Bevy yaw (rotation about +Y) that points the -Z forward axis along `f` (x, z).
pub fn yaw_from_forward(f: Vec2) -> f32 {
    (-f.x).atan2(-f.y)
}

/// Heading in degrees clockwise from north for a forward vector (x, z).
pub fn heading_deg(f: Vec3) -> f32 {
    let h = f.x.atan2(-f.z).to_degrees();
    let h = if h < 0.0 { h + 360.0 } else { h };
    if h >= 359.5 { 0.0 } else { h }
}

// ---------------------------------------------------------------------------------------
// noise

fn hash01(ix: i32, iz: i32, seed: u32) -> f32 {
    let mut h = (ix as u32)
        .wrapping_mul(374761393)
        ^ (iz as u32).wrapping_mul(668265263)
        ^ seed.wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    let fx = x.floor();
    let fz = z.floor();
    let ix = fx as i32;
    let iz = fz as i32;
    let tx = smooth(x - fx);
    let tz = smooth(z - fz);
    let a = hash01(ix, iz, seed);
    let b = hash01(ix + 1, iz, seed);
    let c = hash01(ix, iz + 1, seed);
    let d = hash01(ix + 1, iz + 1, seed);
    let ab = a + (b - a) * tx;
    let cd = c + (d - c) * tx;
    ab + (cd - ab) * tz
}

/// Fractal noise in roughly [0, 1].
pub fn fbm(x: f32, z: f32, octaves: u32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut fx = x;
    let mut fz = z;
    let mut norm = 0.0;
    for o in 0..octaves {
        sum += amp * value_noise(fx + 13.7 * o as f32, fz - 7.1 * o as f32, seed + o);
        norm += amp;
        amp *= 0.5;
        fx *= 2.03;
        fz *= 1.97;
    }
    sum / norm
}

/// `WINGWRIGHT_DEBUG=nodetail,noshadow` style toggles for isolating rendering artefacts in
/// scripted runs (read once).
pub fn debug_flag(name: &str) -> bool {
    static FLAGS: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    FLAGS.get_or_init(|| std::env::var("WINGWRIGHT_DEBUG").unwrap_or_default()).split(',').any(|f| f.trim() == name)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Deterministic xorshift RNG for scenery placement and effects.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
}

// ---------------------------------------------------------------------------------------
// terrain

#[derive(Resource)]
pub struct Terrain {
    heights: Vec<f32>,
    biomes: Vec<u8>,
    pub airports: Vec<Airport>,
    /// Dirt tracks between airports that share an island, painted into the terrain.
    pub roads: Vec<(Vec2, Vec2)>,
    /// Crop fields beside farms, towns and villages: (centre, half size, yaw, crop index).
    pub fields: Vec<(Vec2, Vec2, f32, u8)>,
    /// Settlement pads (centre, half size, yaw) levelled to their airport: no trees here.
    pub pads: Vec<(Vec2, Vec2, f32)>,
}

/// A field or road paints the terrain: distance from `p` to the segment `a`-`b`.
fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

impl Terrain {
    pub fn generate() -> Self {
        let mut airports: Vec<Airport> = AIRPORTS
            .iter()
            .map(|def| {
                let h = def.heading_deg.to_radians();
                Airport { def, elevation: 0.0, dir: Vec2::new(h.sin(), -h.cos()) }
            })
            .collect();
        // Airport elevations come from the raw terrain at their centers, kept above the sea.
        for a in airports.iter_mut() {
            let raw = raw_height(a.def.pos.x, a.def.pos.y);
            a.elevation = match a.def.kind {
                AirportKind::Mesa => 190.0,
                AirportKind::Canyon => raw.clamp(6.0, 30.0).round(),
                _ => raw.clamp(6.0, 60.0).round(),
            };
        }
        // Settlement plans: streets are painted like roads, pads levelled like the runway.
        let plans: Vec<(f32, crate::settle::Plan)> = airports.iter().map(|a| (a.elevation, crate::settle::plan(a))).collect();
        let pads: Vec<(Vec2, Vec2, f32)> = plans.iter().flat_map(|(_, p)| p.pads.iter().cloned()).collect();
        let mut heights = vec![0.0f32; N * N];
        let mut biomes = vec![0u8; N * N];
        for j in 0..N {
            let z = -WORLD_HALF + j as f32 * CELL;
            for i in 0..N {
                let x = -WORLD_HALF + i as f32 * CELL;
                let mut h = raw_height(x, z);
                // Flatten airports.
                for a in &airports {
                    let l = a.local(Vec2::new(x, z));
                    let hl = a.def.length * 0.5 + 70.0;
                    let hw = a.def.width * 0.5 + 60.0;
                    let dx = (l.x.abs() - hl).max(0.0);
                    let dz = (l.y.abs() - hw).max(0.0);
                    let outside = (dx * dx + dz * dz).sqrt();
                    let w = 1.0 - smoothstep(0.0, 160.0, outside);
                    if w > 0.0 {
                        h = h + (a.elevation - h) * w;
                    }
                }
                // Level the settlement pads (land only: the sea stays where it is).
                if h > 1.0 {
                    for (elev, plan) in &plans {
                        for (c, half, yaw) in &plan.pads {
                            let d = Vec2::new(x, z) - *c;
                            let (sy, cy) = yaw.sin_cos();
                            let u = d.x * cy - d.y * sy;
                            let v = d.x * sy + d.y * cy;
                            let dx = (u.abs() - half.x).max(0.0);
                            let dz = (v.abs() - half.y).max(0.0);
                            let outside = (dx * dx + dz * dz).sqrt();
                            let w = 1.0 - smoothstep(0.0, 110.0, outside);
                            if w > 0.0 {
                                h = h + (elev - h) * w;
                            }
                        }
                    }
                }
                // Hard-landing terrain: the mesa top with cliffs all round, the gorge walls.
                for a in &airports {
                    match a.def.kind {
                        AirportKind::Mesa => {
                            let r = (Vec2::new(x, z) - a.def.pos).length();
                            let w = 1.0 - smoothstep(430.0, 560.0, r);
                            if w > 0.0 {
                                let rim = 1.0 + 0.06 * (fbm(x * 0.01, z * 0.01, 2, 91) - 0.5);
                                h = h.max(h + (a.elevation * rim - h) * w);
                            }
                        }
                        AirportKind::Canyon => {
                            // The gorge runs straight past the strip, then bends (opposite ways
                            // at the two ends); its walls wander in and out and vary in height.
                            let l = a.local(Vec2::new(x, z));
                            let half = a.def.length * 0.5;
                            let bend = 300.0 * smoothstep(half + 80.0, half + 1000.0, l.x.abs()) * (l.x / 640.0).sin();
                            let vv = l.y - bend;
                            let wobble = 120.0 * (fbm(x * 0.0016 + 5.0, z * 0.0016 - 3.0, 2, 78) - 0.5);
                            let inner = 150.0 + wobble;
                            let across = smoothstep(inner, inner + 170.0, vv.abs()) * (1.0 - smoothstep(700.0, 1050.0, vv.abs()));
                            let along = 1.0 - smoothstep(half + 1400.0, half + 2100.0, l.x.abs());
                            let rugged = 0.6 + 0.8 * fbm(x * 0.003, z * 0.003, 3, 77);
                            h += 150.0 * across * along * rugged;
                        }
                        _ => {}
                    }
                }
                heights[j * N + i] = h;
                biomes[j * N + i] = biome_at(x, z) as u8;
            }
        }
        // Roads: straight tracks between airports whose connecting line stays on land.
        let mut roads = Vec::new();
        for (i, a) in airports.iter().enumerate() {
            for b in airports.iter().skip(i + 1) {
                let d = (a.def.pos - b.def.pos).length();
                if d > 5200.0 {
                    continue;
                }
                let on_land = (1..12).all(|k| {
                    let p = a.def.pos.lerp(b.def.pos, k as f32 / 12.0);
                    raw_height(p.x, p.y) > 1.5
                });
                if on_land {
                    roads.push((a.def.pos, b.def.pos));
                }
            }
        }
        for (_, plan) in &plans {
            roads.extend(plan.streets.iter().cloned());
        }
        // Fields: a patchwork beside farms, towns and villages, on the apron side.
        let mut fields = Vec::new();
        let mut frng = Rng(0xF1E1D5_0000_1234);
        for a in &airports {
            let (n, v0) = match a.def.kind {
                AirportKind::Farm => (9, 260.0),
                AirportKind::Village => (5, 260.0),
                AirportKind::Home => (3, 400.0),
                _ => (0, 0.0),
            };
            for k in 0..n {
                let u = -a.def.length * 0.4 + (k % 3) as f32 * 150.0 + frng.range(-20.0, 20.0);
                let v = a.def.width * 0.5 + v0 + (k / 3) as f32 * 120.0;
                let c = a.world(u, v);
                if raw_height(c.x, c.y) < 3.0 {
                    continue;
                }
                fields.push((c, Vec2::new(frng.range(55.0, 70.0), frng.range(40.0, 55.0)), a.yaw(), (k % 4) as u8));
            }
        }
        Terrain { heights, biomes, airports, roads, fields, pads }
    }

    fn sample(&self, i: usize, j: usize) -> f32 {
        self.heights[j.min(N - 1) * N + i.min(N - 1)]
    }

    /// Ground height at world (x, z) on the fine surface the near terrain draws: the coarse
    /// triangles plus `detail`, interpolated on the 12.5 m triangles of `build_chunk(fine)`.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let n = 2 * (N - 1);
        let fx = ((x + WORLD_HALF) / FINE).clamp(0.0, n as f32 - 0.001);
        let fz = ((z + WORLD_HALF) / FINE).clamp(0.0, n as f32 - 0.001);
        let i = fx.floor() as usize;
        let j = fz.floor() as usize;
        let tx = fx - i as f32;
        let tz = fz - j as f32;
        let a = self.fine_sample(i, j);
        let b = self.fine_sample(i + 1, j);
        let c = self.fine_sample(i, j + 1);
        let d = self.fine_sample(i + 1, j + 1);
        if self.diagonal_bc(i / 2, j / 2) {
            if tx + tz < 1.0 {
                a + (b - a) * tx + (c - a) * tz
            } else {
                d + (c - d) * (1.0 - tx) + (b - d) * (1.0 - tz)
            }
        } else if tx > tz {
            a + (b - a) * tx + (d - b) * tz
        } else {
            a + (c - a) * tz + (d - c) * tx
        }
    }

    /// The diagonal rule for the coarse cell with corner (i, j); see `diagonal_bc_h`.
    pub fn diagonal_bc(&self, i: usize, j: usize) -> bool {
        let (i, j) = (i.min(N - 2), j.min(N - 2));
        diagonal_bc_h(self.sample(i, j), self.sample(i + 1, j), self.sample(i, j + 1), self.sample(i + 1, j + 1))
    }

    /// Height of the fine grid point (i, j) (12.5 m spacing): coarse surface plus detail.
    pub fn fine_sample(&self, i: usize, j: usize) -> f32 {
        let x = -WORLD_HALF + i as f32 * FINE;
        let z = -WORLD_HALF + j as f32 * FINE;
        self.coarse_height(x, z) + self.detail(x, z)
    }

    /// Small rugged relief: strongest on slopes, none on the flats around airports and on
    /// the settlement pads, so runways and roads sit flush.
    pub fn detail(&self, x: f32, z: f32) -> f32 {
        let h = self.coarse_height(x, z);
        if h < 0.8 || debug_flag("nodetail") {
            return 0.0;
        }
        let dx = self.coarse_height(x + 12.0, z) - self.coarse_height(x - 12.0, z);
        let dz = self.coarse_height(x, z + 12.0) - self.coarse_height(x, z - 12.0);
        let slope = (dx * dx + dz * dz).sqrt() / 24.0;
        let mut amp = (0.35 + 1.8 * smoothstep(0.06, 0.45, slope)) * smoothstep(0.8, 4.0, h);
        let p = Vec2::new(x, z);
        for a in &self.airports {
            let l = a.local(p);
            let ddx = (l.x.abs() - (a.def.length * 0.5 + 120.0)).max(0.0);
            let ddz = (l.y.abs() - (a.def.width * 0.5 + 140.0)).max(0.0);
            amp *= smoothstep(0.0, 90.0, (ddx * ddx + ddz * ddz).sqrt());
            if amp <= 0.0 {
                return 0.0;
            }
        }
        if self.on_pad(x, z, 0.0) {
            return 0.0;
        }
        if self.on_pad(x, z, 70.0) {
            amp *= 0.35;
        }
        // Fade out along coarse chunk borders so fine and coarse chunks meet without cracks.
        let span = CHUNK as f32 * CELL;
        let bx = ((x + WORLD_HALF) % span).min(span - (x + WORLD_HALF) % span);
        let bz = ((z + WORLD_HALF) % span).min(span - (z + WORLD_HALF) % span);
        amp *= smoothstep(0.0, 25.0, bx.min(bz));
        amp * 2.0 * (fbm(x * 0.018 + 7.0, z * 0.018 - 3.0, 2, 313) - 0.5)
    }

    /// Height on the coarse 25 m triangles (what the distant chunks draw).
    pub fn coarse_height(&self, x: f32, z: f32) -> f32 {
        let fx = ((x + WORLD_HALF) / CELL).clamp(0.0, (N - 1) as f32 - 0.001);
        let fz = ((z + WORLD_HALF) / CELL).clamp(0.0, (N - 1) as f32 - 0.001);
        let i = fx.floor() as usize;
        let j = fz.floor() as usize;
        let tx = fx - i as f32;
        let tz = fz - j as f32;
        let a = self.sample(i, j);
        let b = self.sample(i + 1, j);
        let c = self.sample(i, j + 1);
        let d = self.sample(i + 1, j + 1);
        if diagonal_bc_h(a, b, c, d) {
            if tx + tz < 1.0 {
                a + (b - a) * tx + (c - a) * tz
            } else {
                d + (c - d) * (1.0 - tx) + (b - d) * (1.0 - tz)
            }
        } else if tx > tz {
            a + (b - a) * tx + (d - b) * tz
        } else {
            a + (c - a) * tz + (d - c) * tx
        }
    }

    pub fn biome(&self, x: f32, z: f32) -> Biome {
        let i = (((x + WORLD_HALF) / CELL).round() as isize).clamp(0, N as isize - 1) as usize;
        let j = (((z + WORLD_HALF) / CELL).round() as isize).clamp(0, N as isize - 1) as usize;
        match self.biomes[j * N + i] {
            1 => Biome::Snow,
            2 => Biome::Desert,
            3 => Biome::Tropical,
            4 => Biome::Volcanic,
            _ => Biome::Temperate,
        }
    }

    pub fn airport(&self, id: &str) -> &Airport {
        self.airports.iter().find(|a| a.def.id == id).expect("unknown airport")
    }

    /// Inside a settlement pad (plus a margin): keep wild trees and rocks out.
    pub fn on_pad(&self, x: f32, z: f32, margin: f32) -> bool {
        self.pads.iter().any(|(c, half, yaw)| {
            let d = Vec2::new(x, z) - *c;
            let (sy, cy) = yaw.sin_cos();
            let u = d.x * cy - d.y * sy;
            let v = d.x * sy + d.y * cy;
            u.abs() < half.x + margin && v.abs() < half.y + margin
        })
    }

    pub fn home(&self) -> &Airport {
        self.airports.iter().find(|a| a.is_home()).expect("no home airport")
    }

    pub fn nearest_airport(&self, p: Vec3) -> (&Airport, f32) {
        let mut best = &self.airports[0];
        let mut bd = f32::MAX;
        for a in &self.airports {
            let d = a.dist_xz(p);
            if d < bd {
                bd = d;
                best = a;
            }
        }
        (best, bd)
    }

    /// Airport whose zone contains `p`, if any.
    pub fn airport_at(&self, p: Vec3) -> Option<&Airport> {
        self.airports.iter().find(|a| a.in_zone(p))
    }

    /// RGBA pixels of a top-down map, `size` x `size`, north up.
    pub fn map_pixels(&self, size: usize) -> Vec<[u8; 4]> {
        let mut px = Vec::with_capacity(size * size);
        for j in 0..size {
            let z = -WORLD_HALF + (j as f32 + 0.5) / size as f32 * WORLD_HALF * 2.0;
            for i in 0..size {
                let x = -WORLD_HALF + (i as f32 + 0.5) / size as f32 * WORLD_HALF * 2.0;
                let h = self.height(x, z);
                let b = self.biome(x, z);
                // Same bands as the terrain, plus light contour rings every 40 m like a chart.
                let c: [f32; 3] = if h < -6.0 {
                    [0.24, 0.42, 0.70]
                } else if h < 0.5 {
                    [0.34, 0.54, 0.76]
                } else if h < 6.0 {
                    [0.83, 0.70, 0.45]
                } else {
                    let (low, high) = match b {
                        Biome::Temperate => ([0.50, 0.58, 0.30], [0.78, 0.74, 0.50]),
                        Biome::Snow => ([0.72, 0.76, 0.72], [0.94, 0.95, 0.97]),
                        Biome::Desert => ([0.87, 0.72, 0.43], [0.94, 0.86, 0.66]),
                        Biome::Tropical => ([0.36, 0.60, 0.28], [0.80, 0.78, 0.52]),
                        Biome::Volcanic => ([0.30, 0.28, 0.26], [0.55, 0.40, 0.30]),
                    };
                    let band = ((h / 40.0).floor() * 0.16).min(0.8);
                    let ring = if (h / 40.0).fract() < 0.12 { 0.08 } else { 0.0 };
                    let t = (band + ring).min(1.0);
                    let mut c = mix3(low, high, t);
                    // Fields and roads show on the chart too.
                    let p2 = Vec2::new(x, z);
                    if let Some(f) = self.fields.iter().find(|(fc, half, yaw, _)| {
                        let d = p2 - *fc;
                        let (sy, cy) = yaw.sin_cos();
                        (d.x * cy - d.y * sy).abs() < half.x && (d.x * sy + d.y * cy).abs() < half.y
                    }) {
                        c = match f.3 {
                            0 => [0.76, 0.66, 0.34],
                            1 => [0.44, 0.58, 0.26],
                            2 => [0.66, 0.52, 0.34],
                            _ => [0.58, 0.64, 0.30],
                        };
                    }
                    if self.roads.iter().any(|(a, b)| seg_dist(p2, *a, *b) < 14.0) {
                        c = [0.62, 0.52, 0.38];
                    }
                    c
                };
                px.push([(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]);
            }
        }
        px
    }
}

fn island_t(island: &IslandDef, x: f32, z: f32) -> f32 {
    let d = Vec2::new(x, z) - island.center;
    let dist = d.length();
    let ang = d.y.atan2(d.x);
    // Warp the coastline: angular noise breaks the disc, a stronger positional fbm cuts
    // bays and headlands into it (the reference coasts are ragged, fjord-like).
    let n = value_noise(ang.cos() * 2.5 + 7.0, ang.sin() * 2.5 + 3.0, island.seed) * 0.45
        + fbm(x * 0.0021 + 5.0, z * 0.0021 - 3.0, 4, island.seed + 100) * 0.55;
    let r_eff = island.radius * (0.52 + 0.85 * n);
    1.0 - dist / r_eff
}

/// Which diagonal splits the grid cell (i, j): true = b-c (from +x to +z corner).
/// Alternating them hides the grid direction in the facets.
/// Which way the coarse cell with corners a (i, j), b (i+1, j), c (i, j+1), d (i+1, j+1)
/// is split: along b-c when that diagonal has the smaller height difference, else along
/// a-d. Following the flatter diagonal lets the triangles hug ridges and hollows; the old
/// alternating split lit every other cell differently on curved ground (a checkerboard,
/// obvious once the near ground was flat-shaded). Fine cells split like their coarse
/// parent so every fine triangle stays inside one coarse plane. Shared by `height`,
/// `coarse_height` and the chunk meshes so collision and picture agree.
fn diagonal_bc_h(a: f32, b: f32, c: f32, d: f32) -> bool {
    (b - c).abs() < (a - d).abs()
}

/// Distance from `p` to a runway's centre line extended 1.2 km past both ends: the
/// approach corridor, kept free of hills so every strip can be reached on a shallow glide.
fn corridor_dist(a: &AirportDef, p: Vec2) -> f32 {
    let hd = a.heading_deg.to_radians();
    let dir = Vec2::new(hd.sin(), -hd.cos());
    let d = p - a.pos;
    let along = d.dot(dir);
    let across = d.dot(Vec2::new(-dir.y, dir.x));
    let half = a.length * 0.5 + 2200.0;
    let dx = (along.abs() - half).max(0.0);
    (dx * dx + across * across).sqrt()
}

/// Softly quantises `h` into steps, giving hills flat mesa tops between slopes.
fn terrace(h: f32, step: f32) -> f32 {
    let q = (h / step).floor();
    let f = h / step - q;
    let f3 = f * f * f;
    let s = f3 / (f3 + (1.0 - f) * (1.0 - f) * (1.0 - f));
    (q + s) * step
}

/// How many fbm octaves at base frequency `freq` keep the finest one's period at least
/// `min_period` metres (each octave roughly doubles the frequency).
fn octaves_for(freq: f32, min_period: f32) -> u32 {
    let mut o = 1;
    let mut f = freq;
    while f * 2.0 * min_period <= 1.0 && o < 8 {
        f *= 2.0;
        o += 1;
    }
    o
}

fn raw_height(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let mut h = -30.0 + fbm(x * 0.0004, z * 0.0004, 2, 999) * 8.0;
    // Lakes stay out of every runway's approach corridor and hills drop to gentle
    // bumps there, so each strip can be reached on a shallow glide.
    let mut corridor = 1.0_f32;
    for a in AIRPORTS {
        corridor = corridor.min(smoothstep(140.0, 700.0, corridor_dist(a, p)));
    }
    let hill_mask = 0.15 + 0.85 * corridor;
    for island in ISLANDS {
        let t = island_t(island, x, z);
        if t < -0.3 {
            continue;
        }
        let shore = smoothstep(-0.08, 0.14, t);
        let base = 3.0 + 16.0 * smoothstep(0.02, 0.5, t);
        // Band-limited to the 25 m grid: octaves finer than ~90 m only alias into a
        // checkerboard of facets (visible now that the near ground is flat-shaded); the
        // fine chunks add their own relief at 55 m.
        let n = fbm(x * island.freq, z * island.freq, octaves_for(island.freq, 90.0), island.seed).powf(1.5);
        let hills = island.amp * smoothstep(0.08, 0.45, t) * n * hill_mask;
        let hills = hills * 0.55 + terrace(hills, 34.0) * 0.45;
        // Small undulations everywhere on land so the flats catch the light unevenly.
        let bumps = (fbm(x * 0.0055 + 17.0, z * 0.0055 - 31.0, 2, island.seed + 3) - 0.5) * 16.0 * hill_mask * smoothstep(0.05, 0.2, t);
        let mut ih = base + hills + bumps;
        // Inland lakes: the high spots of a rotated, multi-octave noise dip below sea level
        // (the rotation and extra octaves keep value noise's grid out of the shorelines).
        let (lx, lz) = (x * 0.866 - z * 0.5, x * 0.5 + z * 0.866);
        let wx = (fbm(x * 0.0021 + 3.0, z * 0.0021 + 9.0, 2, island.seed + 11) - 0.5) * 700.0;
        let wz = (fbm(x * 0.0021 - 6.0, z * 0.0021 + 2.0, 2, island.seed + 13) - 0.5) * 700.0;
        let lake = fbm((lx + wx) * 0.0016 + 40.0, (lz + wz) * 0.0016 - 25.0, 4, island.seed + 7);
        let lake_mask = smoothstep(0.60, 0.67, lake) * smoothstep(0.22, 0.42, t) * corridor;
        ih += (-9.0 - ih) * lake_mask;
        // Rivers and inlets: the ridge lines of another warped noise, carved to water.
        let rv = fbm((lx + wz * 0.6) * 0.0012 - 11.0, (lz - wx * 0.6) * 0.0012 + 23.0, 3, island.seed + 19);
        let ridge = 1.0 - (2.0 * rv - 1.0).abs();
        let river_mask = smoothstep(0.86, 0.93, ridge) * smoothstep(0.02, 0.18, t) * corridor;
        ih += (-7.0 - ih.min(40.0)) * river_mask;
        let candidate = -30.0 + (ih + 30.0) * shore;
        if candidate > h {
            h = candidate;
        }
    }
    // Guarantee land around every airport.
    for a in AIRPORTS {
        let d = (Vec2::new(x, z) - a.pos).length();
        let t = 1.0 - d / 520.0;
        if t > -0.3 {
            let shore = smoothstep(-0.12, 0.35, t);
            let candidate = -30.0 + 40.0 * shore;
            if candidate > h {
                h = candidate;
            }
        }
    }
    h
}

fn biome_at(x: f32, z: f32) -> Biome {
    let mut best = Biome::Temperate;
    let mut bt = f32::MIN;
    for island in ISLANDS {
        let d = (Vec2::new(x, z) - island.center).length();
        let t = 1.0 - d / island.radius;
        if t > bt {
            bt = t;
            best = island.biome;
        }
    }
    best
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Face colour by height and slope, in the reference game's bands: turquoise shallows,
/// a sand rim, olive flats, terracotta slopes, dark red cliffs, pale rock on the tops.
fn tri_color(biome: Biome, h: f32, ny: f32, broad: f32, jitter: f32, patch: f32) -> [f32; 4] {
    // `broad` (-0.5..0.5) is a 400 m patch noise that picks meadow / pasture / scrub so the
    // colour regions are big and blobby; `jitter` is a small per-triangle brightness wobble
    // for the low-poly look. Slope colours blend in smoothly instead of flipping per triangle.
    let dirt_t = smoothstep(0.93, 0.80, ny);
    let rock_t = smoothstep(0.80, 0.68, ny);
    // Every band blends over a few metres: a hard threshold on height turns into a
    // checkerboard where undulating ground crosses it (adjacent facets fall either side).
    let sand_t = smoothstep(7.0, 4.5, h) * smoothstep(0.4, 0.2, rock_t);
    let c: [f32; 3] = if h < 0.3 {
        let t = (-h / 14.0).clamp(0.0, 1.0);
        mix3([0.36, 0.62, 0.64], [0.12, 0.30, 0.46], t)
    } else {
        let land = match biome {
            Biome::Temperate => {
                let meadow = [0.36, 0.44, 0.17];
                let dry = [0.46, 0.44, 0.21];
                let dark = [0.26, 0.36, 0.14];
                let mut g = mix3(meadow, dry, smoothstep(0.05, 0.35, broad));
                g = mix3(g, dark, smoothstep(-0.12, -0.38, broad));
                if h > 150.0 {
                    g = mix3(g, [0.62, 0.62, 0.48], smoothstep(150.0, 200.0, h));
                }
                if h > 200.0 {
                    g = mix3(g, [0.78, 0.78, 0.74], smoothstep(200.0, 240.0, h));
                }
                let dirt = mix3([0.64, 0.45, 0.28], [0.52, 0.50, 0.54], smoothstep(60.0, 85.0, h) * smoothstep(-0.2, 0.0, broad));
                let g = mix3(g, dirt, dirt_t);
                mix3(g, [0.46, 0.30, 0.22], rock_t)
            }
            Biome::Snow => {
                let mut g = [0.88 + broad * 0.05, 0.90 + broad * 0.04, 0.95];
                if h < 12.0 {
                    g = mix3([0.54, 0.60, 0.52], g, smoothstep(4.0, 12.0, h));
                }
                let g = mix3(g, [0.64, 0.68, 0.76], dirt_t);
                mix3(g, [0.36, 0.38, 0.46], rock_t)
            }
            Biome::Desert => {
                let g = mix3([0.76, 0.70, 0.52], [0.70, 0.62, 0.44], smoothstep(0.0, 0.4, broad));
                let g = mix3(g, [0.58, 0.44, 0.30], dirt_t);
                mix3(g, [0.48, 0.28, 0.18], rock_t)
            }
            Biome::Tropical => {
                let mut g = mix3([0.27, 0.45, 0.19], [0.35, 0.49, 0.21], smoothstep(-0.1, 0.35, broad));
                if h < 9.0 {
                    g = mix3([0.86, 0.80, 0.62], g, smoothstep(4.0, 9.0, h));
                }
                let g = mix3(g, [0.55, 0.46, 0.30], dirt_t);
                mix3(g, [0.42, 0.34, 0.26], rock_t)
            }
            Biome::Volcanic => {
                let mut g = mix3([0.32, 0.30, 0.24], [0.38, 0.34, 0.26], smoothstep(-0.1, 0.35, broad));
                if h < 8.0 {
                    g = mix3([0.18, 0.17, 0.16], g, smoothstep(3.0, 8.0, h));
                }
                if h > 260.0 {
                    g = mix3(g, [0.20, 0.18, 0.18], smoothstep(260.0, 300.0, h));
                }
                let g = mix3(g, [0.42, 0.24, 0.16], dirt_t);
                mix3(g, [0.16, 0.14, 0.14], rock_t)
            }
        };
        if matches!(biome, Biome::Tropical | Biome::Volcanic) {
            land
        } else {
            mix3(land, [0.74 + broad * 0.04, 0.64 + broad * 0.04, 0.42], sand_t)
        }
    };
    // Rock patches (not in the water or on the beach).
    let c = if h > 4.0 {
        let rock = match biome {
            Biome::Snow => [0.47, 0.44, 0.53],
            Biome::Temperate => [0.55, 0.50, 0.42],
            Biome::Desert => [0.58, 0.32, 0.20],
            Biome::Tropical => [0.40, 0.35, 0.28],
            Biome::Volcanic => [0.14, 0.13, 0.13],
        };
        mix3(c, rock, patch * 0.9)
    } else {
        c
    };
    let k = 1.0 + jitter * 0.10;
    let l = meshes::lin([c[0] * k, c[1] * k, c[2] * k]);
    [l[0], l[1], l[2], 1.0]
}

/// One terrain chunk (CHUNK coarse cells square). The distant coarse version is an indexed,
/// smooth-shaded mesh with a colour per vertex; the near `fine` version (half the cell size,
/// with the detail relief) is flat-shaded with one normal and colour per triangle, the
/// reference's crisp small facets (Adam: no *large* poly shapes, but the ground there is
/// clearly faceted). The diagonal rule matches `Terrain::height`.
fn build_chunk(t: &Terrain, ci: usize, cj: usize, fine: bool) -> Option<Mesh> {
    let mut any_land = false;
    'outer: for j in cj * CHUNK..=cj * CHUNK + CHUNK {
        for i in ci * CHUNK..=ci * CHUNK + CHUNK {
            if t.sample(i, j) > -4.0 {
                any_land = true;
                break 'outer;
            }
        }
    }
    if !any_land {
        return None;
    }
    let (cells, size) = if fine { (2 * CHUNK, FINE) } else { (CHUNK, CELL) };
    let i0 = ci * cells;
    let j0 = cj * cells;
    let n = cells + 1;
    let hmax = if fine { 2 * (N - 1) } else { N - 1 };
    let sample = |i: usize, j: usize| -> f32 {
        let (i, j) = (i.min(hmax), j.min(hmax));
        if fine { t.fine_sample(i, j) } else { t.sample(i, j) }
    };
    let color_at = |x: f32, z: f32, h: f32, ny: f32| -> [f32; 4] {
        let biome = t.biome(x, z);
        let broad = fbm(x * 0.0025, z * 0.0025, 3, 5) - 0.5;
        let jitter = value_noise(x * 0.05, z * 0.05, 9) - 0.5;
        // Rock outcrops: soft-edged patches from a mid-scale noise, more where it slopes.
        // Two octaves only (143 m and 70 m): a finer one wobbled across the narrow
        // threshold from one 12.5 m facet to the next and made a rock/grass checkerboard.
        let pn = fbm(x * 0.007 + 11.0, z * 0.007 - 5.0, 2, 21);
        let patch = smoothstep(0.52, 0.66, pn + 0.12 * smoothstep(0.995, 0.9, ny));
        tri_color(biome, h, ny, broad, jitter * 0.3, patch)
    };
    // Slope for the colour bands from the smooth 25 m surface: the facets light up
    // individually but do not each flip between grass and dirt (that was the "confetti").
    let coarse_ny = |x: f32, z: f32| -> f32 {
        let d = 12.5;
        let dx = t.coarse_height(x + d, z) - t.coarse_height(x - d, z);
        let dz = t.coarse_height(x, z + d) - t.coarse_height(x, z - d);
        Vec3::new(-dx, 2.0 * d, -dz).normalize().y
    };
    let mut grid_pos: Vec<Vec3> = Vec::with_capacity(n * n);
    let mut grid_nrm: Vec<Vec3> = Vec::with_capacity(n * n);
    for j in j0..=j0 + cells {
        for i in i0..=i0 + cells {
            let x = -WORLD_HALF + i as f32 * size;
            let z = -WORLD_HALF + j as f32 * size;
            let h = sample(i, j);
            // Normal from central differences of the same height field (seamless across chunks).
            let hl = sample(i.saturating_sub(1), j);
            let hr = sample(i + 1, j);
            let hd = sample(i, j.saturating_sub(1));
            let hu = sample(i, j + 1);
            grid_pos.push(Vec3::new(x, h, z));
            grid_nrm.push(Vec3::new(hl - hr, 2.0 * size, hd - hu).normalize());
        }
    }
    // Alternate the diagonal like Terrain::height (grid indices, not chunk-local). A fine
    // cell splits the same way as the coarse cell it sits in, so every fine triangle stays
    // inside one coarse plane; splitting the other way folded the cells that straddle the
    // coarse diagonal and lit them as a checkerboard.
    let cell_tris = |i: usize, j: usize| -> [[usize; 3]; 2] {
        let a = j * n + i;
        let (b, c) = (a + 1, a + n);
        let d = c + 1;
        let bc = if fine { t.diagonal_bc((i0 + i) / 2, (j0 + j) / 2) } else { t.diagonal_bc(i0 + i, j0 + j) };
        if bc {
            [[a, c, b], [b, c, d]]
        } else {
            [[a, d, b], [a, c, d]]
        }
    };
    let mesh = if fine {
        let cap = cells * cells * 6;
        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(cap);
        let mut normals: Vec<[f32; 3]> = Vec::with_capacity(cap);
        let mut colors: Vec<[f32; 4]> = Vec::with_capacity(cap);
        for j in 0..cells {
            for i in 0..cells {
                for tri in cell_tris(i, j) {
                    let (pa, pb, pc) = (grid_pos[tri[0]], grid_pos[tri[1]], grid_pos[tri[2]]);
                    let nrm = (pb - pa).cross(pc - pa).normalize();
                    let cen = (pa + pb + pc) / 3.0;
                    let col = color_at(cen.x, cen.z, cen.y, coarse_ny(cen.x, cen.z));
                    for p in [pa, pb, pc] {
                        positions.push(p.to_array());
                        normals.push(nrm.to_array());
                        colors.push(col);
                    }
                }
            }
        }
        Mesh::new(bevy::render::mesh::PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    } else {
        let positions: Vec<[f32; 3]> = grid_pos.iter().map(|p| p.to_array()).collect();
        let normals: Vec<[f32; 3]> = grid_nrm.iter().map(|v| v.to_array()).collect();
        let colors: Vec<[f32; 4]> = grid_pos.iter().zip(&grid_nrm).map(|(p, v)| color_at(p.x, p.z, p.y, v.y)).collect();
        let mut indices: Vec<u32> = Vec::with_capacity(cells * cells * 6);
        for j in 0..cells {
            for i in 0..cells {
                for tri in cell_tris(i, j) {
                    indices.extend(tri.iter().map(|&k| k as u32));
                }
            }
        }
        Mesh::new(bevy::render::mesh::PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
            .with_inserted_indices(bevy::render::mesh::Indices::U32(indices))
    };
    Some(mesh)
}

fn push_tri_up(tris: &mut Vec<[f32; 3]>, cols: &mut Vec<[f32; 4]>, a: Vec3, b: Vec3, c: Vec3, col: [f32; 4]) {
    let up = (b - a).cross(c - a).y >= 0.0;
    let (b, c) = if up { (b, c) } else { (c, b) };
    for p in [a, b, c] {
        tris.push(p.to_array());
        cols.push(col);
    }
}

/// Dirt roads and streets as ribbons draped over the terrain (a bit above it), grouped
/// into 3 km regions so they cull like the chunks.
fn build_roads(t: &Terrain) -> Vec<Mesh> {
    const REGION: f32 = 3000.0;
    let n = (2.0 * WORLD_HALF / REGION).ceil() as usize;
    let mut buckets: Vec<(Vec<[f32; 3]>, Vec<[f32; 4]>)> = (0..n * n).map(|_| (Vec::new(), Vec::new())).collect();
    for (a, b) in &t.roads {
        let d = *b - *a;
        let len = d.length();
        if len < 1.0 {
            continue;
        }
        let dir = d / len;
        let perp = Vec2::new(-dir.y, dir.x) * 3.2;
        let steps = (len / 7.0).ceil().max(1.0) as usize;
        let l = meshes::lin(match t.biome(a.x, a.y) {
            Biome::Snow | Biome::Volcanic => [0.40, 0.38, 0.38],
            Biome::Desert => [0.66, 0.50, 0.32],
            _ => [0.58, 0.47, 0.32],
        });
        let col = [l[0], l[1], l[2], 1.0];
        let mut prev: Option<(Vec3, Vec3)> = None;
        for k in 0..=steps {
            let p = *a + dir * (k as f32 * len / steps as f32);
            let (lp, rp) = (p - perp, p + perp);
            let (hl, hr) = (t.height(lp.x, lp.y), t.height(rp.x, rp.y));
            let on_runway = t.airports.iter().any(|ap| {
                let l = ap.local(p);
                l.x.abs() < ap.def.length * 0.5 + 60.0 && l.y.abs() < ap.def.width * 0.5 + 12.0
            });
            if hl < 0.6 || hr < 0.6 || on_runway {
                prev = None;
                continue;
            }
            let vl = Vec3::new(lp.x, hl + 0.22, lp.y);
            let vr = Vec3::new(rp.x, hr + 0.22, rp.y);
            if let Some((pl, pr)) = prev {
                let bi = (((p.x + WORLD_HALF) / REGION) as usize).min(n - 1);
                let bj = (((p.y + WORLD_HALF) / REGION) as usize).min(n - 1);
                let (tris, cols) = &mut buckets[bj * n + bi];
                push_tri_up(tris, cols, pl, pr, vr, col);
                push_tri_up(tris, cols, pl, vr, vl, col);
            }
            prev = Some((vl, vr));
        }
    }
    buckets.into_iter().filter(|(t, _)| !t.is_empty()).map(|(t, c)| meshes::flat_mesh(t, Some(c))).collect()
}

/// Crop fields as striped quads draped over the terrain.
fn build_fields(t: &Terrain) -> Option<Mesh> {
    let mut tris = Vec::new();
    let mut cols = Vec::new();
    for (c, half, yaw, crop) in &t.fields {
        let (sy, cy) = yaw.sin_cos();
        let du = Vec2::new(cy, -sy);
        let dv = Vec2::new(sy, cy);
        let base: [f32; 3] = match crop {
            0 => [0.72, 0.62, 0.30],
            1 => [0.40, 0.55, 0.22],
            2 => [0.62, 0.48, 0.30],
            _ => [0.55, 0.60, 0.26],
        };
        let nu = (half.x * 2.0 / 9.0).ceil() as usize;
        let nv = (half.y * 2.0 / 12.0).ceil() as usize;
        let at = |iu: usize, iv: usize| -> Vec3 {
            let u = -half.x + iu as f32 * (2.0 * half.x / nu as f32);
            let v = -half.y + iv as f32 * (2.0 * half.y / nv as f32);
            let p = *c + du * u + dv * v;
            Vec3::new(p.x, t.height(p.x, p.y) + 0.12, p.y)
        };
        for iu in 0..nu {
            let k = 0.85 + (iu % 2) as f32 * 0.15;
            let l = meshes::lin([base[0] * k, base[1] * k, base[2] * k]);
            let col = [l[0], l[1], l[2], 1.0];
            for iv in 0..nv {
                let (a, b, cc, d) = (at(iu, iv), at(iu + 1, iv), at(iu + 1, iv + 1), at(iu, iv + 1));
                if [a, b, cc, d].iter().any(|p| p.y < 1.0) {
                    continue;
                }
                push_tri_up(&mut tris, &mut cols, a, b, cc, col);
                push_tri_up(&mut tris, &mut cols, a, cc, d, col);
            }
        }
    }
    if tris.is_empty() { None } else { Some(meshes::flat_mesh(tris, Some(cols))) }
}

#[derive(Component)]
pub struct CoarseChunk {
    ci: usize,
    cj: usize,
}

#[derive(Resource)]
pub struct TerrainLod {
    ground: Handle<StandardMaterial>,
    fine: std::collections::HashMap<(usize, usize), Entity>,
}

/// Swaps coarse chunks near the camera for fine ones (and back), a few builds per frame.
pub fn terrain_lod(
    mut commands: Commands,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    terrain: Res<Terrain>,
    lod: Option<ResMut<TerrainLod>>,
    cams: Query<&GlobalTransform, With<crate::MainCamera>>,
    mut coarse: Query<(&CoarseChunk, &mut Visibility)>,
) {
    let Some(mut lod) = lod else { return };
    let Ok(cam) = cams.single() else { return };
    let cp = cam.translation();
    let cam2 = Vec2::new(cp.x, cp.z);
    let half = CHUNK as f32 * CELL * 0.5;
    let mut pending: Vec<(f32, usize, usize)> = Vec::new();
    for (chunk, mut vis) in &mut coarse {
        let centre = Vec2::new(-WORLD_HALF + chunk.ci as f32 * CHUNK as f32 * CELL + half, -WORLD_HALF + chunk.cj as f32 * CHUNK as f32 * CELL + half);
        // Distance from the camera to the chunk square.
        let d = (cam2 - centre).abs() - Vec2::splat(half);
        let dist = d.max(Vec2::ZERO).length();
        let key = (chunk.ci, chunk.cj);
        if let Some(&e) = lod.fine.get(&key) {
            if dist > FINE_DROP {
                if e != Entity::PLACEHOLDER {
                    commands.entity(e).despawn();
                }
                lod.fine.remove(&key);
                *vis = Visibility::Inherited;
            }
        } else if dist < FINE_RADIUS {
            pending.push((dist, chunk.ci, chunk.cj));
        }
    }
    // Nearest first; more builds per frame while a lot is outstanding (a teleport).
    pending.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let budget = if pending.len() > 8 { 4 } else { 1 };
    for (_, ci, cj) in pending.into_iter().take(budget) {
        if let Some(mesh) = build_chunk(&terrain, ci, cj, true) {
            let e = commands.spawn((Mesh3d(mesh_assets.add(mesh)), MeshMaterial3d(lod.ground.clone()), Transform::IDENTITY, WorldEntity)).id();
            lod.fine.insert((ci, cj), e);
            for (chunk, mut vis) in &mut coarse {
                if chunk.ci == ci && chunk.cj == cj {
                    *vis = Visibility::Hidden;
                }
            }
        } else {
            // Nothing to draw here (open sea): remember it so we do not try every frame.
            lod.fine.insert((ci, cj), Entity::PLACEHOLDER);
        }
    }
}

#[derive(Component)]
pub struct WorldEntity;

/// Sky pieces that stay at a fixed offset from the active camera.
#[derive(Component)]
pub struct SkyElement {
    pub offset: Vec3,
}

pub fn sky_follow(cams: Query<(&Camera, &GlobalTransform)>, mut sky: Query<(&mut Transform, &SkyElement)>) {
    let Some(cam) = cams.iter().find(|(c, _)| c.is_active && c.order == 0).map(|(_, t)| t.translation()) else { return };
    for (mut t, s) in sky.iter_mut() {
        t.translation = cam + s.offset;
    }
}

/// Metres covered by one repeat of the water sparkle texture.
const WATER_TILE_M: f32 = 90.0;

#[derive(Resource)]
pub struct WaterMaterial(pub Handle<StandardMaterial>);

/// Blue water with scattered lighter flecks, tiled over the ocean.
fn water_texture() -> Image {
    const S: usize = 128;
    let base = [50u8, 82, 128];
    let fleck = [122u8, 160, 205];
    let mut px = vec![0u8; S * S * 4];
    for i in 0..S * S {
        px[i * 4..i * 4 + 3].copy_from_slice(&base);
        px[i * 4 + 3] = 255;
    }
    let mut rng = Rng(0x5EA5_1DE5_0000_0001);
    for _ in 0..70 {
        let cx = rng.range(0.0, S as f32);
        let cy = rng.range(0.0, S as f32);
        let r = rng.range(1.2, 2.6);
        let ir = r.ceil() as i32 + 1;
        for dy in -ir..=ir {
            for dx in -ir..=ir {
                let d = ((dx as f32).powi(2) + (dy as f32).powi(2)).sqrt();
                if d > r {
                    continue;
                }
                let x = (cx as i32 + dx).rem_euclid(S as i32) as usize;
                let y = (cy as i32 + dy).rem_euclid(S as i32) as usize;
                let k = (y * S + x) * 4;
                let a = (1.0 - d / r).clamp(0.0, 1.0) * 0.9;
                for c in 0..3 {
                    px[k + c] = (px[k + c] as f32 + (fleck[c] as f32 - px[k + c] as f32) * a) as u8;
                }
            }
        }
    }
    let mut img = Image::new(
        Extent3d { width: S as u32, height: S as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    img
}

/// Drifts the sparkle texture so the water glitters.
pub fn water_shimmer(time: Res<Time>, water: Option<Res<WaterMaterial>>, mut materials: ResMut<Assets<StandardMaterial>>, mut off: Local<Option<bool>>) {
    // WINGWRIGHT_NOSHIMMER=1 freezes the texture (used to measure its frame cost).
    let disabled = *off.get_or_insert_with(|| std::env::var_os("WINGWRIGHT_NOSHIMMER").is_some());
    if disabled {
        return;
    }
    let Some(water) = water else { return };
    if let Some(m) = materials.get_mut(&water.0) {
        let t = time.elapsed_secs();
        m.uv_transform.translation = Vec2::new((t * 0.015).fract(), (t * 0.011).fract());
    }
}

pub fn spawn_world(
    mut commands: Commands,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    terrain: Res<Terrain>,
) {
    // Terrain chunks.
    let ground_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.1,
        ..default()
    });
    for mesh in build_roads(&terrain) {
        commands.spawn((Mesh3d(mesh_assets.add(mesh)), MeshMaterial3d(ground_mat.clone()), Transform::IDENTITY, NotShadowCaster, WorldEntity));
    }
    if let Some(mesh) = build_fields(&terrain) {
        commands.spawn((Mesh3d(mesh_assets.add(mesh)), MeshMaterial3d(ground_mat.clone()), Transform::IDENTITY, NotShadowCaster, WorldEntity));
    }
    let chunks = (N - 1) / CHUNK;
    for cj in 0..chunks {
        for ci in 0..chunks {
            if let Some(mesh) = build_chunk(&terrain, ci, cj, false) {
                commands.spawn((
                    Mesh3d(mesh_assets.add(mesh)),
                    MeshMaterial3d(ground_mat.clone()),
                    Transform::IDENTITY,
                    CoarseChunk { ci, cj },
                    WorldEntity,
                ));
            }
        }
    }
    commands.insert_resource(TerrainLod { ground: ground_mat.clone(), fine: std::collections::HashMap::new() });

    // Ocean: a tiled sparkle texture, slightly translucent so the turquoise shallows show.
    let ocean_size = WORLD_HALF * 6.0;
    let ocean = mesh_assets.add(Plane3d::default().mesh().size(ocean_size, ocean_size));
    let water_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.88),
        base_color_texture: Some(images.add(water_texture())),
        uv_transform: Affine2::from_scale(Vec2::splat(ocean_size / WATER_TILE_M)),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.55,
        metallic: 0.0,
        reflectance: 0.35,
        ..default()
    });
    commands.insert_resource(WaterMaterial(water_mat.clone()));
    commands.spawn((Mesh3d(ocean), MeshMaterial3d(water_mat), Transform::from_xyz(0.0, 0.0, 0.0), NotShadowCaster, WorldEntity));

    // Sun and sky. A warm sun about 38 degrees up (long shadows that model the hills and
    // sit trees on the ground, like the reference) with a cool, dimmer ambient so the
    // shadow side of things actually reads darker.
    let sun_rot = Quat::from_euler(EulerRot::YXZ, -0.7, -0.66, 0.0);
    commands.spawn((
        DirectionalLight { illuminance: 9_500.0, color: Color::srgb(1.0, 0.96, 0.88), shadows_enabled: !debug_flag("noshadow"), ..default() },
        // Shadows out to 2.6 km so a hillside of trees still throws them at flight distances.
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            first_cascade_far_bound: 50.0,
            maximum_distance: 2600.0,
            overlap_proportion: 0.15,
            ..default()
        }
        .build(),
        Transform::from_rotation(sun_rot),
        WorldEntity,
    ));
    commands.insert_resource(AmbientLight { color: Color::srgb(0.72, 0.82, 1.0), brightness: 250.0, ..default() });
    commands.insert_resource(ClearColor(Color::srgb(HAZE[0], HAZE[1], HAZE[2])));

    // Sky dome and sun disc follow the camera; clouds are static.
    let sun_dir = sun_rot * Vec3::Z;
    commands.spawn((
        Mesh3d(mesh_assets.add(meshes::sky_dome(HAZE, [0.24, 0.50, 0.88], [0.62, 0.72, 0.86]))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            cull_mode: Some(Face::Front),
            fog_enabled: false,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(14_000.0)),
        NotShadowCaster,
        NotShadowReceiver,
        SkyElement { offset: Vec3::ZERO },
        WorldEntity,
    ));
    // The sun is emissive far above the bloom threshold, so it glows.
    commands.spawn((
        Mesh3d(mesh_assets.add(Sphere::new(1.0).mesh().ico(3).unwrap())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: LinearRgba::rgb(10.0, 7.8, 5.2),
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            fog_enabled: false,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(520.0)),
        NotShadowCaster,
        NotShadowReceiver,
        SkyElement { offset: sun_dir * 11_000.0 },
        WorldEntity,
    ));
    // Cumulus: big clusters of flattened puffs with a common base. The shading is baked into
    // the puff mesh (white tops, blue-grey undersides) and drawn unlit, so they read as soft
    // painterly clouds from every direction; they fade into the haze with distance.
    let cloud_mesh = mesh_assets.add(meshes::cloud_puff());
    let cloud_mat = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() });
    let mut crng = Rng(0xC10D_5EED_1234_5678);
    for _ in 0..80 {
        let cx = crng.range(-13000.0, 13000.0);
        let cz = crng.range(-13000.0, 13000.0);
        let base = crng.range(1100.0, 1700.0);
        let w = crng.range(400.0, 900.0);
        let n = 8 + (crng.f32() * 9.0) as usize;
        for k in 0..n {
            let off = Vec2::new(crng.range(-w, w), crng.range(-w * 0.45, w * 0.45));
            let r = crng.range(150.0, 300.0) * if k == 0 { 1.5 } else { 1.0 };
            let tall = if k == 0 { crng.range(0.75, 0.95) } else { crng.range(0.5, 0.75) };
            let size = Vec3::new(r * crng.range(0.9, 1.3), r * tall, r * crng.range(0.9, 1.25));
            // Flat bottoms: every puff's underside sits close to the cloud base.
            let y = base + size.y * 0.75 + crng.range(0.0, 40.0);
            commands.spawn((
                Mesh3d(cloud_mesh.clone()),
                MeshMaterial3d(cloud_mat.clone()),
                Transform { translation: Vec3::new(cx + off.x, y, cz + off.y), rotation: Quat::IDENTITY, scale: size },
                NotShadowCaster,
                NotShadowReceiver,
                WorldEntity,
            ));
        }
    }

    // Airports and scenery.
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let mut lib = SceneryLib::new(&mut mesh_assets, &mut materials);
    for a in &terrain.airports {
        spawn_airport(&mut commands, &mut mesh_assets, &mut materials, &mut lib, a, &terrain, &mut rng);
    }
    spawn_vegetation(&mut commands, &lib, &terrain, &mut rng);
    spawn_rocks_and_bushes(&mut commands, &lib, &terrain, &mut rng);
    commands.insert_resource(lib.colliders.replace(TreeColliders::default()));
}

/// Boulders where the ground is steep or volcanic, bushes on low temperate and tropical
/// flats: cheap detail that breaks up the big facets.
fn spawn_rocks_and_bushes(commands: &mut Commands, lib: &SceneryLib, terrain: &Terrain, rng: &mut Rng) {
    for island in ISLANDS {
        let rocks = match island.biome {
            Biome::Volcanic => 320,
            Biome::Snow => 180,
            _ => (island.radius / 2900.0 * 260.0) as usize,
        };
        let bushes = match island.biome {
            Biome::Temperate | Biome::Tropical => (island.radius / 2900.0 * 1200.0) as usize,
            _ => 0,
        };
        let mut tries = 0;
        let mut placed = 0;
        while placed < rocks && tries < rocks * 10 {
            tries += 1;
            let ang = rng.range(0.0, std::f32::consts::TAU);
            let r = island.radius * rng.f32().sqrt();
            let x = island.center.x + ang.cos() * r;
            let z = island.center.y + ang.sin() * r;
            let h = terrain.height(x, z);
            if h < 3.0 {
                continue;
            }
            let slope = (terrain.height(x + 12.0, z) - h).abs().max((terrain.height(x, z + 12.0) - h).abs());
            if island.biome != Biome::Volcanic && slope < 5.0 {
                continue;
            }
            if AIRPORTS.iter().any(|a| corridor_dist(a, Vec2::new(x, z)) < 220.0) || terrain.on_pad(x, z, 20.0) {
                continue;
            }
            let s = rng.range(1.2, 4.5);
            lib.colliders.borrow_mut().add(Vec2::new(x, z), s * 0.9, h - 1.0, h + s * 0.35, "a boulder");
            commands.spawn((
                Mesh3d(lib.rock.clone()),
                MeshMaterial3d(lib.m_rock.clone()),
                Transform { translation: Vec3::new(x, h - s * 0.35, z), rotation: Quat::from_euler(EulerRot::YXZ, rng.range(0.0, 3.0), rng.range(-0.4, 0.4), 0.0), scale: Vec3::new(s * rng.range(0.8, 1.4), s * 0.7, s) },
                NotShadowReceiver,
                WorldEntity,
            ));
            placed += 1;
        }
        tries = 0;
        placed = 0;
        while placed < bushes && tries < bushes * 6 {
            tries += 1;
            let ang = rng.range(0.0, std::f32::consts::TAU);
            let r = island.radius * rng.f32().sqrt();
            let x = island.center.x + ang.cos() * r;
            let z = island.center.y + ang.sin() * r;
            let h = terrain.height(x, z);
            if !(3.0..40.0).contains(&h) {
                continue;
            }
            if terrain.airports.iter().any(|a| a.in_zone(Vec3::new(x, h, z))) || terrain.on_pad(x, z, 0.0) {
                continue;
            }
            let s = rng.range(0.7, 1.6);
            commands.spawn((
                Mesh3d(lib.bush.clone()),
                MeshMaterial3d(lib.m_bush.clone()),
                Transform { translation: Vec3::new(x, h - s * 0.3, z), rotation: Quat::IDENTITY, scale: Vec3::new(s * 1.3, s * 0.8, s * 1.2) },
                NotShadowCaster,
                NotShadowReceiver,
                WorldEntity,
            ));
            placed += 1;
        }
    }
}

/// Shared meshes/materials for scenery.
pub(crate) struct SceneryLib {
    pub unit_cube: Handle<Mesh>,
    /// Whole trees (trunk + crown) as one vertex-coloured mesh each, a few colour variants
    /// per kind, all drawn with `m_veg` so every variant is a single instanced batch.
    pub trees: Vec<Handle<Mesh>>,
    pub pines: Vec<Handle<Mesh>>,
    pub snow_pines: Vec<Handle<Mesh>>,
    pub m_veg: Handle<StandardMaterial>,
    pub cone_small: Handle<Mesh>,
    pub rock: Handle<Mesh>,
    pub bush: Handle<Mesh>,
    pub m_rock: Handle<StandardMaterial>,
    pub m_bush: Handle<StandardMaterial>,
    pub cactus: Handle<Mesh>,
    pub palm: Handle<Mesh>,
    pub m_trunk: Handle<StandardMaterial>,
    pub m_leaf: Handle<StandardMaterial>,
    pub m_cactus: Handle<StandardMaterial>,
    pub m_runway: Handle<StandardMaterial>,
    pub m_white: Handle<StandardMaterial>,
    pub m_grey: Handle<StandardMaterial>,
    pub m_red: Handle<StandardMaterial>,
    pub m_olive: Handle<StandardMaterial>,
    pub m_blue: Handle<StandardMaterial>,
    pub m_brown: Handle<StandardMaterial>,
    pub m_yellow: Handle<StandardMaterial>,
    pub m_concrete: Handle<StandardMaterial>,
    pub m_glass: Handle<StandardMaterial>,
    pub m_orange: Handle<StandardMaterial>,
    pub m_cream: Handle<StandardMaterial>,
    pub m_brick: Handle<StandardMaterial>,
    pub m_slate: Handle<StandardMaterial>,
    pub m_terracotta: Handle<StandardMaterial>,
    pub m_dark: Handle<StandardMaterial>,
    pub m_green: Handle<StandardMaterial>,
    pub m_tan: Handle<StandardMaterial>,
    pub m_steel: Handle<StandardMaterial>,
    pub m_navy: Handle<StandardMaterial>,
    pub m_glass_tint: Handle<StandardMaterial>,
    /// Unit building blocks for the settlements: gable prism, pyramid, cylinder (base at
    /// y = 0), cone and hemisphere, all 1 m in every direction.
    pub gable: Handle<Mesh>,
    pub pyramid: Handle<Mesh>,
    pub cyl: Handle<Mesh>,
    pub cone_unit: Handle<Mesh>,
    pub dome_unit: Handle<Mesh>,
    /// Everything solid that gets spawned registers here; inserted as a resource afterwards.
    colliders: std::cell::RefCell<TreeColliders>,
}

impl SceneryLib {
    pub fn solid_cylinder(&self, xz: Vec2, radius: f32, base: f32, top: f32, what: &'static str) {
        self.colliders.borrow_mut().add(xz, radius, base, top, what);
    }

    pub fn solid_box(&self, bottom: Vec3, size: Vec3, yaw: f32, what: &'static str) {
        self.colliders.borrow_mut().add_box(bottom, size, yaw, what);
    }
}

impl SceneryLib {
    fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let mat = |materials: &mut Assets<StandardMaterial>, r: f32, g: f32, b: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                perceptual_roughness: 0.9,
                ..default()
            })
        };
        SceneryLib {
            unit_cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            trees: LEAF_COLORS.iter().map(|&leaf| meshes.add(leafy_tree(leaf))).collect(),
            pines: PINE_COLORS.iter().map(|&needle| meshes.add(pine_tree(needle, false))).collect(),
            snow_pines: PINE_COLORS.iter().map(|&needle| meshes.add(pine_tree(needle, true))).collect(),
            m_veg: materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.95, reflectance: 0.15, ..default() }),
            cone_small: meshes.add(meshes::cone_up(0.4, 0.9, 8)),
            rock: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
            bush: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
            m_rock: mat(materials, 0.42, 0.40, 0.40),
            m_bush: mat(materials, 0.30, 0.48, 0.24),
            cactus: meshes.add(Cylinder::new(0.7, 5.0).mesh().resolution(7).build()),
            palm: meshes.add(palm_tree()),
            m_trunk: mat(materials, 0.42, 0.28, 0.16),
            m_leaf: mat(materials, 0.18, 0.46, 0.2),
            m_cactus: mat(materials, 0.3, 0.55, 0.28),
            m_runway: mat(materials, 0.17, 0.17, 0.29),
            m_white: mat(materials, 0.92, 0.92, 0.9),
            m_grey: mat(materials, 0.55, 0.55, 0.58),
            m_red: mat(materials, 0.75, 0.18, 0.15),
            m_olive: mat(materials, 0.45, 0.5, 0.32),
            m_blue: mat(materials, 0.25, 0.5, 0.85),
            m_brown: mat(materials, 0.45, 0.3, 0.18),
            m_yellow: mat(materials, 0.95, 0.8, 0.25),
            m_concrete: mat(materials, 0.7, 0.68, 0.64),
            m_orange: mat(materials, 0.95, 0.45, 0.1),
            m_cream: mat(materials, 0.88, 0.82, 0.66),
            m_brick: mat(materials, 0.58, 0.28, 0.2),
            m_slate: mat(materials, 0.28, 0.3, 0.34),
            m_terracotta: mat(materials, 0.72, 0.36, 0.2),
            m_dark: mat(materials, 0.14, 0.14, 0.16),
            m_green: mat(materials, 0.22, 0.48, 0.28),
            m_tan: mat(materials, 0.74, 0.62, 0.42),
            m_steel: mat(materials, 0.58, 0.6, 0.64),
            m_navy: mat(materials, 0.14, 0.22, 0.46),
            m_glass_tint: mat(materials, 0.36, 0.52, 0.62),
            gable: meshes.add(crate::settle::gable_mesh()),
            pyramid: meshes.add(crate::meshes::pyramid(0.5, 0.5, 1.0)),
            cyl: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(14).build().translated_by(Vec3::new(0.0, 0.5, 0.0))),
            cone_unit: meshes.add(crate::meshes::cone_up(0.5, 1.0, 12)),
            dome_unit: meshes.add(crate::meshes::dome(1.0, 6, 16)),
            colliders: std::cell::RefCell::new(TreeColliders::default()),
            m_glass: materials.add(StandardMaterial {
                base_color: Color::srgba(0.4, 0.7, 0.95, 0.5),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.2,
                ..default()
            }),
        }
    }
}

/// Spawns a box of `size` centered at `pos` (its bottom sits at pos.y), rotated by `yaw`.
pub(crate) fn spawn_box(
    commands: &mut Commands,
    lib: &SceneryLib,
    mat: &Handle<StandardMaterial>,
    pos: Vec3,
    size: Vec3,
    yaw: f32,
) {
    // Anything taller than a kerb is solid (walls, tanks, masts, crates); slabs are not.
    if size.y >= 1.0 {
        lib.colliders.borrow_mut().add_box(pos, size, yaw, "a building");
    }
    commands.spawn((
        Mesh3d(lib.unit_cube.clone()),
        MeshMaterial3d(mat.clone()),
        Transform {
            translation: pos + Vec3::new(0.0, size.y * 0.5, 0.0),
            rotation: Quat::from_rotation_y(yaw),
            scale: size,
        },
        WorldEntity,
    ));
}

/// Leaf colours (sRGB): green, olive, and the odd rusty autumn tree, like the reference's mix.
const LEAF_COLORS: [[f32; 3]; 3] = [[0.17, 0.36, 0.15], [0.28, 0.38, 0.14], [0.46, 0.34, 0.13]];
const PINE_COLORS: [[f32; 3]; 3] = [[0.12, 0.30, 0.17], [0.15, 0.34, 0.24], [0.20, 0.36, 0.15]];
const TRUNK_COLOR: [f32; 3] = [0.38, 0.25, 0.14];
/// Heights of the unit tree meshes (scale 1); the colliders use them.
pub const TREE_HEIGHT: f32 = 13.0;
pub const TREE_RADIUS: f32 = 3.6;
pub const PINE_HEIGHT: f32 = 18.0;
pub const PINE_RADIUS: f32 = 3.0;

/// A chunky deciduous tree: thick trunk and a lumpy crown of six overlapping faceted
/// spheres, lighter towards the top. About 13 m tall at scale 1.
fn leafy_tree(leaf: [f32; 3]) -> Mesh {
    let trunk = Cylinder::new(0.55, 5.5).mesh().resolution(6).build().translated_by(Vec3::new(0.0, 2.75, 0.0));
    let mut parts = vec![meshes::tinted(trunk, TRUNK_COLOR, 0.0, 5.0, 0.8, 1.0)];
    let blobs: [(f32, f32, f32, f32); 6] = [
        (0.0, 8.4, 0.0, 3.7),
        (2.4, 7.2, 0.7, 2.8),
        (-2.3, 7.5, -0.9, 2.9),
        (0.5, 6.6, 2.5, 2.6),
        (-0.4, 7.0, -2.5, 2.5),
        (0.3, 10.6, 0.3, 2.4),
    ];
    for (x, y, z, r) in blobs {
        let blob = Sphere::new(r).mesh().ico(1).unwrap().translated_by(Vec3::new(x, y, z));
        parts.push(meshes::tinted(blob, leaf, 4.0, 13.0, 0.68, 1.12));
    }
    meshes::merged(parts)
}

/// A conifer: four stacked eight-sided cones on a trunk, each tier darker at its skirt;
/// `snow` adds white caps on the upper part of every tier. About 18 m tall at scale 1.
fn pine_tree(needle: [f32; 3], snow: bool) -> Mesh {
    let trunk = Cylinder::new(0.5, 4.5).mesh().resolution(6).build().translated_by(Vec3::new(0.0, 2.25, 0.0));
    let mut parts = vec![meshes::tinted(trunk, TRUNK_COLOR, 0.0, 4.0, 0.8, 1.0)];
    // (radius, height, base y)
    let tiers: [(f32, f32, f32); 4] = [(4.0, 6.0, 3.0), (3.3, 5.5, 7.0), (2.5, 5.0, 10.5), (1.6, 4.5, 13.5)];
    for (r, h, y) in tiers {
        let tier = meshes::cone_up(r, h, 8).translated_by(Vec3::new(0.0, y, 0.0));
        parts.push(meshes::tinted(tier, needle, y, y + h, 0.7, 1.08));
    }
    if snow {
        for (r, h, y) in tiers {
            let cap_h = h * 0.55;
            let cap = meshes::cone_up(r * 0.55 + 0.06, cap_h + 0.05, 8).translated_by(Vec3::new(0.0, y + h - cap_h, 0.0));
            parts.push(meshes::tinted(cap, [0.95, 0.96, 1.0], 0.0, 1.0, 1.0, 1.0));
        }
    }
    meshes::merged(parts)
}

pub(crate) fn spawn_tree(commands: &mut Commands, lib: &SceneryLib, pos: Vec3, scale: f32, pine: bool) {
    spawn_tree_biome(commands, lib, pos, scale, pine, false);
}

/// One entity per tree; the variant and the yaw come from the position so the forest is
/// stable between runs.
pub(crate) fn spawn_tree_biome(commands: &mut Commands, lib: &SceneryLib, pos: Vec3, scale: f32, pine: bool, snow: bool) {
    let pick = hash01((pos.x * 7.0) as i32, (pos.z * 7.0) as i32, 77);
    let spin = hash01((pos.z * 3.0) as i32, (pos.x * 3.0) as i32, 78);
    let set = if pine {
        if snow { &lib.snow_pines } else { &lib.pines }
    } else {
        &lib.trees
    };
    // Mostly green and olive; the rusty one is the odd tree out.
    let k = if pick < 0.45 { 0 } else if pick < 0.88 { 1 } else { 2 };
    commands.spawn((
        Mesh3d(set[k.min(set.len() - 1)].clone()),
        MeshMaterial3d(lib.m_veg.clone()),
        Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(spin * std::f32::consts::TAU)).with_scale(Vec3::splat(scale)),
        WorldEntity,
    ));
}

pub const PALM_HEIGHT: f32 = 10.0;

/// A palm: leaning slim trunk and seven arched fronds (two flattened segments each, the
/// inner rising and the outer drooping) around a dark heart. About 10 m tall at scale 1.
fn palm_tree() -> Mesh {
    let lean = 0.10;
    let trunk = Cylinder::new(0.35, 9.0).mesh().resolution(6).build().translated_by(Vec3::new(0.0, 4.5, 0.0)).rotated_by(Quat::from_rotation_z(lean));
    let mut parts = vec![meshes::tinted(trunk, [0.50, 0.38, 0.22], 0.0, 9.0, 0.85, 1.0)];
    let top = Vec3::new(-9.0 * lean.sin(), 9.0 * lean.cos(), 0.0);
    let frond_color = [0.24, 0.48, 0.20];
    for k in 0..7 {
        let ang = k as f32 * std::f32::consts::TAU / 7.0 + 0.3;
        let inner = Sphere::new(1.0).mesh().ico(1).unwrap().scaled_by(Vec3::new(1.5, 0.2, 0.6)).translated_by(Vec3::new(1.4, 0.0, 0.0)).rotated_by(Quat::from_rotation_z(0.25));
        let outer = Sphere::new(1.0).mesh().ico(1).unwrap().scaled_by(Vec3::new(1.8, 0.16, 0.5)).rotated_by(Quat::from_rotation_z(-0.35)).translated_by(Vec3::new(4.41, 0.14, 0.0));
        for seg in [inner, outer] {
            let frond = seg.rotated_by(Quat::from_rotation_y(ang)).translated_by(top);
            parts.push(meshes::tinted(frond, frond_color, top.y - 1.5, top.y + 1.2, 0.8, 1.05));
        }
    }
    let heart = Sphere::new(0.9).mesh().ico(1).unwrap().translated_by(top + Vec3::Y * 0.2);
    parts.push(meshes::tinted(heart, [0.34, 0.26, 0.14], 0.0, 1.0, 1.0, 1.0));
    meshes::merged(parts)
}

pub(crate) fn spawn_palm(commands: &mut Commands, lib: &SceneryLib, pos: Vec3) {
    let spin = hash01((pos.z * 3.0) as i32, (pos.x * 3.0) as i32, 79);
    let s = 0.85 + 0.35 * hash01((pos.x * 5.0) as i32, (pos.z * 5.0) as i32, 80);
    commands.spawn((
        Mesh3d(lib.palm.clone()),
        MeshMaterial3d(lib.m_veg.clone()),
        Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(spin * std::f32::consts::TAU)).with_scale(Vec3::splat(s)),
        WorldEntity,
    ));
}

fn spawn_airport(
    commands: &mut Commands,
    mesh_assets: &mut Assets<Mesh>,
    _materials: &mut Assets<StandardMaterial>,
    lib: &mut SceneryLib,
    a: &Airport,
    terrain: &Terrain,
    rng: &mut Rng,
) {
    let yaw = a.yaw();
    let l = a.def.length;
    let w = a.def.width;
    // Runway slab.
    let slab = mesh_assets.add(Cuboid::new(w, 0.4, l));
    commands.spawn((
        Mesh3d(slab),
        MeshMaterial3d(lib.m_runway.clone()),
        Transform { translation: a.world3(0.0, 0.0, 0.0), rotation: Quat::from_rotation_y(yaw), scale: Vec3::ONE },
        WorldEntity,
    ));
    // Edge lines, centerline dashes and threshold bars.
    let edge = mesh_assets.add(Cuboid::new(0.7, 0.06, l - 4.0));
    for side in [-1.0, 1.0] {
        commands.spawn((
            Mesh3d(edge.clone()),
            MeshMaterial3d(lib.m_white.clone()),
            Transform { translation: a.world3(0.0, side * (w * 0.5 - 0.8), 0.23), rotation: Quat::from_rotation_y(yaw), scale: Vec3::ONE },
            WorldEntity,
        ));
    }
    let dash = mesh_assets.add(Cuboid::new(0.9, 0.06, 12.0));
    let mut u = -l * 0.5 + 40.0;
    while u < l * 0.5 - 40.0 {
        commands.spawn((
            Mesh3d(dash.clone()),
            MeshMaterial3d(lib.m_white.clone()),
            Transform { translation: a.world3(u, 0.0, 0.23), rotation: Quat::from_rotation_y(yaw), scale: Vec3::ONE },
            WorldEntity,
        ));
        u += 30.0;
    }
    let bar = mesh_assets.add(Cuboid::new(w * 0.8, 0.06, 4.0));
    for end in [-1.0, 1.0] {
        commands.spawn((
            Mesh3d(bar.clone()),
            MeshMaterial3d(lib.m_white.clone()),
            Transform { translation: a.world3(end * (l * 0.5 - 12.0), 0.0, 0.23), rotation: Quat::from_rotation_y(yaw), scale: Vec3::ONE },
            WorldEntity,
        ));
    }
    // Apron on the right side.
    let apron = mesh_assets.add(Cuboid::new(70.0, 0.3, 120.0));
    commands.spawn((
        Mesh3d(apron),
        MeshMaterial3d(lib.m_concrete.clone()),
        Transform { translation: a.world3(-l * 0.5 + 140.0, w * 0.5 + 45.0, -0.05), rotation: Quat::from_rotation_y(yaw), scale: Vec3::ONE },
        WorldEntity,
    ));
    // Windsock pole.
    spawn_box(commands, lib, &lib.m_white, a.world3(l * 0.5 - 30.0, w * 0.5 + 25.0, 0.0), Vec3::new(0.3, 8.0, 0.3), yaw);
    spawn_box(commands, lib, &lib.m_red, a.world3(l * 0.5 - 30.0, w * 0.5 + 25.0, 7.0), Vec3::new(3.0, 1.0, 1.0), yaw);
    // Tower.
    let tw = a.world3(-l * 0.5 + 60.0, w * 0.5 + 60.0, 0.0);
    spawn_box(commands, lib, &lib.m_concrete, tw, Vec3::new(5.0, 12.0, 5.0), yaw);
    spawn_box(commands, lib, &lib.m_glass, tw + Vec3::new(0.0, 12.0, 0.0), Vec3::new(7.0, 3.0, 7.0), yaw);
    // Radio mast with red and white bands.
    let mp = a.world3(-l * 0.5 + 30.0, w * 0.5 + 80.0, 0.0);
    for k in 0..6 {
        let m = if k % 2 == 0 { &lib.m_red } else { &lib.m_white };
        spawn_box(commands, lib, m, mp + Vec3::new(0.0, k as f32 * 5.0, 0.0), Vec3::new(0.8, 5.0, 0.8), yaw);
    }
    // Traffic cones along the apron edge and a few crates.
    for k in 0..5 {
        let cp = a.world3(-l * 0.5 + 90.0 + k as f32 * 22.0, w * 0.5 + 12.0, 0.0);
        commands.spawn((
            Mesh3d(lib.cone_small.clone()),
            MeshMaterial3d(lib.m_orange.clone()),
            Transform::from_translation(cp),
            WorldEntity,
        ));
    }
    for k in 0..3 {
        spawn_box(commands, lib, &lib.m_brown, a.world3(-l * 0.5 + 170.0 + k as f32 * 3.0, w * 0.5 + 30.0 + (k % 2) as f32 * 2.5, 0.0), Vec3::new(2.0, 2.0, 2.0), yaw + k as f32 * 0.3);
    }

    let kind = a.def.kind;
    match kind {
        AirportKind::Home => {
            // The hangar: floor slab, back wall, roof on pillars, open toward the runway.
            let hc = a.hangar_center();
            let hy = a.hangar_yaw();
            let fwd = -a.right(); // toward runway
            let back = Vec3::new(-fwd.x, 0.0, -fwd.y);
            let side = Vec3::new(a.dir.x, 0.0, a.dir.y);
            spawn_box(commands, lib, &lib.m_concrete, hc + Vec3::new(0.0, -0.1, 0.0), Vec3::new(44.0, 0.3, 44.0), hy);
            spawn_box(commands, lib, &lib.m_grey, hc + back * 21.0, Vec3::new(44.0, 14.0, 1.0), hy);
            spawn_box(commands, lib, &lib.m_grey, hc + side * 21.5, Vec3::new(1.0, 14.0, 44.0), hy);
            spawn_box(commands, lib, &lib.m_grey, hc - side * 21.5, Vec3::new(1.0, 14.0, 44.0), hy);
            spawn_box(commands, lib, &lib.m_red, hc + Vec3::new(0.0, 14.0, 0.0), Vec3::new(46.0, 1.0, 46.0), hy);
            for s in [-1.0, 1.0] {
                spawn_box(commands, lib, &lib.m_grey, hc + side * (s * 20.5) - back * 21.0, Vec3::new(1.2, 14.0, 1.2), hy);
            }
            // Work lights so the builder is not in the dark.
            for s in [-1.0, 1.0] {
                commands.spawn((
                    PointLight { intensity: 6_000_000.0, range: 70.0, shadows_enabled: false, color: Color::srgb(1.0, 0.97, 0.9), ..default() },
                    Transform::from_translation(hc + side * (s * 10.0) + Vec3::new(0.0, 12.5, 0.0)),
                    WorldEntity,
                ));
            }
            // Fuel depot and crates.
            spawn_box(commands, lib, &lib.m_yellow, a.world3(-l * 0.5 + 200.0, w * 0.5 + 40.0, 0.0), Vec3::new(4.0, 4.0, 4.0), yaw);
            spawn_box(commands, lib, &lib.m_yellow, a.world3(-l * 0.5 + 206.0, w * 0.5 + 40.0, 0.0), Vec3::new(4.0, 4.0, 4.0), yaw);
            // Harbor warehouses.
            for k in 0..3 {
                spawn_box(commands, lib, &lib.m_blue, a.world3(-l * 0.5 + 240.0 + k as f32 * 30.0, w * 0.5 + 90.0, 0.0), Vec3::new(20.0, 9.0, 24.0), yaw);
            }
        }
        _ => {}
    }
    let mut cx = crate::settle::Ctx { commands, meshes: mesh_assets, lib, terrain, rng };
    crate::settle::spawn_airfield(&mut cx, a);
    crate::settle::spawn_settlement(&mut cx, a);
}

/// Solid scenery the plane can hit: trees, rocks (vertical cylinders) and buildings
/// (yawed boxes), bucketed into 64 m cells. The name is historical: it started with trees.
#[derive(Resource, Default)]
pub struct TreeColliders {
    cells: std::collections::HashMap<(i32, i32), Vec<(Vec2, f32, f32, f32, &'static str)>>,
    /// (bottom centre, size, yaw, what) per cell the footprint touches.
    boxes: std::collections::HashMap<(i32, i32), Vec<(Vec3, Vec3, f32, &'static str)>>,
}

const TREE_CELL: f32 = 64.0;

impl TreeColliders {
    fn add(&mut self, xz: Vec2, radius: f32, base: f32, top: f32, what: &'static str) {
        let key = ((xz.x / TREE_CELL).floor() as i32, (xz.y / TREE_CELL).floor() as i32);
        self.cells.entry(key).or_default().push((xz, radius, base, top, what));
    }

    fn add_box(&mut self, bottom: Vec3, size: Vec3, yaw: f32, what: &'static str) {
        let reach = (size.x.max(size.z)) * 0.75;
        let (x0, x1) = (((bottom.x - reach) / TREE_CELL).floor() as i32, ((bottom.x + reach) / TREE_CELL).floor() as i32);
        let (z0, z1) = (((bottom.z - reach) / TREE_CELL).floor() as i32, ((bottom.z + reach) / TREE_CELL).floor() as i32);
        for cx in x0..=x1 {
            for cz in z0..=z1 {
                self.boxes.entry((cx, cz)).or_default().push((bottom, size, yaw, what));
            }
        }
    }

    /// What `p` is inside of, if anything ("a tree", "a rock", "a building", ...).
    pub fn hit(&self, p: Vec3) -> Option<&'static str> {
        let cx = (p.x / TREE_CELL).floor() as i32;
        let cz = (p.z / TREE_CELL).floor() as i32;
        let pxz = Vec2::new(p.x, p.z);
        for dx in -1..=1 {
            for dz in -1..=1 {
                if let Some(list) = self.cells.get(&(cx + dx, cz + dz)) {
                    for (xz, r, base, top, what) in list {
                        if p.y > *base && p.y < *top && (pxz - *xz).length() < *r {
                            return Some(what);
                        }
                    }
                }
            }
        }
        if let Some(list) = self.boxes.get(&(cx, cz)) {
            for (bottom, size, yaw, what) in list {
                if p.y < bottom.y || p.y > bottom.y + size.y {
                    continue;
                }
                let d = Quat::from_rotation_y(-*yaw) * (p - *bottom);
                if d.x.abs() < size.x * 0.5 && d.z.abs() < size.z * 0.5 {
                    return Some(what);
                }
            }
        }
        None
    }
}

fn spawn_vegetation(commands: &mut Commands, lib: &SceneryLib, terrain: &Terrain, rng: &mut Rng) {
    let mut colliders = lib.colliders.borrow_mut();
    for island in ISLANDS {
        let (count, pine, cactus, palms) = match island.biome {
            Biome::Temperate => ((island.radius / 2900.0 * 7000.0) as usize, false, false, false),
            Biome::Snow => (3000, true, false, false),
            Biome::Desert => (260, false, true, false),
            Biome::Tropical => ((island.radius / 1400.0 * 900.0) as usize, false, false, true),
            Biome::Volcanic => (200, true, false, false),
        };
        let mut placed = 0;
        let mut attempts = 0;
        // Trees come in clumps: pick a centre, then scatter a few around it.
        let mut clump = Vec2::ZERO;
        let mut clump_left = 0;
        while placed < count && attempts < count * 8 {
            attempts += 1;
            if clump_left == 0 {
                let ang = rng.range(0.0, std::f32::consts::TAU);
                let r = island.radius * rng.f32().sqrt();
                clump = island.center + Vec2::new(ang.cos() * r, ang.sin() * r);
                clump_left = 6 + (rng.f32() * 16.0) as usize;
            }
            clump_left -= 1;
            let x = clump.x + rng.range(-90.0, 90.0);
            let z = clump.y + rng.range(-90.0, 90.0);
            let h = terrain.height(x, z);
            if h < 5.0 || h > 230.0 {
                continue;
            }
            let slope = (terrain.height(x + 12.0, z) - h).abs().max((terrain.height(x, z + 12.0) - h).abs());
            if slope > 4.5 {
                continue;
            }
            if terrain.airports.iter().any(|a| {
                let l = a.local(Vec2::new(x, z));
                l.x.abs() < a.def.length * 0.5 + 120.0 && l.y.abs() < a.def.width * 0.5 + 220.0
            }) || terrain.on_pad(x, z, 30.0) {
                continue;
            }
            // Keep the climb-out and approach lanes clear: trees are solid now.
            if AIRPORTS.iter().any(|a| corridor_dist(a, Vec2::new(x, z)) < 200.0) {
                continue;
            }
            let p = Vec3::new(x, h - 0.3, z);
            if palms {
                spawn_palm(commands, lib, p);
                colliders.add(Vec2::new(x, z), 1.2, h, h + PALM_HEIGHT, "a palm");
            } else if cactus {
                let s = rng.range(0.6, 1.1);
                commands.spawn((
                    Mesh3d(lib.cactus.clone()),
                    MeshMaterial3d(lib.m_cactus.clone()),
                    Transform::from_translation(p + Vec3::new(0.0, 2.5, 0.0)).with_scale(Vec3::splat(s)),
                    WorldEntity,
                ));
                colliders.add(Vec2::new(x, z), 0.7 * s, h, h + 5.0 * s, "a cactus");
            } else {
                let use_pine = pine || rng.f32() < 0.6;
                let s = if island.biome == Biome::Snow { rng.range(0.9, 1.6) } else { rng.range(0.75, 1.35) };
                spawn_tree_biome(commands, lib, p, s, use_pine, island.biome == Biome::Snow);
                if use_pine {
                    colliders.add(Vec2::new(x, z), PINE_RADIUS * s, h, h + PINE_HEIGHT * s, "a tree");
                } else {
                    colliders.add(Vec2::new(x, z), TREE_RADIUS * s, h, h + TREE_HEIGHT * s, "a tree");
                }
            }
            placed += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn obstacles_hit_boxes_and_cylinders() {
        let mut c = TreeColliders::default();
        c.add(Vec2::new(10.0, 10.0), 2.0, 5.0, 12.0, "a tree");
        c.add_box(Vec3::new(100.0, 5.0, 100.0), Vec3::new(20.0, 6.0, 8.0), std::f32::consts::FRAC_PI_2, "a building");
        assert_eq!(c.hit(Vec3::new(11.0, 8.0, 10.0)), Some("a tree"));
        assert_eq!(c.hit(Vec3::new(11.0, 13.0, 10.0)), None);
        // Rotated 90 degrees: the 20 m side now runs along z.
        assert_eq!(c.hit(Vec3::new(100.0, 8.0, 108.0)), Some("a building"));
        assert_eq!(c.hit(Vec3::new(108.0, 8.0, 100.0)), None);
        assert_eq!(c.hit(Vec3::new(100.0, 12.0, 100.0)), None);
    }
    use super::*;
    use crate::missions::all;

    /// Every mission ships a good its pickup produces to a place that needs it, so the
    /// map's produce/need icons never lie about where a job comes from.
    #[test]
    fn missions_follow_supply_and_demand() {
        for m in all() {
            let from = AIRPORTS.iter().find(|a| a.id == m.from).expect("unknown pickup airport");
            let to = AIRPORTS.iter().find(|a| a.id == m.to).expect("unknown delivery airport");
            assert!(from.produces.contains(&m.cargo), "{}: {} does not produce {:?}", m.id, m.from, m.cargo);
            assert!(to.needs.contains(&m.cargo), "{}: {} does not need {:?}", m.id, m.to, m.cargo);
        }
    }

    #[test]
    fn height_matches_mesh_triangles() {
        let t = Terrain::generate();
        // Grid corners are exact, and points on a cell's diagonal agree with both triangles.
        let (i, j) = (600usize, 640usize);
        let x = -WORLD_HALF + i as f32 * FINE;
        let z = -WORLD_HALF + j as f32 * FINE;
        assert!((t.height(x, z) - t.fine_sample(i, j)).abs() < 1e-3);
        let (a, b, c, d) = (t.fine_sample(i, j), t.fine_sample(i + 1, j), t.fine_sample(i, j + 1), t.fine_sample(i + 1, j + 1));
        let mid = t.height(x + FINE * 0.5, z + FINE * 0.5);
        let expect = if t.diagonal_bc(i / 2, j / 2) { (b + c) * 0.5 } else { (a + d) * 0.5 };
        assert!((mid - expect).abs() < 1e-3, "mid {mid} vs {expect}");
    }
}
