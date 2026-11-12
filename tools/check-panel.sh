#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Runs the QML smoke tests of the app's pages (M6a.6 onwards) without the
# Rust objects, under Qt's `offscreen` platform (it draws no GPU chart, so no
# Mesa is needed).
#
# Each test is one file, crates/kassensturz-ui-bridge/qml-tests/*Smoke.qml,
# and says what it checks in its own header. A file runs here when it has
# the line
#
#   // check-panel: <what the log says when it passes>
#
# and runs a second time with the desktop style the app uses when it also has
#
#   // check-panel-desktop: <what the log says when that passes>
#
# so a new test adds its own file and touches no list in this script, and
# parallel pull requests that add tests don't conflict here (US5.10, Q-502).
# The tests run in the order of their file names, each in its own `qml6`.
# ChartsSmoke.qml has no such line: tools/check-charts.sh runs it.
#
# The tests see every page of crates/kassensturz-ui-bridge/qml as part of
# the app's module, with the stand-ins for the Rust objects
# (qml-tests/fakes) on top; a stand-in replaces a page of the same name.
# A file with `pragma Singleton` is listed as a singleton.
#
# Needs `qml6` and the QML modules the pages import (Kirigami, KI18n,
# QuickCharts). It fails if no test is found, if a page doesn't load, if a
# check fails, or if QML prints a warning about a type, a binding or a
# property.
#
#   tools/check-panel.sh

set -eu

cd "$(dirname "$0")/.."

bridge=crates/kassensturz-ui-bridge
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The pages and the stand-ins, as a module with the app's URI, so their files
# see each other and the stand-ins as they see the Rust objects.
module="$work/imports/io/github/vondor/kassensturz"
mkdir -p "$module"
cp "$bridge"/qml/*.qml "$module"
cp "$bridge"/qml-tests/fakes/*.qml "$module"
{
    echo 'module io.github.vondor.kassensturz'
    for path in "$module"/*.qml; do
        file=$(basename "$path" .qml)
        if grep -q '^pragma Singleton' "$path"; then
            echo "singleton $file 1.0 $file.qml"
        else
            echo "$file 1.0 $file.qml"
        fi
    done
} > "$module/qmldir"
cp "$bridge"/qml-tests/*Smoke.qml "$work"

run_smoke() {
    (
        cd "$work"
        status=0
        QML_IMPORT_PATH="$work/imports" QT_QPA_PLATFORM=offscreen \
            timeout 90 qml6 "$1.qml" > run.log 2>&1 || status=$?
        cat run.log
        if grep -E 'FAIL|TypeError|ReferenceError|is not a type|Binding loop|Cannot assign|Unable to assign|is not installed|Cannot read|arguments to message|overrides a member of the base object|invalid override' run.log; then
            echo "check-panel: $1 printed warnings or failures (above)" >&2
            status=1
        fi
        exit $status
    )
}

tests=0
for path in "$bridge"/qml-tests/*Smoke.qml; do
    name=$(basename "$path" .qml)
    passed=$(sed -n 's|^// check-panel: ||p' "$path")
    [ -n "$passed" ] || continue
    run_smoke "$name"
    echo "$passed: ok"
    desktop=$(sed -n 's|^// check-panel-desktop: ||p' "$path")
    if [ -n "$desktop" ]; then
        (
            export QT_QUICK_CONTROLS_STYLE=org.kde.desktop
            run_smoke "$name"
        )
        echo "$desktop: ok"
    fi
    tests=$((tests + 1))
done
if [ "$tests" -eq 0 ]; then
    echo "check-panel: no smoke test found in $bridge/qml-tests" >&2
    exit 1
fi
echo "check-panel: $tests smoke tests passed"
