#!/usr/bin/env bash
# Builds a release binary and wraps it in a minimal RustyEdit.app bundle
# (Info.plist + the icon), so Finder, the Dock and Spotlight show the
# real app icon instead of a generic binary. Run from the repo root:
#
#   ./scripts/bundle-mac.sh
#
# The result is target/release/bundle/RustyEdit.app, which can be
# dragged into /Applications or opened directly with `open`.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')
APP="target/release/bundle/RustyEdit.app"

cargo build --release -p rustyedit

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/rustyedit "$APP/Contents/MacOS/RustyEdit"
cp assets/icon.icns "$APP/Contents/Resources/icon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>RustyEdit</string>
	<key>CFBundleDisplayName</key>
	<string>RustyEdit</string>
	<key>CFBundleIdentifier</key>
	<string>com.mascanho.rustyedit</string>
	<key>CFBundleVersion</key>
	<string>${VERSION}</string>
	<key>CFBundleShortVersionString</key>
	<string>${VERSION}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleExecutable</key>
	<string>RustyEdit</string>
	<key>CFBundleIconFile</key>
	<string>icon.icns</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
</dict>
</plist>
PLIST

touch "$APP"
echo "Built $APP"
