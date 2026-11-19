#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Makes the experimental, UNSIGNED macOS download (D-875) from what KDE Craft
# installed: KassenSturz.app and a .dmg that holds it. No Apple Developer
# account is used, so the app is only ad-hoc signed (an Apple Silicon Mac
# refuses to run a program with no signature at all) and not notarized;
# docs/MACOS.md tells the person how to open it anyway.
#
# Usage: packaging/macos/make-dmg.sh <Craft root> <build number> <output folder>
#
# Prints what it finds, since nobody on the team has a Mac to look at.

set -eu

craft_root=${1:?Craft root}
version=${2:?build number}
out=${3:?output folder}
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)

app="$out/KassenSturz.app"
dmg_name="KassenSturz-$version-macos-arm64-experimental.dmg"
rm -rf "$app" "$out/dmgroot"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$out"

binary="$craft_root/bin/kassensturz"
[ -x "$binary" ] || { echo "No $binary: Craft did not install the app."; exit 1; }
cp "$binary" "$app/Contents/MacOS/kassensturz"
if [ -x "$craft_root/bin/kassensturz-mcp" ]; then
    cp "$craft_root/bin/kassensturz-mcp" "$app/Contents/MacOS/kassensturz-mcp"
fi

# The icon: the PNG sizes of data/icons as an .icns.
iconset="$out/KassenSturz.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
icons="$repo/data/icons/hicolor"
png() { echo "$icons/$1x$1/apps/io.github.vondor.KassenSturz.png"; }
cp "$(png 16)" "$iconset/icon_16x16.png"
cp "$(png 32)" "$iconset/icon_16x16@2x.png"
cp "$(png 32)" "$iconset/icon_32x32.png"
cp "$(png 64)" "$iconset/icon_32x32@2x.png"
cp "$(png 128)" "$iconset/icon_128x128.png"
cp "$(png 256)" "$iconset/icon_128x128@2x.png"
cp "$(png 256)" "$iconset/icon_256x256.png"
iconutil --convert icns --output "$app/Contents/Resources/KassenSturz.icns" "$iconset"
rm -rf "$iconset"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>KassenSturz</string>
  <key>CFBundleDisplayName</key><string>KassenSturz</string>
  <key>CFBundleIdentifier</key><string>io.github.vondor.KassenSturz</string>
  <key>CFBundleExecutable</key><string>kassensturz</string>
  <key>CFBundleIconFile</key><string>KassenSturz</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleLocalizations</key><array><string>en</string><string>de</string></array>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.finance</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

# The German texts (.mo) that ki18n_install put next to the program: KI18n
# looks for locale/<language>/LC_MESSAGES/ in the data folders, which for an
# app bundle include Contents/Resources.
mo_found=0
for mo in $(find "$craft_root/share/locale" "$craft_root/Applications" -name 'kassensturz.mo' 2>/dev/null); do
    lang=$(basename "$(dirname "$(dirname "$mo")")")
    mkdir -p "$app/Contents/Resources/locale/$lang/LC_MESSAGES"
    cp "$mo" "$app/Contents/Resources/locale/$lang/LC_MESSAGES/kassensturz.mo"
    mo_found=$((mo_found + 1))
done
echo "German and other catalogs copied: $mo_found"
[ "$mo_found" -gt 0 ] || { echo "No kassensturz.mo under $craft_root: the app would show English only."; exit 1; }

# Qt, KDE Frameworks and the QML modules: macdeployqt reads the QML files'
# imports and copies the frameworks, plugins and modules they need into the
# bundle, and points the program at them.
macdeployqt="$craft_root/bin/macdeployqt"
[ -x "$macdeployqt" ] || macdeployqt=$(command -v macdeployqt || true)
[ -n "$macdeployqt" ] || { echo "No macdeployqt in $craft_root/bin or on the PATH."; exit 1; }
echo "macdeployqt: $macdeployqt"
"$macdeployqt" "$app" \
    -qmldir="$repo/crates/kassensturz-ui-bridge/qml" \
    -qmlimport="$craft_root/qml" \
    -libpath="$craft_root/lib" \
    -always-overwrite \
    -verbose=1

