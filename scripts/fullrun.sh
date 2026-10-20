#!/bin/zsh
# Plays the opening of the game in three short scenes and captures the real window (egui
# included) at every stage, so the flow can be reviewed screen by screen:
#   1 hangar: menu, empty hangar, building the plane part by part, research, missions,
#     accepting, the airport card, the map, take-off
#   2 first leg: cruise, parked at the pick-up, the cargo dialog, loading, refuelling
#   3 second leg: cruise, delivery, mission complete, verdict
#   4 tier2 (creative): metal prop plane with metal wings, medium tailwing, retractable gear;
#     built, flown to Lighthouse Point and parked
#   5 jet (creative): twin small jets under airliner wings on huge body segments; flown to
#     Northwood and parked (tests the speed limits, the tail surfaces upright, big parts)
# Pass a scene name (hangar | leg1 | leg2) to run only that one.
# Captures land in $WINGWRIGHT_SHOTDIR (default /tmp/wingwright-fullrun). Needs screen
# recording permission for the terminal and swiftc (Xcode command line tools). The game
# window must be on the visible Space for `cap` to work; `shot` always works (no egui).
set -uo pipefail
cd "$(dirname "$0")/.."
SHOTS="${WINGWRIGHT_SHOTDIR:-/tmp/wingwright-fullrun}"
mkdir -p "$SHOTS"
export WINGWRIGHT_WINID=/tmp/wingwright-winid
[ -x "$WINGWRIGHT_WINID" ] || swiftc -O scripts/winid.swift -o "$WINGWRIGHT_WINID"

# The starter-budget plane, placed part by part (2 s apart so it can be watched) ...
BUILD="4:clear,6:place eng_small 0 NZ,8:place fus_cargo 0 PZ,10:place tail_cone 2 PZ,12:place wing_biplane 0 PX 0 0.25 0.75,16:place elevator 3 PX 0 0 0.5,18:place elevator 3 PY 0 0 0.5,20:place wheel_fixed 0 NY 0 0 -0.5,22:place wheel_tall 4 NY -0.5 0 0.5,24:place tank_small 0 PY 0 0 0.5"
# ... and the same plane built instantly for the flight scenes.
QUICK="2:clear,2.1:place eng_small 0 NZ,2.2:place fus_cargo 0 PZ,2.3:place tail_cone 2 PZ,2.4:place wing_biplane 0 PX 0 0.25 0.75,2.6:place elevator 3 PX 0 0 0.5,2.7:place elevator 3 PY 0 0 0.5,2.8:place wheel_fixed 0 NY 0 0 -0.5,2.9:place wheel_tall 4 NY -0.5 0 0.5,3.0:place tank_small 0 PY 0 0 0.5"

