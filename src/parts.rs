//! Part catalog: every buildable part, its stats, cost, and research requirement.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    PX,
    NX,
    PY,
    NY,
    PZ,
    NZ,
}

#[allow(dead_code)]
impl Dir {
    pub const ALL: [Dir; 6] = [Dir::PX, Dir::NX, Dir::PY, Dir::NY, Dir::PZ, Dir::NZ];

    pub fn vec(self) -> Vec3 {
        match self {
            Dir::PX => Vec3::X,
            Dir::NX => Vec3::NEG_X,
            Dir::PY => Vec3::Y,
            Dir::NY => Vec3::NEG_Y,
            Dir::PZ => Vec3::Z,
            Dir::NZ => Vec3::NEG_Z,
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::PX => Dir::NX,
            Dir::NX => Dir::PX,
            Dir::PY => Dir::NY,
            Dir::NY => Dir::PY,
            Dir::PZ => Dir::NZ,
            Dir::NZ => Dir::PZ,
        }
    }

    /// Nearest axis direction to an arbitrary vector.
    pub fn from_vec(v: Vec3) -> Dir {
        let a = v.abs();
        if a.x >= a.y && a.x >= a.z {
            if v.x >= 0.0 { Dir::PX } else { Dir::NX }
        } else if a.y >= a.z {
            if v.y >= 0.0 { Dir::PY } else { Dir::NY }
        } else if v.z >= 0.0 {
            Dir::PZ
        } else {
            Dir::NZ
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Cockpit,
    Fuselage,
    Wings,
    Control,
    Engines,
    Gear,
    Fuel,
    Utility,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Cockpit,
        Category::Fuselage,
        Category::Wings,
        Category::Control,
        Category::Engines,
        Category::Gear,
        Category::Fuel,
        Category::Utility,
    ];

    #[allow(dead_code)]
    pub fn name(self) -> &'static str {
        match self {
            Category::Cockpit => "Cockpits",
            Category::Fuselage => "Fuselage",
            Category::Wings => "Wings",
            Category::Control => "Tail",
            Category::Engines => "Engines",
            Category::Gear => "Landing Gear",
            Category::Fuel => "Fuel & Power",
            Category::Utility => "Utility",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    None,
    Elevator,
    Aileron,
    Rudder,
    /// Tail surface: acts as an elevator when mounted flat and as a rudder when upright.
    Tail,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PartKind {
    /// `max_speed` (m/s): an open cockpit cannot take jet speeds.
    Cockpit { cargo: u32, seats: u32, fuel: f32, max_speed: f32 },
    /// Box section. `cd_z` is the drag coefficient along the fuselage axis.
    Fuselage { cargo_per_m: f32, seats_per_m: f32, cd_z: f32 },
    /// Tapered box (nose / tail). Its cross-section adapts to the face it is placed on.
    Cone { cd_z: f32, tip_scale: f32 },
    Boom,
    Hub,
    Wing {
        area: f32,
        span: f32,
        cl_max: f32,
        cd0: f32,
        control: Control,
        /// Lifting wing with a built-in aileron (roll control comes with the wing).
        aileron: bool,
        vertical: bool,
        /// Full-span symmetric surface centered on its attach point (tail surfaces).
        centered: bool,
        /// Airspeed (m/s) above which the surface tears off. 0 = no limit.
        max_speed: f32,
    },
    /// `ramjet` engines only make thrust once the plane is already fast.
    Engine { thrust: f32, burn: f32, electric: bool, prop: bool, ramjet: bool },
    Wheel { max_impact: f32, retractable: bool, max_load: f32 },
    Tank { fuel: f32, external: bool },
    Battery { charge: f32 },
    Parachute,
    DragChute,
}

#[derive(Clone, Copy, Debug)]
pub struct PartDef {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub desc: &'static str,
    /// Cost in $. For stretchable parts this is per meter of length.
    pub cost: i64,
    /// Research points needed to unlock (0 = available from the start).
    pub research: u32,
    pub tier: u8,
    /// Mass in kg. For stretchable parts this is per meter of length.
    pub mass: f32,
    /// Local size (width, height, length) in meters.
    pub dims: Vec3,
    /// Allowed stretched length range (min, max) if the part can be stretched.
    pub stretch: Option<(f32, f32)>,
    /// Faces (local directions) by which this part attaches to another part. First is primary.
    pub attach: &'static [Dir],
    pub kind: PartKind,
    /// Default palette color index.
    pub color: usize,
}

impl PartDef {
    pub fn stretchable(&self) -> bool {
        self.stretch.is_some()
    }

    #[allow(dead_code)]
    pub fn is_wing(&self) -> bool {
        matches!(self.kind, PartKind::Wing { .. })
    }

    /// Parts placed on the left side get mirrored geometry instead of rotated geometry.
    pub fn mirrorable(&self) -> bool {
        match self.kind {
            PartKind::Wing { centered, .. } => !centered,
            _ => false,
        }
    }

    /// Adapts its cross-section to the face it is placed on (body segments match the
    /// cockpit or the previous segment; cones taper from it).
    pub fn fits_face(&self) -> bool {
        matches!(self.kind, PartKind::Cone { .. } | PartKind::Fuselage { .. })
    }

    /// Parts that live inside a fuselage section (no drag, hidden, selectable in X-ray).
    pub fn internal(&self) -> bool {
        matches!(self.kind, PartKind::Tank { external: false, .. } | PartKind::Battery { .. })
    }

    /// Parts that can contain internal parts.
    pub fn container(&self) -> bool {
        matches!(self.kind, PartKind::Cockpit { .. } | PartKind::Fuselage { .. })
    }

    pub fn is_wheel(&self) -> bool {
        matches!(self.kind, PartKind::Wheel { .. })
    }

    pub fn is_engine(&self) -> bool {
        matches!(self.kind, PartKind::Engine { .. })
    }

    /// Cost for a placed instance with the given length.
    pub fn cost_for(&self, length: f32) -> i64 {
        if self.stretchable() {
            (self.cost as f32 * length).round() as i64
        } else {
            self.cost
        }
    }

    pub fn mass_for(&self, length: f32) -> f32 {
        if self.stretchable() {
            self.mass * length
        } else {
            self.mass
        }
    }
}

/// The user-selectable part colour palette.
pub const PALETTE: [([f32; 3], &str); 10] = [
    ([0.92, 0.92, 0.88], "White"),
    ([0.85, 0.2, 0.18], "Red"),
    ([0.95, 0.6, 0.1], "Orange"),
    ([0.95, 0.85, 0.2], "Yellow"),
    ([0.25, 0.65, 0.3], "Green"),
    ([0.2, 0.45, 0.85], "Blue"),
    ([0.15, 0.2, 0.35], "Navy"),
    ([0.45, 0.45, 0.48], "Grey"),
    ([0.18, 0.18, 0.2], "Black"),
    ([0.55, 0.38, 0.22], "Brown"),
];

const ALL6: &[Dir] = &[Dir::NZ, Dir::PZ, Dir::NY, Dir::PY, Dir::NX, Dir::PX];
const FRONT_BACK: &[Dir] = &[Dir::NZ, Dir::PZ];
/// Body segments also hang under or beside wings (engine nacelles, side pods).
const BODY_FACES: &[Dir] = &[Dir::NZ, Dir::PZ, Dir::PY, Dir::NY, Dir::PX, Dir::NX];

macro_rules! wing {
    ($area:expr, $span:expr, $clmax:expr, $cd0:expr, $vmax:expr) => {
        PartKind::Wing {
            area: $area,
            span: $span,
            cl_max: $clmax,
            cd0: $cd0,
            control: Control::None,
            aileron: true,
            vertical: false,
            centered: false,
            max_speed: $vmax,
        }
    };
}

pub static PARTS: &[PartDef] = &[
    // ------------------------------------------------------------------ cockpits
    PartDef {
        id: "cockpit_basic",
        name: "Basic Cockpit",
        category: Category::Cockpit,
        desc: "Open cockpit with a windscreen. Cheap and light, but above 210 km/h the pilot cannot hold on.",
        cost: 0,
        research: 0,
        tier: 1,
        mass: 140.0,
        dims: Vec3::new(1.0, 1.0, 2.0),
        stretch: None,
        attach: &[],
        kind: PartKind::Cockpit { cargo: 2, seats: 0, fuel: 8.0, max_speed: 58.0 },
        color: 0,
    },
    PartDef {
        id: "cockpit_closed",
        name: "Closed Cockpit",
        category: Category::Cockpit,
        desc: "Same small nose with a canopy over the pilot. Good to 400 km/h.",
        cost: 350,
        research: 1,
        tier: 2,
        mass: 60.0,
        dims: Vec3::new(1.0, 1.0, 2.2),
        stretch: None,
        attach: &[],
        kind: PartKind::Cockpit { cargo: 2, seats: 0, fuel: 10.0, max_speed: 110.0 },
        color: 0,
    },
    PartDef {
        id: "cockpit_light",
        name: "Cabin Cockpit",
        category: Category::Cockpit,
        desc: "A little wider, room for a passenger beside the pilot and a bit more cargo.",
        cost: 700,
        research: 2,
        tier: 2,
        mass: 80.0,
        dims: Vec3::new(1.25, 1.25, 2.6),
        stretch: None,
        attach: &[],
        kind: PartKind::Cockpit { cargo: 3, seats: 1, fuel: 15.0, max_speed: 130.0 },
        color: 0,
    },
    PartDef {
        id: "cockpit_cabin",
        name: "Cargo Cockpit",
        category: Category::Cockpit,
        desc: "Larger cockpit with a cargo hold behind the seats. Roomier, heavier.",
        cost: 900,
        research: 3,
        tier: 3,
        mass: 260.0,
        dims: Vec3::new(1.5, 1.5, 3.0),
        stretch: None,
        attach: &[],
        kind: PartKind::Cockpit { cargo: 4, seats: 2, fuel: 20.0, max_speed: 150.0 },
        color: 0,
    },
    PartDef {
        id: "cockpit_fast",
        name: "Racer Cockpit",
        category: Category::Cockpit,
        desc: "Slim, streamlined nose for speed. Little room inside, good to 1000 km/h.",
        cost: 1800,
        research: 6,
        tier: 4,
        mass: 110.0,
        dims: Vec3::new(1.2, 1.2, 3.0),
        stretch: None,
        attach: &[],
        kind: PartKind::Cockpit { cargo: 1, seats: 0, fuel: 25.0, max_speed: 720.0 },
        color: 0,
    },
    PartDef {
        id: "cockpit_airliner",
        name: "Jet Cockpit",
        category: Category::Cockpit,
        desc: "Wide-body flight deck: 4 seats, 6 cargo slots, 40 L of fuel.",
        cost: 2500,
        research: 8,
        tier: 5,
        mass: 450.0,
        dims: Vec3::new(2.0, 2.0, 4.0),
        stretch: None,
        attach: &[],
        kind: PartKind::Cockpit { cargo: 6, seats: 4, fuel: 40.0, max_speed: 600.0 },
        color: 0,
    },
    PartDef {
        id: "cockpit_cargo_jet",
        name: "Cargo Jet Cockpit",
        category: Category::Cockpit,
        desc: "Huge nose for the biggest haulers: a full deck of cargo and seats.",
        cost: 4500,
        research: 12,
        tier: 6,
        mass: 420.0,
        dims: Vec3::new(2.5, 2.5, 5.0),
        stretch: None,
        attach: &[],
        kind: PartKind::Cockpit { cargo: 10, seats: 6, fuel: 60.0, max_speed: 450.0 },
        color: 0,
    },
    // ------------------------------------------------------------------ fuselage
    PartDef {
        id: "fus_cargo",
        name: "Body Segment",
        category: Category::Fuselage,
        desc: "Stretchable body. Takes the width of whatever it sits behind; cargo space and weight grow with it.",
        cost: 150,
        research: 0,
        tier: 1,
        mass: 45.0,
        dims: Vec3::new(1.0, 1.0, 2.0),
        stretch: Some((1.0, 6.0)),
        attach: BODY_FACES,
        kind: PartKind::Fuselage { cargo_per_m: 2.0, seats_per_m: 0.0, cd_z: 0.5 },
        color: 0,
    },
    PartDef {
        id: "fus_passenger",
        name: "Cabin Segment",
        category: Category::Fuselage,
        desc: "1.5 m wide cabin with windows. 2 seats and 1 cargo slot per meter.",
        cost: 350,
        research: 5,
        tier: 2,
        mass: 80.0,
        dims: Vec3::new(1.5, 1.5, 2.0),
        stretch: Some((1.0, 8.0)),
        attach: BODY_FACES,
        kind: PartKind::Fuselage { cargo_per_m: 1.0, seats_per_m: 2.0, cd_z: 0.5 },
        color: 0,
    },
    PartDef {
        id: "nose_cone",
        name: "Nose Cone",
        category: Category::Fuselage,
        desc: "Streamlined nose. Cuts frontal drag of the section it is mounted on.",
        cost: 120,
        research: 0,
        tier: 1,
        mass: 25.0,
        dims: Vec3::new(1.0, 1.0, 1.5),
        stretch: None,
        attach: &[Dir::PZ],
        kind: PartKind::Cone { cd_z: 0.10, tip_scale: 0.35 },
        color: 0,
    },
    PartDef {
        id: "tail_cone",
        name: "Tail Start Segment",
        category: Category::Fuselage,
        desc: "Tapered tail section. Mount tail surfaces on its end.",
        cost: 100,
        research: 0,
        tier: 1,
        mass: 12.0,
        dims: Vec3::new(1.0, 1.0, 1.4),
        stretch: None,
        attach: &[Dir::NZ],
        kind: PartKind::Cone { cd_z: 0.10, tip_scale: 0.5 },
        color: 0,
    },
    PartDef {
        id: "neck",
        name: "Neck Segment",
        category: Category::Fuselage,
        desc: "Widens the body behind the cockpit so bigger segments fit.",
        cost: 90,
        research: 0,
        tier: 1,
        mass: 12.0,
        dims: Vec3::new(1.0, 1.0, 0.9),
        stretch: None,
        attach: &[Dir::NZ],
        kind: PartKind::Cone { cd_z: 0.12, tip_scale: 1.3 },
        color: 0,
    },
    PartDef {
        id: "tail_end",
        name: "Tail End Segment",
        category: Category::Fuselage,
        desc: "Pointed end of the tail. Goes after a Tail Start Segment.",
        cost: 80,
        research: 0,
        tier: 1,
        mass: 8.0,
        dims: Vec3::new(1.0, 1.0, 1.2),
        stretch: None,
        attach: &[Dir::NZ],
        kind: PartKind::Cone { cd_z: 0.07, tip_scale: 0.12 },
        color: 0,
    },
    PartDef {
        id: "tail_boom",
        name: "Tail Boom",
        category: Category::Fuselage,
        desc: "Thin stretchable beam for mounting the tail far back.",
        cost: 40,
        research: 1,
        tier: 2,
        mass: 12.0,
        dims: Vec3::new(0.3, 0.3, 3.0),
        stretch: Some((1.0, 6.0)),
        attach: FRONT_BACK,
        kind: PartKind::Boom,
        color: 7,
    },
    PartDef {
        id: "hub",
        name: "Connector Hub",
        category: Category::Fuselage,
        desc: "1 m cube with attachment points on every face.",
        cost: 60,
        research: 0,
        tier: 1,
        mass: 25.0,
        dims: Vec3::new(1.0, 1.0, 1.0),
        stretch: None,
        attach: ALL6,
        kind: PartKind::Hub,
        color: 7,
    },
    PartDef {
        id: "pylon",
        name: "Pylon",
        category: Category::Fuselage,
        desc: "Short strut for hanging engines or tanks under wings.",
        cost: 40,
        research: 1,
        tier: 2,
        mass: 8.0,
        dims: Vec3::new(0.25, 0.5, 0.6),
        stretch: None,
        attach: &[Dir::PY, Dir::NY],
        kind: PartKind::Boom,
        color: 7,
    },
    // ------------------------------------------------------------------ wings
    PartDef {
        id: "wing_biplane",
        name: "Wood Wing",
        category: Category::Wings,
        desc: "Wood and fabric. Light and cheap with plenty of lift, but it tears off above 220 km/h.",
        cost: 120,
        research: 0,
        tier: 1,
        mass: 20.0,
        dims: Vec3::new(2.5, 0.15, 1.4),
        stretch: None,
        attach: &[Dir::NX],
        kind: wing!(3.5, 2.5, 1.6, 0.035, 62.0),
        color: 0,
    },
    PartDef {
        id: "wing_standard",
        name: "Metal Wing",
        category: Category::Wings,
        desc: "Aluminium skin. Heavier than wood but good to 430 km/h.",
        cost: 350,
        research: 2,
        tier: 2,
        mass: 38.0,
        dims: Vec3::new(4.0, 0.18, 1.3),
        stretch: None,
        attach: &[Dir::NX],
        kind: wing!(5.2, 4.0, 1.5, 0.022, 120.0),
        color: 0,
    },
    PartDef {
        id: "wing_highlift",
        name: "Big Wing",
        category: Category::Wings,
        desc: "Broad metal wing with lots of lift for heavy loads and short strips. Good to 485 km/h.",
        cost: 550,
        research: 3,
        tier: 3,
        mass: 55.0,
        dims: Vec3::new(4.5, 0.22, 1.8),
        stretch: None,
        attach: &[Dir::NX],
        kind: wing!(8.1, 4.5, 1.9, 0.03, 135.0),
        color: 0,
    },
    PartDef {
        id: "wing_glider",
        name: "Glider Wing",
        category: Category::Wings,
        desc: "Long slender wing. Superb glide ratio for long-range economy.",
        cost: 800,
        research: 5,
        tier: 4,
        mass: 60.0,
        dims: Vec3::new(7.0, 0.18, 1.1),
        stretch: None,
        attach: &[Dir::NX],
        kind: wing!(7.7, 7.0, 1.4, 0.015, 95.0),
        color: 0,
    },
    PartDef {
        id: "wing_airliner",
        name: "Airliner Wing",
        category: Category::Wings,
        desc: "Big wing for big planes.",
        cost: 1800,
        research: 8,
        tier: 5,
        mass: 210.0,
        dims: Vec3::new(9.0, 0.35, 2.6),
        stretch: None,
        attach: &[Dir::NX],
        kind: wing!(23.4, 9.0, 1.6, 0.02, 290.0),
        color: 0,
    },
    PartDef {
        id: "wing_heavy",
        name: "Heavy-Lift Wing",
        category: Category::Wings,
        desc: "Enormous cargo-hauler wing. Bring engines.",
        cost: 3200,
        research: 12,
        tier: 6,
        mass: 380.0,
        dims: Vec3::new(12.0, 0.45, 3.2),
        stretch: None,
        attach: &[Dir::NX],
        kind: wing!(38.0, 12.0, 1.8, 0.022, 190.0),
        color: 0,
    },
    PartDef {
        id: "wing_jet",
        name: "Jet Wing",
        category: Category::Wings,
        desc: "Short, thin metal wing built for speed. Little lift at low speed, good to 1000 km/h.",
        cost: 1400,
        research: 6,
        tier: 4,
        mass: 70.0,
        dims: Vec3::new(3.6, 0.12, 1.6),
        stretch: None,
        attach: &[Dir::NX],
        kind: wing!(6.0, 3.6, 1.25, 0.012, 520.0),
        color: 0,
    },
    PartDef {
        id: "wing_jet_big",
        name: "Big Jet Wing",
        category: Category::Wings,
        desc: "Airliner-grade jet wing: lots of span, still slippery, good to 1000 km/h.",
        cost: 3600,
        research: 11,
        tier: 5,
        mass: 210.0,
        dims: Vec3::new(7.5, 0.3, 2.6),
        stretch: None,
        attach: &[Dir::NX],
        kind: wing!(19.5, 7.5, 1.4, 0.014, 700.0),
        color: 0,
    },
    // ------------------------------------------------------------------ tail (control surfaces are built into wings, tail wings and fins)
    PartDef {
        id: "elevator",
        name: "Small Tailwing",
        category: Category::Control,
        desc: "Half a tail surface. Mirror one onto each side of the tail for the elevators, stand one on top for the fin.",
        cost: 80,
        research: 0,
        tier: 1,
        mass: 7.0,
        dims: Vec3::new(1.2, 0.08, 0.7),
        stretch: None,
        attach: &[Dir::NX],
        kind: PartKind::Wing {
            area: 0.85,
            span: 1.2,
            cl_max: 1.2,
            cd0: 0.02,
            control: Control::Tail,
            aileron: false,
            vertical: false,
            centered: false,
            max_speed: 115.0,
        },
        color: 0,
    },
    PartDef {
        id: "elevator_large",
        name: "Medium Tailwing",
        category: Category::Control,
        desc: "Medium tail panel for bigger planes. Sides = elevators, on top = rudder.",
        cost: 220,
        research: 4,
        tier: 2,
        mass: 20.0,
        dims: Vec3::new(2.5, 0.12, 1.2),
        stretch: None,
        attach: &[Dir::NX],
        kind: PartKind::Wing {
            area: 2.1,
            span: 2.5,
            cl_max: 1.2,
            cd0: 0.02,
            control: Control::Tail,
            aileron: false,
            vertical: false,
            centered: false,
            max_speed: 200.0,
        },
        color: 0,
    },
    PartDef {
        id: "tail_racing",
        name: "Racing Tailwing",
        category: Category::Control,
        desc: "Small, stiff tail panel for racers and fighters: the size of the Small Tailwing, good to 1900 km/h. Sides = elevators, on top = rudder.",
        cost: 320,
        research: 5,
        tier: 3,
        mass: 9.0,
        dims: Vec3::new(1.3, 0.06, 0.75),
        stretch: None,
        attach: &[Dir::NX],
        kind: PartKind::Wing {
            area: 0.9,
            span: 1.3,
            cl_max: 1.1,
            cd0: 0.015,
            control: Control::Tail,
            aileron: false,
            vertical: false,
            centered: false,
            max_speed: 540.0,
        },
        color: 0,
    },
    PartDef {
        id: "tail_big",
        name: "Big Tailwing",
        category: Category::Control,
        desc: "Big tail panel for airliners and heavy haulers. Sides = elevators, on top = rudder.",
        cost: 500,
        research: 7,
        tier: 3,
        mass: 40.0,
        dims: Vec3::new(3.5, 0.2, 1.6),
        stretch: None,
        attach: &[Dir::NX],
        kind: PartKind::Wing {
            area: 3.4,
            span: 3.5,
            cl_max: 1.2,
            cd0: 0.02,
            control: Control::Tail,
            aileron: false,
            vertical: false,
            centered: false,
            max_speed: 300.0,
        },
        color: 0,
    },
    PartDef {
        id: "tail_swept",
        name: "Swept Tailwing",
        category: Category::Control,
        desc: "Swept medium tail panel for fast jets: Medium Tailwing authority, good past 2500 km/h. Sides = elevators, on top = rudder.",
        cost: 650,
        research: 7,
        tier: 4,
        mass: 18.0,
        dims: Vec3::new(2.2, 0.1, 1.1),
        stretch: None,
        attach: &[Dir::NX],
        kind: PartKind::Wing {
            area: 1.7,
            span: 2.2,
            cl_max: 1.1,
            cd0: 0.016,
            control: Control::Tail,
            aileron: false,
            vertical: false,
            centered: false,
            max_speed: 720.0,
        },
        color: 0,
    },
    // ------------------------------------------------------------------ engines
    PartDef {
        id: "eng_small",
        name: "Wood Propeller Engine",
        category: Category::Engines,
        desc: "Little piston engine swinging a wooden two-blade prop. Enough to get a light plane off the ground.",
        cost: 300,
        research: 0,
        tier: 1,
        mass: 60.0,
        dims: Vec3::new(0.8, 0.8, 0.9),
        stretch: None,
        attach: &[Dir::PZ, Dir::PY, Dir::NY],
        kind: PartKind::Engine { thrust: 1700.0, burn: 0.13, electric: false, prop: true, ramjet: false },
        color: 7,
    },
    PartDef {
        id: "eng_electric",
        name: "Electric Motor",
        category: Category::Engines,
        desc: "Light and quiet. 0.65 kN. Needs a battery pack instead of fuel.",
        cost: 350,
        research: 2,
        tier: 2,
        mass: 30.0,
        dims: Vec3::new(0.6, 0.6, 0.6),
        stretch: None,
        attach: &[Dir::PZ, Dir::PY, Dir::NY],
        kind: PartKind::Engine { thrust: 650.0, burn: 0.4, electric: true, prop: true, ramjet: false },
        color: 5,
    },
    PartDef {
        id: "eng_electric_big",
        name: "Big Electric Engine",
        category: Category::Engines,
        desc: "Twice the motor, twice the draw. Needs a couple of battery packs.",
        cost: 950,
        research: 5,
        tier: 3,
        mass: 110.0,
        dims: Vec3::new(1.0, 1.0, 1.1),
        stretch: None,
        attach: &[Dir::PZ, Dir::PY, Dir::NY],
        kind: PartKind::Engine { thrust: 1700.0, burn: 0.9, electric: true, prop: true, ramjet: false },
        color: 5,
    },
    PartDef {
        id: "eng_metal_prop",
        name: "Metal Propeller Engine",
        category: Category::Engines,
        desc: "Piston engine with a metal prop and spinner. More thrust than wood, still cheap to run.",
        cost: 520,
        research: 2,
        tier: 2,
        mass: 95.0,
        dims: Vec3::new(1.2, 1.2, 1.0),
        stretch: None,
        attach: &[Dir::PZ, Dir::PY, Dir::NY],
        kind: PartKind::Engine { thrust: 2300.0, burn: 0.18, electric: false, prop: true, ramjet: false },
        color: 7,
    },
    PartDef {
        id: "eng_radial",
        name: "Big Propeller Engine",
        category: Category::Engines,
        desc: "Big radial with a metal prop. Heavy, thirsty, strong.",
        cost: 900,
        research: 4,
        tier: 3,
        mass: 150.0,
        dims: Vec3::new(1.2, 1.2, 1.0),
        stretch: None,
        attach: &[Dir::PZ, Dir::PY, Dir::NY],
        kind: PartKind::Engine { thrust: 3200.0, burn: 0.3, electric: false, prop: true, ramjet: false },
        color: 7,
    },
    PartDef {
        id: "eng_jet_small",
        name: "Small Jet Engine",
        category: Category::Engines,
        desc: "Compact turbojet. Fast and thirsty; no propeller to break.",
        cost: 1800,
        research: 7,
        tier: 4,
        mass: 160.0,
        dims: Vec3::new(0.8, 0.8, 1.8),
        stretch: None,
        attach: &[Dir::PZ, Dir::PY, Dir::NY],
        kind: PartKind::Engine { thrust: 6500.0, burn: 0.65, electric: false, prop: false, ramjet: false },
        color: 7,
    },
    PartDef {
        id: "eng_jet",
        name: "Jet Engine",
        category: Category::Engines,
        desc: "10 kN of pure thrust that does not fade with speed. Drinks 0.55 L/s.",
        cost: 3000,
        research: 10,
        tier: 5,
        mass: 360.0,
        dims: Vec3::new(1.2, 1.2, 2.4),
        stretch: None,
        attach: &[Dir::PZ, Dir::PY, Dir::NY],
        kind: PartKind::Engine { thrust: 12500.0, burn: 1.1, electric: false, prop: false, ramjet: false },
        color: 7,
    },
    PartDef {
        id: "eng_ramjet",
        name: "Ramjet",
        category: Category::Engines,
        desc: "No moving parts: it only pushes once you are already going fast. Get airborne with something else.",
        cost: 6000,
        research: 16,
        tier: 6,
        mass: 360.0,
        dims: Vec3::new(1.2, 1.2, 2.4),
        stretch: None,
        attach: &[Dir::PZ, Dir::PY, Dir::NY],
        kind: PartKind::Engine { thrust: 18000.0, burn: 1.4, electric: false, prop: false, ramjet: true },
        color: 7,
    },
    PartDef {
        id: "eng_rocket",
        name: "Rocket Engine",
        category: Category::Engines,
        desc: "Bell-nozzle rocket. Brutal thrust from a standstill, drinks fuel like nothing else. The most expensive part there is.",
        cost: 12000,
        research: 25,
        tier: 7,
        mass: 240.0,
        dims: Vec3::new(1.2, 1.2, 2.0),
        stretch: None,
        attach: &[Dir::PZ, Dir::PY, Dir::NY],
        kind: PartKind::Engine { thrust: 45000.0, burn: 3.0, electric: false, prop: false, ramjet: false },
        color: 7,
    },
    PartDef {
        id: "eng_fan",
        name: "Fan",
        category: Category::Engines,
        desc: "Ducted propulsion unit. Point it down for vertical take-off, back for a push.",
        cost: 1500,
        research: 6,
        tier: 4,
        mass: 70.0,
        dims: Vec3::new(1.0, 1.0, 0.5),
        stretch: None,
        attach: &[Dir::PZ, Dir::PY, Dir::NY],
        kind: PartKind::Engine { thrust: 3000.0, burn: 0.36, electric: false, prop: false, ramjet: false },
        color: 7,
    },
    // ------------------------------------------------------------------ landing gear
    PartDef {
        id: "wheel_fixed",
        name: "Fixed Wheel",
        category: Category::Gear,
        desc: "Basic landing gear. Survives touchdowns up to 5 m/s, carries 240 kg.",
        cost: 40,
        research: 0,
        tier: 1,
        mass: 14.0,
        dims: Vec3::new(0.3, 0.8, 0.5),
        stretch: None,
        attach: &[Dir::PY],
        kind: PartKind::Wheel { max_impact: 5.0, retractable: false, max_load: 240.0 },
        color: 8,
    },
    PartDef {
        id: "wheel_short",
        name: "Short Wheel",
        category: Category::Gear,
        desc: "Stubby nose or tail wheel on a short strut. Survives 5 m/s, carries 180 kg.",
        cost: 35,
        research: 0,
        tier: 1,
        mass: 11.0,
        dims: Vec3::new(0.3, 0.5, 0.5),
        stretch: None,
        attach: &[Dir::PY],
        kind: PartKind::Wheel { max_impact: 5.0, retractable: false, max_load: 180.0 },
        color: 8,
    },
    PartDef {
        id: "wheel_tall",
        name: "Tall Wheel",
        category: Category::Gear,
        desc: "Wheel on a long strut for hanging under wings. Survives 5 m/s, carries 220 kg.",
        cost: 55,
        research: 0,
        tier: 1,
        mass: 20.0,
        dims: Vec3::new(0.3, 1.4, 0.5),
        stretch: None,
        attach: &[Dir::PY],
        kind: PartKind::Wheel { max_impact: 5.0, retractable: false, max_load: 220.0 },
        color: 8,
    },
    PartDef {
        id: "wheel_tail",
        name: "Tail Wheel",
        category: Category::Gear,
        desc: "Small wheel for tail-dragger layouts. Steers on the ground. Carries 120 kg.",
        cost: 25,
        research: 0,
        tier: 1,
        mass: 6.0,
        dims: Vec3::new(0.2, 0.4, 0.3),
        stretch: None,
        attach: &[Dir::PY],
        kind: PartKind::Wheel { max_impact: 5.0, retractable: false, max_load: 120.0 },
        color: 8,
    },
    PartDef {
        id: "wheel_tough",
        name: "Tough Wheel",
        category: Category::Gear,
        desc: "Reinforced gear for rough strips. Survives 8 m/s, carries 480 kg.",
        cost: 110,
        research: 1,
        tier: 2,
        mass: 26.0,
        dims: Vec3::new(0.4, 1.0, 0.7),
        stretch: None,
        attach: &[Dir::PY],
        kind: PartKind::Wheel { max_impact: 8.0, retractable: false, max_load: 480.0 },
        color: 8,
    },
    PartDef {
        id: "gear_retract",
        name: "Retractable Gear",
        category: Category::Gear,
        desc: "Tucks away in flight (G key) for zero drag. Survives 5 m/s, carries 420 kg.",
        cost: 260,
        research: 4,
        tier: 3,
        mass: 34.0,
        dims: Vec3::new(0.4, 1.0, 0.7),
        stretch: None,
        attach: &[Dir::PY],
        kind: PartKind::Wheel { max_impact: 5.0, retractable: true, max_load: 420.0 },
        color: 8,
    },
    PartDef {
        id: "gear_heavy",
        name: "Heavy Gear",
        category: Category::Gear,
        desc: "Tall double-wheel bogie for heavy freighters. Survives 10 m/s, carries 1300 kg.",
        cost: 400,
        research: 7,
        tier: 4,
        mass: 60.0,
        dims: Vec3::new(0.6, 1.4, 1.0),
        stretch: None,
        attach: &[Dir::PY],
        kind: PartKind::Wheel { max_impact: 10.0, retractable: false, max_load: 1300.0 },
        color: 8,
    },
    // ------------------------------------------------------------------ fuel & power
    PartDef {
        id: "tank_small",
        name: "Fuel Tank",
        category: Category::Fuel,
        desc: "1 m cube holding 25 L. Goes inside a fuselage or cockpit.",
        cost: 100,
        research: 0,
        tier: 1,
        mass: 12.0,
        dims: Vec3::new(0.5, 0.5, 1.6),
        stretch: None,
        attach: ALL6,
        kind: PartKind::Tank { fuel: 25.0, external: false },
        color: 2,
    },
    PartDef {
        id: "tank_external",
        name: "External Fuel Tank",
        category: Category::Fuel,
        desc: "Streamlined 40 L drop tank. Hang it under a wing.",
        cost: 160,
        research: 2,
        tier: 2,
        mass: 18.0,
        dims: Vec3::new(0.5, 0.5, 2.0),
        stretch: None,
        attach: &[Dir::PY, Dir::NY, Dir::PZ, Dir::NZ],
        kind: PartKind::Tank { fuel: 40.0, external: true },
        color: 7,
    },
    PartDef {
        id: "tank_large",
        name: "Large Fuel Tank",
        category: Category::Fuel,
        desc: "1.5 m cube holding 100 L. Goes inside a fuselage.",
        cost: 380,
        research: 5,
        tier: 3,
        mass: 35.0,
        dims: Vec3::new(0.7, 0.7, 1.9),
        stretch: None,
        attach: ALL6,
        kind: PartKind::Tank { fuel: 100.0, external: false },
        color: 2,
    },
    PartDef {
        id: "battery",
        name: "Battery Pack",
        category: Category::Fuel,
        desc: "Stores 100 charge for electric motors.",
        cost: 220,
        research: 2,
        tier: 2,
        mass: 45.0,
        dims: Vec3::new(0.6, 0.35, 1.2),
        stretch: None,
        attach: ALL6,
        kind: PartKind::Battery { charge: 100.0 },
        color: 3,
    },
    // ------------------------------------------------------------------ utility
    PartDef {
        id: "parachute",
        name: "Parachute",
        category: Category::Utility,
        desc: "Deploy with P for a slow, survivable descent when things go wrong.",
        cost: 150,
        research: 3,
        tier: 2,
        mass: 10.0,
        dims: Vec3::new(0.5, 0.3, 0.5),
        stretch: None,
        attach: &[Dir::NY],
        kind: PartKind::Parachute,
        color: 2,
    },
    PartDef {
        id: "drag_chute",
        name: "Drag Chute",
        category: Category::Utility,
        desc: "Deploy with B after touchdown to stop on short runways.",
        cost: 120,
        research: 2,
        tier: 2,
        mass: 8.0,
        dims: Vec3::new(0.4, 0.4, 0.4),
        stretch: None,
        attach: &[Dir::NZ],
        kind: PartKind::DragChute,
        color: 1,
    },
];

pub fn part(id: &str) -> Option<&'static PartDef> {
    PARTS.iter().find(|p| p.id == id)
}

pub fn part_or_panic(id: &str) -> &'static PartDef {
    part(id).unwrap_or_else(|| panic!("unknown part id {id}"))
}

/// Parts that are unlocked in a fresh campaign.
pub fn starting_parts() -> Vec<String> {
    PARTS.iter().filter(|p| p.research == 0).map(|p| p.id.to_string()).collect()
}

/// Whether `def` can be researched given the set of already unlocked part ids.
/// The parts one must own before researching `def`: those in the same category on the
/// nearest lower tier that has any (rows can skip tiers, so "tier - 1" alone would make a
/// part unreachable; Big Tailwing was). Tier 1-2 parts have no prerequisite.
pub fn prerequisites(def: &PartDef) -> Vec<&'static PartDef> {
    if def.tier <= 2 {
        return Vec::new();
    }
    let Some(prev) = PARTS.iter().filter(|p| p.category == def.category && p.tier < def.tier).map(|p| p.tier).max() else { return Vec::new() };
    PARTS.iter().filter(|p| p.category == def.category && p.tier == prev).collect()
}

pub fn researchable(def: &PartDef, unlocked: &[String]) -> bool {
    if def.research == 0 || unlocked.iter().any(|u| u == def.id) {
        return false;
    }
    let pre = prerequisites(def);
    pre.is_empty() || pre.iter().any(|p| unlocked.iter().any(|u| u == p.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_is_reachable() {
        // Unlock everything researchable, tier by tier; nothing may be left locked.
        let mut unlocked: Vec<String> = starting_parts();
        for _ in 0..12 {
            let next: Vec<String> = PARTS.iter().filter(|p| researchable(p, &unlocked)).map(|p| p.id.to_string()).collect();
            unlocked.extend(next);
        }
        let stuck: Vec<&str> = PARTS.iter().filter(|p| p.research > 0 && !unlocked.iter().any(|u| u == p.id)).map(|p| p.id).collect();
        assert!(stuck.is_empty(), "unreachable parts: {stuck:?}");
    }
}
