//! Cargo types and the campaign mission list.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CargoKind {
    Supplies,
    Apples,
    Mail,
    Medicine,
    Lumber,
    Glassware,
    Passengers,
    Fish,
    Scrap,
    FuelDrums,
    Water,
    Machinery,
    Ore,
}

impl CargoKind {
    pub fn name(self) -> &'static str {
        match self {
            CargoKind::Supplies => "Supplies",
            CargoKind::Apples => "Apples",
            CargoKind::Mail => "Mail",
            CargoKind::Medicine => "Medicine",
            CargoKind::Lumber => "Lumber",
            CargoKind::Glassware => "Lab Glassware",
            CargoKind::Passengers => "Passengers",
            CargoKind::Fish => "Fresh Fish",
            CargoKind::Scrap => "Salvage",
            CargoKind::FuelDrums => "Fuel Drums",
            CargoKind::Water => "Water",
            CargoKind::Machinery => "Machinery",
            CargoKind::Ore => "Iron Ore",
        }
    }

    /// Mass per unit (crate or person) in kg.
    pub fn mass(self) -> f32 {
        match self {
            CargoKind::Supplies => 45.0,
            CargoKind::Apples => 35.0,
            CargoKind::Mail => 20.0,
            CargoKind::Medicine => 25.0,
            CargoKind::Lumber => 70.0,
            CargoKind::Glassware => 30.0,
            CargoKind::Passengers => 85.0,
            CargoKind::Fish => 40.0,
            CargoKind::Scrap => 80.0,
            CargoKind::FuelDrums => 95.0,
            CargoKind::Water => 60.0,
            CargoKind::Machinery => 130.0,
            CargoKind::Ore => 120.0,
        }
    }

    pub fn is_passenger(self) -> bool {
        self == CargoKind::Passengers
    }

    pub fn unit(self) -> &'static str {
        if self.is_passenger() { "passenger" } else { "crate" }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MissionDef {
    pub id: &'static str,
    pub title: &'static str,
    pub story: &'static str,
    pub cargo: CargoKind,
    pub count: u32,
    pub from: &'static str,
    pub to: &'static str,
    /// Seconds allowed between loading a unit and delivering it.
    pub time_limit: Option<f32>,
    /// Fragile cargo breaks on touchdowns harder than [`FRAGILE_MAX_IMPACT`].
    pub fragile: bool,
    pub reward: i64,
    pub rp: u32,
    pub requires: &'static [&'static str],
    /// Generated supply contract (needs both airports discovered) rather than a story mission.
    pub contract: bool,
}

/// Touchdown sink rate (m/s) below which fragile cargo is safe...
pub const FRAGILE_MAX_IMPACT: f32 = 3.5;
/// ... and above which all of it breaks; in between a growing share does.
pub const FRAGILE_ALL_BREAK: f32 = 7.0;