SCENE1="1.5:cap menu,2:new,3.5:cap hangar_empty,$BUILD,13:cap hangar_wings,25:cap hangar_built,26:tab research,27.5:cap research,28:tab missions,29.5:cap missions,30:accept first_flight,31.5:cap accepted,32:launch,34:cap airport,35:map,36.5:cap map,37:map,38:thr 1,56:cap takeoff,58:quit"
SCENE2="1:new,$QUICK,4:accept first_flight,4.5:launch,5:speed 2,5.5:nav meadowvale,40:cap enroute,41:shot enroute_world,70:wait,71:cap parked_pickup,72:cargo,73.5:cap cargo_dialog,74:load first_flight,75.5:cap cargo_loaded,76:cargo,77:refuel,78:cap refuelled,80:quit"
# Higher tiers in creative mode (everything unlocked). Host indices count placed parts in
# order; mirrored parts append two.
# Speed tests: take off and climb on pitch holds, then hold level and let the plane
# accelerate; the physics summary reports the top speed reached. No landing (the landing
# autopilot is tuned for the 32 m/s starter).
FLY="18:launch,19.5:cap {n}_airport,20:thr 1,21:hold 8,22:speed 2,40:gear up,50:alt 400,58:bank 25,140:shot {n}_air,142:cap {n}_flight,145:quit"
# Host indices count placed parts in order (mirrored placements append two). Main gear goes
# under the body so every test plane sits level or nose-up on its wheels.
TIER2="2:creative,3:clear,4:place eng_metal_prop 0 NZ,5:place neck 0 PZ,6:place fus_cargo 2 PZ,7:place tail_cone 3 PZ,8:place tail_end 4 PZ,9:place wing_standard 3 PX 0 0.25 0,10:place elevator_large 4 PX 0 0 0.1,11:place elevator 4 PY 0 0 0.1,12:place gear_retract 0 NY 0 0 -0.3,13:place gear_retract 6 NY 1.0 0 0,14:place tank_large 3 PY,15:stats,16:cap tier2_built,${FLY//\{n\}/tier2}"
# Jets: the 2 m Jet Cockpit straight onto huge body segments (no neck needed).
JET="2:creative,3:clear cockpit_airliner,4:place fus_cargo 0 PZ,5:place fus_cargo 1 PZ,6:place tail_cone 2 PZ,7:place tail_end 3 PZ,8:place wing_jet 1 PX 0 0 0.3,9:place eng_jet_small 5 NY 0 0 0,10:place elevator_large 3 PX 0 0 0.1,11:place elevator_large 3 PY 0 0 0.1,12:place gear_retract 0 NY 0 0 -0.6,13:place gear_retract 1 NY 1.3 0 0.6,14:place tank_large 1 PY,15:place tank_large 2 PY,16:stats,16.5:cap jet_built,${FLY//\{n\}/jet}"
# Two big turbines and two small ones under big jet wings, like Adam's airliner screenshot.
HEAVY="2:creative,3:clear cockpit_airliner,4:place fus_cargo 0 PZ,5:place fus_cargo 1 PZ,6:place fus_cargo 2 PZ,7:place tail_cone 3 PZ,8:place tail_end 4 PZ,9:place wing_jet_big 2 PX 0 0 -0.3,10:place eng_jet 6 NY -1.4 0 0,11:place eng_jet_small 6 NY 1.9 0 0,12:place tail_big 4 PX 0 0 0.1,13:place elevator_large 4 PY 0 0 0.1,14:place gear_heavy 0 NY 0 0 -0.6,15:place gear_heavy 2 NY 1.3 0 0.5,16:place tank_large 1 PY,17:place tank_large 2 PY,18:place tank_large 3 PY,19:stats,19.5:cap heavy_built,${FLY//\{n\}/heavy}"
RAMJET="2:creative,3:clear cockpit_airliner,4:place fus_cargo 0 PZ,5:place fus_cargo 1 PZ,6:place tail_cone 2 PZ,7:place tail_end 3 PZ,8:place wing_jet 1 PX 0 0 0.3,9:place eng_jet_small 5 NY 0 0 0,10:place pylon 2 PY 0 0 0,11:place eng_ramjet 9 NZ,12:place elevator_large 3 PX 0 0 0.1,13:place elevator_large 3 PY 0 0 0.1,14:place gear_retract 0 NY 0 0 -0.6,15:place gear_retract 1 NY 1.3 0 0.6,16:place tank_large 1 PY,16.5:place tank_large 2 PY,16.8:stats,16.9:cap ramjet_built,${FLY//\{n\}/ramjet}"
MANUAL="2:creative,3:clear cockpit_airliner,4:place fus_cargo 0 PZ,5:place fus_cargo 1 PZ,6:place tail_cone 2 PZ,7:place tail_end 3 PZ,8:place wing_jet 1 PX 0 0 0.3,9:place eng_jet_small 5 NY 0 0 0,10:place elevator_large 3 PX 0 0 0.1,11:place elevator_large 3 PY 0 0 0.1,12:place gear_retract 0 NY 0 0 -0.6,13:place gear_retract 1 NY 1.3 0 0.6,14:place tank_large 1 PY,15:place tank_large 2 PY,16:stats,18:launch,48:cap manual_1,78:cap manual_2,108:cap manual_3,138:cap manual_4,168:cap manual_5,198:cap manual_6,228:cap manual_7,258:cap manual_8,290:quit"
ROCKET="2:creative,3:clear cockpit_airliner,4:place fus_cargo 0 PZ,5:place fus_cargo 1 PZ,6:place tail_cone 2 PZ,7:place tail_end 3 PZ,8:place wing_jet 1 PX 0 0 0.3,9:place eng_rocket 2 PX 0 0 0,10:place elevator_large 3 PX 0 0 0.1,11:place elevator_large 3 PY 0 0 0.1,12:place gear_retract 0 NY 0 0 -0.6,13:place gear_retract 1 NY 1.3 0 0.6,14:place tank_large 1 PY,15:place tank_large 2 PY,16:stats,16.5:cap rocket_built,${FLY//\{n\}/rocket}"

