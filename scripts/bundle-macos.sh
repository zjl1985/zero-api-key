#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# GPUI is a native renderer; only the executable, icon and notices ship.
# Set PRECOMPILE_SHADERS=1 with full Xcode installed for precompiled Metal.
if [[ "${PRECOMPILE_SHADERS:-0}" == 1 ]]; then
  cargo build --locked --release --workspace --no-default-features
else
  cargo build --locked --release --workspace
fi

build_stage="$(mktemp -d "${TMPDIR:-/tmp}/zero-api-key-bundle.XXXXXX")"
trap 'rm -rf "$build_stage"' EXIT
app_path="$build_stage/ZeroApiKey.app"
mkdir -p "$app_path/Contents/MacOS" "$app_path/Contents/Resources/licenses"
cp target/release/zero-api-key "$app_path/Contents/MacOS/zero-api-key"
cp crates/desktop/assets/icon.icns "$app_path/Contents/Resources/icon.icns"
cp LICENSE THIRD_PARTY_NOTICES.md "$app_path/Contents/Resources/"
cp licenses/Apache-2.0.txt "$app_path/Contents/Resources/licenses/"
cp crates/desktop/Info.plist "$app_path/Contents/Info.plist"

signing_identity="${SIGNING_IDENTITY:-ZeroApiKey Local Dev}"
if security find-identity -v -p codesigning | rg -Fq "\"$signing_identity\""; then
  codesign --force --sign "$signing_identity" "$app_path"
else
  codesign --force --sign - "$app_path"
fi
codesign --verify --strict "$app_path"

mkdir -p dist
if [[ -e dist/ZeroApiKey.app ]]; then
  # Preserve previous builds, including any files the user added to the bundle.
  command -v trash >/dev/null || { echo 'Install trash or move dist/ZeroApiKey.app aside first.' >&2; exit 1; }
  trash dist/ZeroApiKey.app
fi
cp -R "$app_path" dist/ZeroApiKey.app
ln -s /Applications "$build_stage/Applications"
architecture="$(uname -m)"
disk_image="dist/ZeroApiKey_0.2.0_${architecture}.dmg"
if [[ -e "$disk_image" ]]; then
  command -v trash >/dev/null || { echo 'Install trash or move the previous DMG aside first.' >&2; exit 1; }
  trash "$disk_image"
fi
hdiutil create -volname ZeroApiKey -srcfolder "$build_stage" -format UDZO "$disk_image" >/dev/null
printf '\nBuilt native GPUI app:\n'
du -sh dist/ZeroApiKey.app "$disk_image" target/release/zak
