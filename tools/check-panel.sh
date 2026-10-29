#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Checks that the properties panel (M6a.6) loads and does what its model
# says: runs crates/kassensturz-ui-bridge/qml-tests/PanelSmoke.qml, which puts
# PropertiesPanel.qml on a stand-in for the ComponentProperties object, under
# Qt's `offscreen` platform (it draws no chart, so no GPU is needed). Needs
# `qml6` and the QML modules the panel imports (Kirigami, KI18n).
#
#   tools/check-panel.sh
#
# It fails if the panel doesn't load, if it shows a different set of controls
# than the fields it was given, or if a change reaches the model as another
# call.

set -eu

cd "$(dirname "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp crates/kassensturz-ui-bridge/qml/PropertiesPanel.qml \
    crates/kassensturz-ui-bridge/qml-tests/PanelSmoke.qml "$work"
printf 'PropertiesPanel 1.0 PropertiesPanel.qml\n' > "$work/qmldir"

(
    cd "$work"
    QT_QPA_PLATFORM=offscreen timeout 60 qml6 PanelSmoke.qml
)
echo "properties panel: ok"