pub static MISSIONS: &[MissionDef] = &[
    MissionDef {
        id: "first_flight",
        title: "First Flight",
        story: "Harbor Field's chief mechanic wants proof you can fly. Collect the week's mail from Meadowvale and bring it back here.",
        cargo: CargoKind::Mail,
        count: 1,
        from: "meadowvale",
        to: "harbor",
        time_limit: None,
        fragile: false,
        reward: 350,
        rp: 1,
        requires: &[],
        contract: false,
    },
    MissionDef {
        id: "grocery_run",
        title: "Grocery Run",
        story: "The Meadowvale general store is out of everything. Collect two crates of supplies at Northwood Farm and fly them over.",
        cargo: CargoKind::Supplies,
        count: 2,
        from: "northwood",
        to: "meadowvale",
        time_limit: None,
        fragile: false,
        reward: 450,
        rp: 1,
        requires: &["first_flight"],
        contract: false,
    },
    MissionDef {
        id: "apple_harvest",
        title: "Apple Harvest",
        story: "Northwood Farm's apples bruise fast. Each crate must reach the harbor market within 85 seconds of loading.",
        cargo: CargoKind::Apples,
        count: 3,
        from: "northwood",
        to: "harbor",
        time_limit: Some(85.0),
        fragile: false,
        reward: 650,
        rp: 2,
        requires: &["first_flight"],
        contract: false,
    },
    MissionDef {
        id: "mail_route",
        title: "Mail Route",
        story: "The lighthouse keeper has not had post in weeks. Fly the mail bags from Meadowvale to Lighthouse Point.",
        cargo: CargoKind::Mail,
        count: 2,
        from: "meadowvale",
        to: "lighthouse",
        time_limit: None,
        fragile: false,
        reward: 500,
        rp: 1,
        requires: &["grocery_run"],
        contract: false,
    },
    MissionDef {
        id: "medical_emergency",
        title: "Medical Emergency",
        story: "Lighthouse Point radios in: the keeper was bitten by a snake. Collect antivenom at St. Mary's Hospital. One hundred seconds per crate once loaded.",
        cargo: CargoKind::Medicine,
        count: 2,
        from: "stmarys",
        to: "lighthouse",
        time_limit: Some(100.0),
        fragile: false,
        reward: 800,
        rp: 2,
        requires: &["grocery_run"],
        contract: false,
    },
    MissionDef {
        id: "building_materials",
        title: "Building Materials",
        story: "A spring storm tore roofs off half of Meadowvale. The farm's sawmill has lumber. Haul it over.",
        cargo: CargoKind::Lumber,
        count: 6,
        from: "northwood",
        to: "meadowvale",
        time_limit: None,
        fragile: false,
        reward: 1200,
        rp: 3,
        requires: &["apple_harvest"],
        contract: false,
    },
    MissionDef {
        id: "lab_glassware",
        title: "Fragile: Lab Glassware",
        story: "Replacement glassware for the hospital lab is waiting at the Meadowvale glassworks. It shatters on hard landings: touch down under 2.5 m/s.",
        cargo: CargoKind::Glassware,
        count: 3,
        from: "meadowvale",
        to: "stmarys",
        time_limit: None,
        fragile: true,
        reward: 900,
        rp: 3,
        requires: &["medical_emergency"],
        contract: false,
    },
    MissionDef {
        id: "passenger_charter",
        title: "Passenger Charter",
        story: "Two tourists want to see the lighthouse. Passengers need seats: research the Cabin Cockpit or a Passenger Cabin.",
        cargo: CargoKind::Passengers,
        count: 2,
        from: "meadowvale",
        to: "lighthouse",
        time_limit: None,
        fragile: false,
        reward: 1000,
        rp: 3,
        requires: &["mail_route"],
        contract: false,
    },
    MissionDef {
        id: "storm_damage",
        title: "Storm Damage",
        story: "The storm hit Lighthouse Point hardest. Collect eight crates of lumber at the Northwood sawmill to rebuild the pier.",
        cargo: CargoKind::Lumber,
        count: 8,
        from: "northwood",
        to: "lighthouse",
        time_limit: None,
        fragile: false,
        reward: 1500,
        rp: 4,
        requires: &["building_materials"],
        contract: false,
    },
    MissionDef {
        id: "frostholm_relief",
        title: "Cold Front: Frostholm Relief",
        story: "Radio from Frostholm Base, far to the north across open sea: the supply ship is iced in. Collect supplies at Meadowvale. You will need range for this one.",
        cargo: CargoKind::Supplies,
        count: 4,
        from: "meadowvale",
        to: "frostholm",
        time_limit: None,
        fragile: false,
        reward: 2500,
        rp: 6,
        requires: &["storm_damage", "lab_glassware"],
        contract: false,
    },
    MissionDef {
        id: "blizzard_rescue",
        title: "Blizzard Rescue",
        story: "Researchers at Glacier Outpost are snowed in with a sick colleague. Medicine from Frostholm Base, sixty-five seconds per crate.",
        cargo: CargoKind::Medicine,
        count: 2,
        from: "frostholm",
        to: "glacier",
        time_limit: Some(65.0),
        fragile: false,
        reward: 1800,
        rp: 4,
        requires: &["frostholm_relief"],
        contract: false,
    },
    MissionDef {
        id: "frozen_fish",
        title: "Frozen Fish",
        story: "Glacier Outpost's ice fishers have a catch for the harbor restaurants. It must arrive within 140 seconds of loading.",
        cargo: CargoKind::Fish,
        count: 6,
        from: "glacier",
        to: "harbor",
        time_limit: Some(140.0),
        fragile: false,
        reward: 2800,
        rp: 6,
        requires: &["blizzard_rescue"],
        contract: false,
    },
    MissionDef {
        id: "hermit_request",
        title: "The Hermit's Request",
        story: "A hermit on a rock far to the east has sent a bottle with a shopping list. The lighthouse keeper packed it up; collect it there. His strip is short.",
        cargo: CargoKind::Supplies,
        count: 3,
        from: "lighthouse",
        to: "hermit",
        time_limit: None,
        fragile: false,
        reward: 1600,
        rp: 4,
        requires: &["storm_damage"],
        contract: false,
    },
    MissionDef {
        id: "prison_transfer",
        title: "Prison Transfer",
        story: "Gull Rock Prison needs four guards flown out to the west. Collect them at Meadowvale. Seats required.",
        cargo: CargoKind::Passengers,
        count: 4,
        from: "meadowvale",
        to: "gullrock",
        time_limit: None,
        fragile: false,
        reward: 2200,
        rp: 5,
        requires: &["passenger_charter"],
        contract: false,
    },
    MissionDef {
        id: "shipwreck_salvage",
        title: "Shipwreck Salvage",
        story: "A freighter ran aground on Wreck Rock. Its cargo of brass fittings is worth a fortune to the harbor scrapyard. Heavy stuff.",
        cargo: CargoKind::Scrap,
        count: 5,
        from: "wreck",
        to: "harbor",
        time_limit: None,
        fragile: false,
        reward: 2000,
        rp: 5,
        requires: &["hermit_request"],
        contract: false,
    },
    MissionDef {
        id: "desert_expedition",
        title: "Desert Expedition",
        story: "An expedition camped at Oasis Strip on Dunmar, the desert island to the south-east, is out of fuel. Fly drums over.",
        cargo: CargoKind::FuelDrums,
        count: 6,
        from: "harbor",
        to: "oasis",
        time_limit: None,
        fragile: false,
        reward: 4000,
        rp: 8,
        requires: &["frostholm_relief"],
        contract: false,
    },
    MissionDef {
        id: "heatwave",
        title: "Heatwave",
        story: "Dunmar Airbase's water plant failed in a heatwave. Water from the oasis, 70 seconds per load.",
        cargo: CargoKind::Water,
        count: 8,
        from: "oasis",
        to: "dunmar",
        time_limit: Some(70.0),
        fragile: false,
        reward: 3500,
        rp: 8,
        requires: &["desert_expedition"],
        contract: false,
    },
    MissionDef {
        id: "airbase_resupply",
        title: "Airbase Resupply",
        story: "Dunmar Airbase is opening its long runway to civilian traffic if you can bring twelve crates of supplies from the harbor.",
        cargo: CargoKind::Supplies,
        count: 12,
        from: "harbor",
        to: "dunmar",
        time_limit: None,
        fragile: false,
        reward: 6000,
        rp: 12,
        requires: &["heatwave"],
        contract: false,
    },
    MissionDef {
        id: "fragile_instruments",
        title: "Fragile Instruments",
        story: "Frostholm's glaciologists are lending Dunmar their seismographs. Six crates, breakable, across the whole map.",
        cargo: CargoKind::Glassware,
        count: 6,
        from: "frostholm",
        to: "dunmar",
        time_limit: None,
        fragile: true,
        reward: 5500,
        rp: 10,
        requires: &["airbase_resupply", "frozen_fish"],
        contract: false,
    },
    MissionDef {
        id: "big_haul",
        title: "The Big Haul",
        story: "Frostholm is building a geothermal plant. Fifteen crates of machinery, 130 kg each. Build a freighter.",
        cargo: CargoKind::Machinery,
        count: 15,
        from: "harbor",
        to: "frostholm",
        time_limit: None,
        fragile: false,
        reward: 9000,
        rp: 15,
        requires: &["fragile_instruments", "shipwreck_salvage"],
        contract: false,
    },
    MissionDef {
        id: "grand_tour",
        title: "Grand Tour",
        story: "The islands' governor wants a scenic flight: four passengers from Gull Rock Prison all the way to Dunmar Airbase, in one go.",
        cargo: CargoKind::Passengers,
        count: 4,
        from: "gullrock",
        to: "dunmar",
        time_limit: None,
        fragile: false,
        reward: 7000,
        rp: 10,
        requires: &["prison_transfer", "airbase_resupply"],
        contract: false,
    },
    // ------------------------------------------------------------------ 2.0: the outer ring
    MissionDef {
        id: "resort_supplies",
        title: "Resort Supplies",
        story: "Palm Cay's resort opens for the season and needs four crates of supplies from Harbor Field. Nine kilometres of open water: bring fuel.",
        cargo: CargoKind::Supplies,
        count: 4,
        from: "harbor",
        to: "palmcay",
        time_limit: None,
        fragile: false,
        reward: 2600,
        rp: 5,
        requires: &["airbase_resupply"],
        contract: false,
    },
    MissionDef {
        id: "fresh_catch",
        title: "Fresh Catch Express",
        story: "The Coral Atoll fishery lands its catch at dawn and Palm Cay's kitchens want it on the plate. Three crates, 50 seconds each: a job for a very fast plane.",
        cargo: CargoKind::Fish,
        count: 3,
        from: "atoll",
        to: "palmcay",
        time_limit: Some(50.0),
        fragile: false,
        reward: 2400,
        rp: 5,
        requires: &["resort_supplies"],
        contract: false,
    },
    MissionDef {
        id: "observatory_optics",
        title: "Observatory Optics",
        story: "Blackpeak Observatory ordered three crates of precision glassware from Meadowvale. Ten kilometres, and it must arrive in one piece.",
        cargo: CargoKind::Glassware,
        count: 3,
        from: "meadowvale",
        to: "blackpeak",
        time_limit: None,
        fragile: true,
        reward: 3400,
        rp: 7,
        requires: &["fragile_instruments"],
        contract: false,
    },
    MissionDef {
        id: "ore_train",
        title: "Ore Train",
        story: "Ironholm's first ore is ready for the smelters at Harbor Field. Eight crates at 120 kg: this is a freighter's job.",
        cargo: CargoKind::Ore,
        count: 8,
        from: "ironholm",
        to: "harbor",
        time_limit: None,
        fragile: false,
        reward: 4200,
        rp: 8,
        requires: &["big_haul"],
        contract: false,
    },
    MissionDef {
        id: "timber_run",
        title: "Timber Run",
        story: "Marsh Delta is rebuilding its jetties and the Wolds lumber camp has the timber. Six crates, sixteen kilometres across the whole map.",
        cargo: CargoKind::Lumber,
        count: 6,
        from: "sawmill",
        to: "delta",
        time_limit: None,
        fragile: false,
        reward: 5200,
        rp: 9,
        requires: &["ore_train"],
        contract: false,
    },
    MissionDef {
        id: "mine_machinery",
        title: "Machinery for the Mine",
        story: "Ironholm needs three crates of machinery from Cape Wolds Aerodrome. Seventeen kilometres with 130 kg crates: range and load, both.",
        cargo: CargoKind::Machinery,
        count: 3,
        from: "wolds",
        to: "ironholm",
        time_limit: None,
        fragile: false,
        reward: 4800,
        rp: 8,
        requires: &["timber_run"],
        contract: false,
    },
    MissionDef {
        id: "medevac_express",
        title: "Medevac Express",
        story: "A diver at Coral Atoll needs medicine from St. Mary's within 140 seconds of loading. Eleven kilometres: only a fast plane makes it.",
        cargo: CargoKind::Medicine,
        count: 2,
        from: "stmarys",
        to: "atoll",
        time_limit: Some(140.0),
        fragile: false,
        reward: 4500,
        rp: 8,
        requires: &["fresh_catch"],
        contract: false,
    },
    MissionDef {
        id: "island_hopper",
        title: "Island Hopper",
        story: "Four resort guests want the scenic route to Windward Village, sixteen kilometres, in under three minutes and five seconds. Seats and speed.",
        cargo: CargoKind::Passengers,
        count: 4,
        from: "palmcay",
        to: "windward",
        time_limit: Some(185.0),
        fragile: false,
        reward: 5600,
        rp: 9,
        requires: &["medevac_express", "grand_tour"],
        contract: false,
    },
    MissionDef {
        id: "volcano_watch",
        title: "Volcano Watch",
        story: "The observatory's cistern is dry. Five crates of water from Oasis Strip, thirteen kilometres, 60 kg each.",
        cargo: CargoKind::Water,
        count: 5,
        from: "oasis",
        to: "blackpeak",
        time_limit: None,
        fragile: false,
        reward: 4000,
        rp: 7,
        requires: &["observatory_optics"],
        contract: false,
    },
    MissionDef {
        id: "fleet_fuel",
        title: "Fuel for the Fleet",
        story: "The atoll's boats are out of diesel. Five fuel drums from Dunmar Airbase, 95 kg apiece.",
        cargo: CargoKind::FuelDrums,
        count: 5,
        from: "dunmar",
        to: "atoll",
        time_limit: None,
        fragile: false,
        reward: 3600,
        rp: 6,
        requires: &["fresh_catch"],
        contract: false,
    },
    MissionDef {
        id: "rush_order",
        title: "Rush Order",
        story: "The sawmill's blade broke mid-season. Two crates of machinery from Harbor Field within two and a half minutes of loading, twelve kilometres out.",
        cargo: CargoKind::Machinery,
        count: 2,
        from: "harbor",
        to: "sawmill",
        time_limit: Some(150.0),
        fragile: false,
        reward: 5000,
        rp: 9,
        requires: &["mine_machinery"],
        contract: false,
    },
    MissionDef {
        id: "heavy_metal",
        title: "Heavy Metal",
        story: "Twelve crates of iron ore, nearly one and a half tonnes, from Ironholm to Harbor Field. The heaviest load on the board.",
        cargo: CargoKind::Ore,
        count: 12,
        from: "ironholm",
        to: "harbor",
        time_limit: None,
        fragile: false,
        reward: 7500,
        rp: 12,
        requires: &["mine_machinery"],
        contract: false,
    },
    MissionDef {
        id: "cold_chain",
        title: "Cold Chain",
        story: "Marsh Delta's fishers sell to Cape Wolds, sixteen kilometres away, and fish spoils in three minutes. Six crates: fast, far and heavy at once.",
        cargo: CargoKind::Fish,
        count: 6,
        from: "delta",
        to: "wolds",
        time_limit: Some(180.0),
        fragile: false,
        reward: 8000,
        rp: 12,
        requires: &["timber_run", "island_hopper"],
        contract: false,
    },
    MissionDef {
        id: "summit_lens",
        title: "Summit Lens",
        story: "The observatory's great lens, four crates of glassware from Frostholm Base, fourteen kilometres over open sea in two and a half minutes, and it shatters at anything but a gentle touchdown.",
        cargo: CargoKind::Glassware,
        count: 4,
        from: "frostholm",
        to: "blackpeak",
        time_limit: Some(150.0),
        fragile: true,
        reward: 9500,
        rp: 14,
        requires: &["volcano_watch", "heavy_metal"],
        contract: false,
    },
    // ------------------------------------------------------------------ 2.0: hard landings
    MissionDef {
        id: "downtown_express",
        title: "Downtown Express",
        story: "Four executives want Kingsport City in under 145 seconds. The strip sits between office towers and ends at a wall of high-rises: come in slow and stop short.",
        cargo: CargoKind::Passengers,
        count: 4,
        from: "harbor",
        to: "kingsport",
        time_limit: Some(145.0),
        fragile: false,
        reward: 7000,
        rp: 10,
        requires: &["island_hopper"],
        contract: false,
    },
    MissionDef {
        id: "glass_skyline",
        title: "Glass Skyline",
        story: "Five crates of glassware for Kingsport's new gallery. Thread the towers and touch down gently or deliver a box of shards.",
        cargo: CargoKind::Glassware,
        count: 5,
        from: "meadowvale",
        to: "kingsport",
        time_limit: None,
        fragile: true,
        reward: 8200,
        rp: 11,
        requires: &["downtown_express"],
        contract: false,
    },
    MissionDef {
        id: "canyon_run",
        title: "Canyon Run",
        story: "Redgorge Quarry burns diesel faster than the boats bring it. Six drums, eighteen kilometres, and the strip lies at the bottom of a gorge: fly the canyon, do not cross it.",
        cargo: CargoKind::FuelDrums,
        count: 6,
        from: "harbor",
        to: "ravine",
        time_limit: None,
        fragile: false,
        reward: 7800,
        rp: 11,
        requires: &["ore_train"],
        contract: false,
    },
    MissionDef {
        id: "quarry_haul",
        title: "Quarry Haul",
        story: "Ten crates of ore out of the gorge and across the whole map to Kingsport's foundry. A heavy plane that can still climb out between the walls.",
        cargo: CargoKind::Ore,
        count: 10,
        from: "ravine",
        to: "kingsport",
        time_limit: None,
        fragile: false,
        reward: 11000,
        rp: 14,
        requires: &["canyon_run", "downtown_express"],
        contract: false,
    },
    MissionDef {
        id: "relay_repair",
        title: "Relay Repair",
        story: "The Skyreach relay is dark. Two crates of machinery from Harbor Field to a 460 m strip on top of a mesa, with a cliff at both ends.",
        cargo: CargoKind::Machinery,
        count: 2,
        from: "harbor",
        to: "mesa",
        time_limit: None,
        fragile: false,
        reward: 6000,
        rp: 9,
        requires: &["timber_run"],
        contract: false,
    },
    MissionDef {
        id: "mesa_medevac",
        title: "Mesa Medevac",
        story: "A technician on Skyreach is badly hurt. Three crates of medicine from St. Mary's within two and a half minutes, onto the shortest strip on the chart.",
        cargo: CargoKind::Medicine,
        count: 3,
        from: "stmarys",
        to: "mesa",
        time_limit: Some(150.0),
        fragile: false,
        reward: 8500,
        rp: 12,
        requires: &["relay_repair"],
        contract: false,
    },
    MissionDef {
        id: "gorge_rescue",
        title: "Gorge Rescue",
        story: "A rockfall has cut the quarry road. Bring four injured workers out of Redgorge to St. Mary's within 160 seconds: seats, speed, range and a canyon take-off with a full cabin.",
        cargo: CargoKind::Passengers,
        count: 4,
        from: "ravine",
        to: "stmarys",
        time_limit: Some(160.0),
        fragile: false,
        reward: 14000,
        rp: 18,
        requires: &["quarry_haul", "mesa_medevac", "summit_lens", "cold_chain"],
        contract: false,
    },
];

