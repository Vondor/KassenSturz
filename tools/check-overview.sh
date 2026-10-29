#!/bin/sh
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Checks that the month of the overview (M6a.8) loads and does what the page
# needs: runs crates/kassensturz-ui-bridge/qml-tests/OverviewSmoke.qml, which
# puts OverviewMonth.qml on stand-ins for BookCards and BookCharts, under Qt's
# `offscreen` platform. Needs `qml6` and the QML modules the components
# import (Kirigami, KI18n, KQuickCharts).
#
#   tools/check-overview.sh
#
# It fails if the month doesn't load, doesn't ask for the month the overview
# shows (and again when that changes), or shows another number of cards or
# charts than it was given, puts the cards in one column of a wide window
# or lists other than the five largest categories.

set -eu

cd "$(dirname "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
qml=crates/kassensturz-ui-bridge/qml
for name in OverviewMonth KpiGrid KpiCard KpiNames ProgressMeter AmountLabel ChartView ChartTable ChartPalette DataTable; do
    cp "$qml/$name.qml" "$work"
done
cp crates/kassensturz-ui-bridge/qml-tests/OverviewSmoke.qml "$work"
printf 'singleton ChartPalette 1.0 ChartPalette.qml\n' > "$work/qmldir"
for name in OverviewMonth KpiGrid KpiCard KpiNames ProgressMeter AmountLabel ChartView ChartTable DataTable; do
    printf '%s 1.0 %s.qml\n' "$name" "$name" >> "$work/qmldir"
done

(
    cd "$work"
    QT_QPA_PLATFORM=offscreen timeout 60 qml6 OverviewSmoke.qml
)
