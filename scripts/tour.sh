#!/bin/zsh
# Photographs every airport with the free camera (two angles each) into shots/tour/.
# Usage: scripts/tour.sh [airport ids...]   (default: all)
set -e
cd "$(dirname "$0")/.."
ids=("$@")
if [ ${#ids} -eq 0 ]; then
  ids=(harbor meadowvale northwood stmarys lighthouse frostholm glacier oasis dunmar hermit gullrock wreck palmcay atoll blackpeak wolds sawmill delta ironholm windward kingsport ravine mesa)
fi
mkdir -p shots/tour
script="0:creative,0.5:launch"
t=3
for id in $ids; do
  # a: the settlement from over the runway; b: the apron and hangar from the far side.
  script="$script,${t}:look $id 330 -55 26 -200 150,$((t+1)).2:shot tour_${id}_a,$((t+1)).4:look $id 260 125 30 -250 60,$((t+2)).6:shot tour_${id}_b"
  t=$((t+3))
done
script="$script,${t}:quit"
WINGWRIGHT_NOSAVE=1 WINGWRIGHT_NOSHIMMER=1 WINGWRIGHT_SHOTDIR=shots/tour WINGWRIGHT_WINDOW=1920x1080 \
  WINGWRIGHT_AUTOTEST="$script" cargo run --features dev 2>&1 > shots/tour/log.txt 2>&1 || true
ls shots/tour | wc -l
