#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Checks the Statistics workbench's QML (M6a.6, M6a.7) without the Rust
# objects, under Qt's `offscreen` platform (it draws no GPU chart, so no
# Mesa is needed):
#
# - PanelSmoke.qml puts PropertiesPanel.qml on a stand-in for the
#   ComponentProperties object and checks that it shows one control per
#   field and passes each change on as the call it names.
# - StatisticsSmoke.qml opens StatisticsPage.qml, with the chart, table, card
#   and progress components, on stand-ins for SavedViews, ComponentProperties,
#   BookCharts and BookCards (crates/kassensturz-ui-bridge/qml-tests/fakes)
#   and clicks through it.
#
# - ReviewSmoke.qml opens AiChangeReviewPage.qml with a set that holds a new
#   chart the AI proposed (M6a.9), on stand-ins for AiSwitch and AiChat.
#
# Needs `qml6` and the QML modules the pages import (Kirigami, KI18n,
# QuickCharts). It fails if a page doesn't load, if a check fails, or if QML
# prints a warning about a type, a binding or a property.
#
#   tools/check-panel.sh

set -eu

cd "$(dirname "$0")/.."

bridge=crates/kassensturz-ui-bridge
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# 1. The panel alone.
cp "$bridge"/qml/PropertiesPanel.qml "$bridge"/qml-tests/PanelSmoke.qml "$work"
printf 'PropertiesPanel 1.0 PropertiesPanel.qml\n' > "$work/qmldir"
(
    cd "$work"
    QT_QPA_PLATFORM=offscreen timeout 60 qml6 PanelSmoke.qml
)
echo "properties panel: ok"

# 2. The page, as a module with the app's URI, so its files see each other
# and the stand-ins as they see the Rust objects.
module="$work/imports/io/github/vondor/kassensturz"
mkdir -p "$module"
for file in StatisticsPage AiChangeReviewPage AiChangeCorrection ComponentPreview PropertiesPanel ChartView ChartTable DataTable \
    AmountLabel KpiCard KpiNames ProgressMeter ChartPalette FormError Plain; do
    cp "$bridge/qml/$file.qml" "$module"
done
cp "$bridge"/qml-tests/fakes/*.qml "$module"
{
    echo 'module io.github.vondor.kassensturz'
    echo 'singleton ChartPalette 1.0 ChartPalette.qml'
    echo 'singleton Plain 1.0 Plain.qml'
    for file in StatisticsPage AiChangeReviewPage AiChangeCorrection ComponentPreview PropertiesPanel ChartView \
        ChartTable DataTable AmountLabel KpiCard KpiNames ProgressMeter FormError SavedViews \
        ComponentProperties BookCharts BookCards AiSwitch AiChat AccountTypes AiErrorMessage; do
        echo "$file 1.0 $file.qml"
    done
} > "$module/qmldir"
cp "$bridge"/qml-tests/StatisticsSmoke.qml "$bridge"/qml-tests/ReviewSmoke.qml "$work"
run_smoke() {
    (
        cd "$work"
        status=0
        QML_IMPORT_PATH="$work/imports" QT_QPA_PLATFORM=offscreen \
            timeout 90 qml6 "$1.qml" > run.log 2>&1 || status=$?
        cat run.log
        if grep -E 'FAIL|TypeError|ReferenceError|is not a type|Binding loop|Cannot assign|Unable to assign|is not installed|Cannot read' run.log; then
            echo "check-panel: $1 printed warnings or failures (above)" >&2
            status=1
        fi
        exit $status
    )
}

run_smoke StatisticsSmoke
echo "statistics page: ok"
run_smoke ReviewSmoke
echo "review page: ok"
