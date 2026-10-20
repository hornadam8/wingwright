#!/bin/bash
# Cross-compiles the Windows build from macOS and zips it into dist/Wingwright-windows.zip.
# Needs: brew install mingw-w64 && rustup target add x86_64-pc-windows-gnu (linker set in .cargo/config.toml).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --target x86_64-pc-windows-gnu
OUT=dist/Wingwright-windows
rm -rf "$OUT" dist/Wingwright-windows.zip
mkdir -p "$OUT"
cp target/x86_64-pc-windows-gnu/release/wingwright.exe "$OUT/Wingwright.exe"
cat > "$OUT/README.txt" <<'TXT'
Wingwright (Windows build, cross-compiled from macOS)

Run Wingwright.exe. Saves go to %APPDATA%\Wingwright (profile.json, designs\).
Needs a GPU with DirectX 12 or Vulkan. If Windows SmartScreen warns, choose "Run anyway"
(the build is unsigned).

Controls: Space/Shift throttle, Ctrl throttle down, W/S pitch, A/D roll, Q/E rudder,
G gear, P parachute, B drag chute, right mouse look, scroll zoom, F free camera,
Tab missions, M map, H help, Esc pause.

Environment overrides (set before launching): WINGWRIGHT_WINDOW=1920x1080 for a window
instead of fullscreen, WINGWRIGHT_UI_SCALE=1.5, WINGWRIGHT_NOVSYNC=1.
TXT
(cd dist && zip -qr Wingwright-windows.zip Wingwright-windows)
ls -la dist/Wingwright-windows.zip
