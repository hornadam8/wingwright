#!/bin/zsh
# Fixed-view captures for graphics comparisons: the same views before and after a change.
# Usage: scripts/gfx.sh OUTDIR
set -e
cd "$(dirname "$0")/.."
out=${1:-shots/gfx}
mkdir -p "$out"
# Views: the harbour town from the air (lighthouse reference), a low look across Meadowvale
# with trees and the strip, the plane on the apron, a high wide shot of the coast, and the
# chase view in flight over the hills.
script="0:creative,0.5:launch"
script="$script,3:look harbor 420 -55 24 -200 150,5.5:shot v_harbor"
script="$script,5.7:look meadowvale 180 150 9 -260 40,8:shot v_meadow_low"
script="$script,8.2:look lighthouse 900 -120 22 0 0,10.5:shot v_lighthouse"
script="$script,10.7:look northwood 1400 40 30 0 0,13:shot v_farm_high"
script="$script,13.2:lookplane 14 -140 12,15:shot v_plane"
script="$script,15.2:freecam -200 260 200 -140 -14,17.5:shot v_hills"
script="$script,17.7:freecam -200 120 200 140 22,20:shot v_sun"
script="$script,20.5:quit"
WINGWRIGHT_NOSAVE=1 WINGWRIGHT_NOSHIMMER=1 WINGWRIGHT_SHOTDIR="$out" WINGWRIGHT_WINDOW=1920x1080 \
  WINGWRIGHT_AUTOTEST="$script" cargo run --features dev > "$out/log.txt" 2>&1 || true
grep -E 'panick|ERROR|autotest' "$out/log.txt" | tail -20
ls "$out"