/// Test Pilot time trials: every timed story mission, flown with what the campaign has
/// paid out by then (see `resources_before`).
pub fn timed() -> Vec<&'static MissionDef> {
    MISSIONS.iter().filter(|m| m.time_limit.is_some()).collect()
}

/// Rewards and research points of every story mission listed before `id`: what a player
/// who flew the campaign in order has when this job comes up (parts refund when removed,
/// so money plus the plane's value is the whole payout; fuel is free).
pub fn resources_before(id: &str) -> (i64, u32) {
    let idx = MISSIONS.iter().position(|m| m.id == id).unwrap_or(0);
    MISSIONS[..idx].iter().fold((0, 0), |(money, rp), m| (money + m.reward, rp + m.rp))
}

/// Story missions plus generated supply contracts (built once, deterministic).
pub fn all() -> &'static [MissionDef] {
    static ALL: std::sync::OnceLock<&'static [MissionDef]> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut v: Vec<MissionDef> = MISSIONS.to_vec();
        v.extend(generate_contracts());
        Box::leak(v.into_boxed_slice())
    })
}

pub fn mission(id: &str) -> Option<&'static MissionDef> {
    all().iter().find(|m| m.id == id)
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn hash(s: &str, salt: u32) -> u32 {
    let mut h = 2166136261u32 ^ salt.wrapping_mul(16777619);
    for b in s.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    h
}

