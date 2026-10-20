#!/bin/bash
# End-to-end walkthrough: build a plane within the starting budget, accept the first mission,
# fly to the pickup airport on autopilot, collect the cargo, fly it to the destination, land,
# and confirm the delivery. Prints WALKTHROUGH OK or WALKTHROUGH FAIL. Runs the real game
# window (full screen) at 2x speed; takes about five minutes.
set -uo pipefail
cd "$(dirname "$0")/.."
SHOTS="${WINGWRIGHT_SHOTDIR:-/tmp/wingwright-walkthrough}"
mkdir -p "$SHOTS"
SCRIPT="1:new,2:clear,2.2:place eng_small 0 NZ,2.4:place fus_cargo 0 PZ,2.6:place tail_cone 2 PZ,2.8:place wing_biplane 0 PX 0 0.25 0.75,3.2:place elevator 3 PX 0 0 0.5,3.4:place elevator 3 PY 0 0 0.5,3.6:place wheel_fixed 0 NY 0 0 -0.5,3.8:place wheel_tall 4 NY -0.5 0 0.5,4.0:place tank_small 0 PY 0 0 0.5,4.3:stats,4.5:tab drag,4.9:shot built_drag,5.0:tab drag,5.1:tab xray,5.3:shot built_xray,5.4:tab xray,5.5:tab cg,5.6:tab cg,5.7:shot built,5.8:accept first_flight,6:launch,6.5:map,7:shot map,7.4:map,7.6:speed 2,7.8:nav meadowvale,60:shot enroute,90:wait,91:load first_flight,92:refuel,93:shot pickup,94:nav harbor,150:wait,151:check first_flight,153:quit"
WINGWRIGHT_NOSAVE=1 WINGWRIGHT_SHOTDIR="$SHOTS" WINGWRIGHT_AUTOTEST="$SCRIPT" \
  timeout 500 cargo run --features dev 2>&1 \
  | grep -E 'autotest|nav:|toast|WALKTHROUGH|sim fps|panick|ERROR' \
  | sed 's/\x1b\[[0-9;]*m//g' | sed 's/.*wingwright::[a-z]*: //' | sed 's/^.*ERROR //' | cut -c1-190 \
  | tee "$SHOTS/walkthrough.log" | grep -vE '^sim fps' 
echo "---- verdict:"
grep -E 'WALKTHROUGH' "$SHOTS/walkthrough.log" || echo "no verdict (crash or timeout?)"