SCENE3="1:new,$QUICK,4:accept first_flight,4.5:launch,5:teleport meadowvale,6:load first_flight,6.5:speed 2,7:nav harbor,40:cap enroute2,70:wait,71:cap delivered,72:check first_flight,73:cap complete,75:quit"

run_scene() {
  local name=$1 script=$2
  echo "== scene $name"
  WINGWRIGHT_NOSAVE=1 WINGWRIGHT_SHOTDIR="$SHOTS" WINGWRIGHT_AUTOTEST="$script" \
    timeout 400 cargo run --features dev > "$SHOTS/$name.log" 2>&1
  local clean="$SHOTS/$name.clean"
  sed 's/\x1b\[[0-9;]*m//g' "$SHOTS/$name.log" > "$clean"
  grep -E 'WALKTHROUGH|toast|wait over|cap |nav: (airborne|stopped|parked)|Crashed|panick|window not found|could not place|autotest: design' "$clean" | sed 's/.*wingwright[a-z:]*: //' | cut -c1-220
  # Physics summary: what the builder predicted vs what the flight reached.
  local vmax=$(grep -oE 'spd=[0-9.]+' "$clean" | sed 's/spd=//' | sort -n | tail -1)
  local hmax=$(grep -oE 'pos=\([^)]*\)' "$clean" | awk -F, '{print $2}' | sort -n | tail -1)
  local vsmax=$(grep -oE 'vs=\+[0-9.]+' "$clean" | sed 's/vs=+//' | sort -n | tail -1)
  echo "physics: max speed ${vmax:-?} m/s ($(printf '%.0f' $(echo "${vmax:-0} * 3.6" | bc -l)) km/h), max altitude ${hmax:-?} m, best climb ${vsmax:-?} m/s"
}
# Optional argument: run a single scene (hangar | leg1 | leg2 | tier2 | jet | heavy | ramjet | rocket); `manual` builds the
# twin jet, launches and captures every 30 s for 4.5 minutes while YOU fly it.
ONLY="${1:-all}"
[[ "$ONLY" == all || "$ONLY" == hangar ]] && run_scene hangar "$SCENE1"
[[ "$ONLY" == all || "$ONLY" == leg1 ]] && run_scene leg1 "$SCENE2"
[[ "$ONLY" == all || "$ONLY" == leg2 ]] && run_scene leg2 "$SCENE3"
[[ "$ONLY" == all || "$ONLY" == tier2 ]] && run_scene tier2 "$TIER2"
[[ "$ONLY" == all || "$ONLY" == jet ]] && run_scene jet "$JET"
[[ "$ONLY" == all || "$ONLY" == heavy ]] && run_scene heavy "$HEAVY"
[[ "$ONLY" == all || "$ONLY" == ramjet ]] && run_scene ramjet "$RAMJET"
[[ "$ONLY" == all || "$ONLY" == rocket ]] && run_scene rocket "$ROCKET"
[[ "$ONLY" == manual ]] && run_scene manual "$MANUAL"
ls "$SHOTS"/*.png | wc -l | xargs echo "captures:"
