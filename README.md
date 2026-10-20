# Wingwright

A build-and-fly game in Rust + Bevy: snap plane parts together in the
hangar, then fly cargo missions between island airports to earn money and research points
for better parts. Native macOS app, no assets to install: the world and every part mesh are
generated procedurally.

## Run

```sh
cargo run --release            # play
cargo run --features dev       # faster incremental builds while developing
cargo test --features dev      # unit tests (design model, placement, cargo logic)
scripts/walkthrough.sh         # end-to-end: build a plane, fly a mission on autopilot, deliver
scripts/make_app.sh            # builds dist/Wingwright.app (double-clickable)
```

Save data lives in `~/Library/Application Support/Wingwright/` (`profile.json` and
`designs/*.json`).

## How to play

**Campaign** starts you at Harbor Field with $250, a wobbly starter plane and one mission.
Fly missions to earn money (buys parts) and research points (unlocks parts). **Creative**
mode has every part unlocked and unlimited money.

### Hangar (builder)

| Action | Control |
| --- | --- |
| Pick a part | click it in the Parts list |
| Attach it | point at a face of your plane, left click |
| Rotate the part being placed | `R` (90 degree steps) |
| Cancel placement | `Esc` or right click |
| Mirror mode (places a twin on the other side) | `M` or the Mirror button |
| Select a placed part | left click it |
| Delete selected part (and whatever hangs off it) | `Delete` / `Backspace` |
| Undo | Cmd+Z or the Undo button |
| Stretch a fuselage / recolor | card at the bottom, with a part selected |
| X-ray (see and pick tanks and batteries inside fuselages) | `X` or the X-ray button |
| Drag overlay (tint parts by drag) / CG markers | Drag / CG buttons |
| Orbit / zoom camera | right-drag / scroll |

Fuel tanks and batteries go inside a fuselage or cockpit: point at one and click. Red ball =
center of mass, blue ball = center of lift; keep the red one slightly ahead. Put the main
wheels under the wings on tall struts for a wide track, and keep the wheels' total load limit
above the plane's weight. Stats shows stall speed, top speed, endurance and range and warns
about missing control surfaces, balance or overloaded wheels.

### Flight

| Action | Control |
| --- | --- |
| Throttle up / down | `Shift` / `Ctrl` (below zero is reverse thrust: the only brake) |
| Full / cut throttle | `Z` / `X` |
| Pitch (nose down / up) | `W` / `S` or arrow keys |
| Roll | `A` / `D` or arrow keys |
| Rudder (also steers on the ground) | `Q` / `E` |
| Landing gear (retractable gear only) | `G` |
| Parachute / drag chute | `P` / `B` |
| Look around / zoom | right-drag / scroll |
| Mission board / map / controls | `Tab` / `M` / `H` |
| Pause | `Esc` |

Park inside an airport's zone at under 4 km/h and a card appears to refuel, load cargo,
edit the plane, or open the mission board. Accept a mission, fly to its pickup airport,
Load Cargo, then fly to the destination; delivery happens when you park there. Missions can
be delivered in several trips. Fuel is tight early on, so plan refuelling stops (free at
Harbor Field, paid elsewhere). Perishable cargo has a timer per load; fragile cargo shatters
on touchdowns harder than 2.5 m/s; passengers need seats.

Crashing costs nothing but the cargo aboard. Landing gear collapses above its rated
touchdown speed (5 m/s for basic wheels, less when overloaded). Scraping a wing or the belly
at low speed is survivable; slamming in or hitting the ground fast is not. Flying too far
from the islands tears the plane apart. The map only shows tiles you have flown over.

## Layout

| File | What |
| --- | --- |
| `src/parts.rs` | part catalog: cost, mass, research tier, aero/engine/gear numbers |
| `src/design.rs` | a plane design: placement geometry, connectivity, stats, serialization |
| `src/builder.rs` | hangar state: picking, snapping, mirror, research/mission/design tabs |
| `src/flight.rs` | rigid-body flight model, wheels, crashes, cargo, HUD, airport services |
| `src/world.rs` | procedural islands, airports, scenery, lighting, map image |
| `src/missions.rs` | cargo kinds and the campaign mission list |
| `src/profile.rs` | save file |
| `src/partvis.rs`, `src/meshes.rs` | procedural part and scenery meshes |
| `src/autotest.rs` | scripted self-test driven by `WINGWRIGHT_AUTOTEST` (see file header) |