/// Supply contracts: for every producer/consumer pair sharing a good, a job whose demands
/// come from the distance and the good (long hauls need fuel, timed ones need speed, heavy
/// ones need capacity, glassware needs a gentle landing). Both airports must be discovered.
pub fn generate_contracts() -> Vec<MissionDef> {
    use crate::world::AIRPORTS;
    let mut out = Vec::new();
    for from in AIRPORTS {
        for to in AIRPORTS {
            if from.id == to.id {
                continue;
            }
            for good in from.produces {
                if !to.needs.contains(good) {
                    continue;
                }
                let key = format!("{}-{}-{:?}", from.id, to.id, good);
                let h = hash(&key, 7);
                // Keep the board readable: about a quarter of the possible pairs.
                if h % 4 != 0 {
                    continue;
                }
                let dist = (from.pos - to.pos).length();
                let km = dist / 1000.0;
                let base_count = 1 + (km / 4.0) as u32 + (h >> 4) % 3;
                let count = if good.is_passenger() { base_count.min(4) } else { base_count.min(8) };
                // Every other contract is timed; the target speed rises with the hash.
                let timed = (h >> 8) % 2 == 0;
                // Timed contracts demand real speed (Adam flew a "four minute" run in 39 s).
                let target = [45.0f32, 60.0, 80.0, 100.0][((h >> 12) % 4) as usize];
                let time_limit = if timed { Some(((dist / target) + 60.0).round()) } else { None };
                let mass = count as f32 * good.mass();
                let fragile = *good == CargoKind::Glassware;
                let mut reward = (km * 0.9 + 0.6) * (1.0 + 0.004 * mass) * 110.0;
                if timed {
                    reward *= 1.4;
                }
                if fragile {
                    reward *= 1.25;
                }
                let rp = 1 + (km / 6.0) as u32 + timed as u32 + fragile as u32;
                let title = leak(format!("{} to {}", good.name(), to.name));
                let story = leak(format!(
                    "{} needs {} {} of {} from {}. {:.1} km{}{}.",
                    to.name,
                    count,
                    if count == 1 { good.unit().to_string() } else { format!("{}s", good.unit()) },
                    good.name(),
                    from.name,
                    km,
                    if timed { ", against the clock" } else { "" },
                    if fragile { ", handle with care" } else { "" }
                ));
                out.push(MissionDef {
                    id: leak(format!("ct_{}_{}_{}", from.id, to.id, good.name().to_lowercase().replace(' ', "_"))),
                    title,
                    story,
                    cargo: *good,
                    count,
                    from: from.id,
                    to: to.id,
                    time_limit,
                    fragile,
                    reward: (reward / 10.0).round() as i64 * 10,
                    rp,
                    requires: &[],
                    contract: true,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contracts_are_well_formed() {
        let cs = generate_contracts();
        assert!(cs.len() >= 12, "only {} contracts", cs.len());
        let mut ids = std::collections::HashSet::new();
        for c in &cs {
            assert!(ids.insert(c.id), "duplicate contract id {}", c.id);
            assert!(c.count >= 1 && c.reward > 0 && c.rp >= 1);
            assert!(crate::world::AIRPORTS.iter().any(|a| a.id == c.from));
            assert!(crate::world::AIRPORTS.iter().any(|a| a.id == c.to));
            println!("{:<40} {:>2} x {:<10} {:>6} km  {:>6} ${:>5} +{} RP", c.title, c.count, c.cargo.name(), "", c.time_limit.map_or("-".to_string(), |t| format!("{t:.0}s")), c.reward, c.rp);
        }
        // Story missions must be unique too and every prerequisite must exist.
        for m in MISSIONS {
            assert!(ids.insert(m.id), "duplicate mission id {}", m.id);
            for r in m.requires {
                assert!(MISSIONS.iter().any(|x| x.id == *r), "{} requires unknown {}", m.id, r);
            }
        }
    }
}