# The KDE modules the QML code loads by name (kirigami, ki18n, quickcharts,
# the desktop style) must be in the bundle even if the scan missed one.
mkdir -p "$app/Contents/Resources/qml"
for module in org/kde/kirigami org/kde/kirigamiaddons org/kde/ki18n org/kde/quickcharts org/kde/desktop org/kde/qqc2desktopstyle org/kde/sonnet org/kde/kquickcontrols org/kde/kquickcontrolsaddons org/kde/coreaddons org/kde/config org/kde/iconthemes; do
    if [ -d "$craft_root/qml/$module" ] && [ ! -d "$app/Contents/Resources/qml/$module" ]; then
        mkdir -p "$app/Contents/Resources/qml/$(dirname "$module")"
        cp -R "$craft_root/qml/$module" "$app/Contents/Resources/qml/$(dirname "$module")/"
        echo "Added QML module $module"
    fi
done

# macdeployqt deploys only the window plugin (cocoa). The offscreen one is a
# few hundred kilobytes and lets CI start the app where no screen is attached
# (the start check in macos-app.yml tries the real window first).
offscreen=$(find "$craft_root/plugins" -name 'libqoffscreen.dylib' 2>/dev/null | head -n 1)
if [ -n "$offscreen" ]; then
    mkdir -p "$app/Contents/PlugIns/platforms"
    cp "$offscreen" "$app/Contents/PlugIns/platforms/libqoffscreen.dylib"
    chmod u+w "$app/Contents/PlugIns/platforms/libqoffscreen.dylib"
else
    echo "No libqoffscreen.dylib under $craft_root/plugins: the start check can only use a real window."
fi

# Modules copied by hand above (and anything macdeployqt left alone) still
# point at Craft's lib folder. Copy each library they need into the bundle's
# Frameworks folder and point the loader at the copy; repeat, since a copied
# library has dependencies of its own.
mkdir -p "$app/Contents/Frameworks"
pass=0
while [ "$pass" -lt 6 ]; do
    pass=$((pass + 1))
    changed=0
    for file in "$app/Contents/MacOS/kassensturz" $(find "$app/Contents/Frameworks" "$app/Contents/PlugIns" "$app/Contents/Resources/qml" \( -name '*.dylib' -o -name '*.so' \) 2>/dev/null); do
        for dep in $(otool -L "$file" 2>/dev/null | tail -n +2 | awk '{print $1}' | grep "$craft_root" || true); do
            base=$(basename "$dep")
            if [ ! -f "$app/Contents/Frameworks/$base" ]; then
                if [ -f "$dep" ]; then
                    cp "$dep" "$app/Contents/Frameworks/$base"
                    chmod u+w "$app/Contents/Frameworks/$base"
                    install_name_tool -id "@executable_path/../Frameworks/$base" "$app/Contents/Frameworks/$base"
                    echo "Copied $base into Frameworks"
                else
                    echo "Missing $dep, needed by $file"
                    continue
                fi
            fi
            install_name_tool -change "$dep" "@executable_path/../Frameworks/$base" "$file"
            changed=1
        done
    done
    [ "$changed" -eq 1 ] || break
done

# No path of the build machine may be left in the program or its libraries:
# every library must be found inside the bundle or in the system.
bad=0
for file in "$app/Contents/MacOS/kassensturz" $(find "$app/Contents/Frameworks" "$app/Contents/PlugIns" "$app/Contents/Resources/qml" \( -name '*.dylib' -o -name '*.so' \) 2>/dev/null); do
    if otool -L "$file" 2>/dev/null | tail -n +2 | awk '{print $1}' | grep -E "CraftRoot|/Users/|/opt/homebrew|/usr/local" > /dev/null; then
        echo "A path of the build machine is left in $file:"
        otool -L "$file" | grep -E "CraftRoot|/Users/|/opt/homebrew|/usr/local"
        bad=1
    fi
done
[ "$bad" -eq 0 ] || { echo "Libraries outside the bundle are still linked."; exit 1; }

# Ad-hoc signature, no identity: this is not a Developer ID signature and does
# not make Gatekeeper trust the app, but Apple Silicon will not run code with
# no signature at all.
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
echo "Bundle size:"; du -sh "$app"

# The disk image: the app, a link to /Applications and the way to open it.
root="$out/dmgroot"
mkdir -p "$root"
cp -R "$app" "$root/"
ln -s /Applications "$root/Applications"
cp "$here/HOW_TO_OPEN.txt" "$root/SO OEFFNEN - HOW TO OPEN.txt"
rm -f "$out/$dmg_name"
hdiutil create -volname "KassenSturz $version" -srcfolder "$root" -ov -format UDZO "$out/$dmg_name"
rm -rf "$root"
ls -l "$out"
