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
#   field and passes each change on as the call it names, and that the
#   formula field (M6b.7) shows a result with what it counted, marks the place
#   of a mistake, completes names and lists the functions (the stand-in
#   BookFormula).
# - StatisticsSmoke.qml opens StatisticsPage.qml, with the chart, table, card
#   and progress components, on stand-ins for SavedViews, ComponentProperties,
#   BookCharts and BookCards (crates/kassensturz-ui-bridge/qml-tests/fakes)
#   and clicks through it.
#
# - ReviewSmoke.qml opens AiChangeReviewPage.qml with a set that holds a new
#   chart the AI proposed (M6a.9), on stand-ins for AiSwitch and AiChat.
#
# - GroupsSmoke.qml opens the pages of My groups and Hidden categories (M6b.5c)
#   on a stand-in for CategoryGroups and checks what they send.
#
# - GridSmoke.qml puts DashboardGrid.qml (the Overview's grid, M6b.3) on a
#   stand-in for DashboardLayout and checks where it draws each component,
#   the one-column list of a narrow window, the problem count and what each
#   key asks the layout for.
#
# - BudgetsSmoke.qml opens the Budgets page, the sheet that sets or changes a
#   budget and the budget bars (M6c.5) on stand-ins for BudgetBook and
#   BookBudgets.
#
# - FilterSmoke.qml opens the filter bar and the notes under a component
#   (M6c.7) on stand-ins for DashboardLayout and CategoryGroups and checks
#   what the buttons say, what a choice sends and the words of every note.
#
# - UpdateAllSmoke.qml opens the Update all accounts widget (M6d.7) on a
#   stand-in for BankConnection and checks that it always shows (D-497), that
#   Connect a bank and the reason banks cannot be used appear, that only
#   its button starts it, what it says while it runs and the summary.
#
# - SheetSmoke.qml opens the Properties dialog of the Overview (M6-B197) on
#   a stand-in for ComponentProperties and checks that it shows the panel and
#   the preview, not only its title and Close, fits a low window and opens
#   again.
#
# - SpecSmoke.qml opens the description field of a custom chart (M6e.3) on
#   stand-ins for ChartDescription and ComponentProperties and checks the
#   check as you type, that only "Use this description" stores the text, the
#   list of the parts that are not supported and "Draw it without these".
#
# - PermissionsSmoke.qml puts AiPermissionGrid.qml on stand-ins for AiSwitch
#   and AiSettings and checks that a field switch that is on means the AI may
#   write it, as in the grid (#631), and what each switch stores.
#
# - TransactionSmoke.qml opens TransactionPage.qml on a stand-in for
#   AccountRegister and checks that a note typed but not saved is saved
#   before "Join with the other side of a move…" (M6-B227).
#
# - CategorySmoke.qml opens TransactionPage.qml on the same stand-in and
#   checks that the category box shows the stored category after the
#   direction changes, after the book changed while the page was open, and
#   for a category its list doesn't offer (M6-B234).
# - PartsSmoke.qml opens PartsPage.qml on a stand-in for AccountRegister and
#   checks the line that says what the Parts add up to and removing a Part,
#   with no KI18n warning about a missing argument (M6-B235).
# - TagSmoke.qml opens TransactionPage.qml on the same stand-in and checks
#   the Tags field: a tag chosen, one made by typing, one taken away with
#   the keyboard, and that a save sends the tags only when they changed
#   (US4.8).
# - TagsSmoke.qml opens Settings › Tags on a stand-in for BookTags and
#   checks the use counts in words, renaming and merging by ID, and that a
#   refused name keeps its reason (US4.8).
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

# The pages and the panel, as a module with the app's URI, so their files see
# each other and the stand-ins as they see the Rust objects.
module="$work/imports/io/github/vondor/kassensturz"
mkdir -p "$module"
for file in StatisticsPage AiChangeReviewPage AiChangeCorrection AiBudgetChange AiRulesSortChange AiJoinTransferChange AiChartCard ComponentPreview PropertiesPanel PropertiesSheet ChartView ChartOverlays ChartTable DataTable \
    CategoryGroupsPage CategoryGroupPage AmountLabel AmountLine KpiCard KpiGrid KpiNames ProgressMeter ChartPalette FormError Plain ActionLog \
    DashboardGrid GridItem GridNames TitleWords AddMenu FormulaField FormulaWords PeriodChoice StatisticWizard ColumnBoard \
    BudgetBars BudgetWords BudgetsPage BudgetPage FilterBar FilterPicker FilterNotes UpdateAllWidget PeriodHeader DashboardTabs SpecPage ChartExamples ChartColorsCard AiPermissionGrid \
    AlarmList AlarmRuleDialog AlarmRulesField AlarmWords \
    TransactionPage TagField TagsPage TransferPrompt TransferPartnerDialog RemoveOriginalDialog PartsPage PartRow TransactionRow; do
    cp "$bridge/qml/$file.qml" "$module"
done
cp "$bridge"/qml-tests/fakes/*.qml "$module"
{
    echo 'module io.github.vondor.kassensturz'
    echo 'singleton ChartPalette 1.0 ChartPalette.qml'
    echo 'singleton Plain 1.0 Plain.qml'
    echo 'singleton ActionLog 1.0 ActionLog.qml'
    for file in StatisticsPage AiChangeReviewPage AiChangeCorrection AiBudgetChange AiRulesSortChange AiJoinTransferChange AiChartCard ComponentPreview PropertiesPanel PropertiesSheet ChartView ChartOverlays \
        ChartTable DataTable AmountLabel AmountLine KpiCard KpiGrid KpiNames ProgressMeter FormError SavedViews \
        ComponentProperties BookCharts BookCards AiSwitch AiChat AccountTypes AiErrorMessage \
        CategoryGroupsPage CategoryGroupPage CategoryGroups DashboardGrid GridItem GridNames TitleWords DashboardLayout \
        FormulaField FormulaWords BookFormula PeriodChoice PeriodRange StatisticWizard ColumnBoard AddMenu \
        AlarmList AlarmRuleDialog AlarmRulesField AlarmWords BookAlarms \
        BudgetBars BudgetWords BudgetsPage BudgetPage BudgetBook BookBudgets FilterBar FilterPicker FilterNotes DeveloperMode UpdateAllWidget PeriodHeader DashboardTabs BankConnection SpecPage ChartExamples ChartDescription ChartColorsCard ChartPaletteSettings AiPermissionGrid AiSettings \
        TransactionPage TagField TagsPage TransferPrompt TransferPartnerDialog RemoveOriginalDialog AccountRegister BookTags PartsPage PartRow TransactionRow BookTransactions; do
        echo "$file 1.0 $file.qml"
    done
} > "$module/qmldir"
cp "$bridge"/qml-tests/PanelSmoke.qml "$bridge"/qml-tests/StatisticsSmoke.qml "$bridge"/qml-tests/ReviewSmoke.qml "$bridge"/qml-tests/SortSmoke.qml "$bridge"/qml-tests/ChatChartSmoke.qml "$bridge"/qml-tests/GroupsSmoke.qml "$bridge"/qml-tests/GridSmoke.qml "$bridge"/qml-tests/BudgetsSmoke.qml "$bridge"/qml-tests/FilterSmoke.qml "$bridge"/qml-tests/UpdateAllSmoke.qml "$bridge"/qml-tests/PageWidgetsSmoke.qml "$bridge"/qml-tests/SpecSmoke.qml "$bridge"/qml-tests/ColorsSmoke.qml "$bridge"/qml-tests/SheetSmoke.qml "$bridge"/qml-tests/PermissionsSmoke.qml "$bridge"/qml-tests/TransactionSmoke.qml "$bridge"/qml-tests/CategorySmoke.qml "$bridge"/qml-tests/PartsSmoke.qml "$bridge"/qml-tests/AlarmsSmoke.qml "$bridge"/qml-tests/TagSmoke.qml "$bridge"/qml-tests/TagsSmoke.qml "$work"
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

run_smoke PanelSmoke
echo "properties panel: ok"
run_smoke PermissionsSmoke
echo "AI field switches: ok"
run_smoke TransactionSmoke
echo "transaction page keeps a typed note: ok"
run_smoke CategorySmoke
echo "transaction page shows the stored category: ok"
run_smoke PartsSmoke
echo "parts page: ok"
run_smoke AlarmsSmoke
echo "warnings tile: ok"
run_smoke TagSmoke
echo "transaction page tags: ok"
run_smoke TagsSmoke
echo "settings tags page: ok"
run_smoke StatisticsSmoke
echo "statistics page: ok"
run_smoke ReviewSmoke
echo "review page: ok"
run_smoke SortSmoke
echo "sort with all Rules in the review: ok"
run_smoke ChatChartSmoke
echo "chat chart card: ok"
run_smoke GroupsSmoke
echo "groups pages: ok"
run_smoke GridSmoke
echo "dashboard grid: ok"
run_smoke BudgetsSmoke
echo "budget pages: ok"
run_smoke FilterSmoke
echo "filter bar: ok"
run_smoke UpdateAllSmoke
echo "update all accounts: ok"
run_smoke PageWidgetsSmoke
echo "period header, filter bar and tabs widgets: ok"
# Again with the desktop style the app uses: its TabBar complained about a
# bar with no tab (M6-B101), which the plain style never shows.
(
    export QT_QUICK_CONTROLS_STYLE=org.kde.desktop
    run_smoke PageWidgetsSmoke
)
echo "the same with the desktop style: ok"
run_smoke SpecSmoke
echo "custom chart description: ok"
run_smoke ColorsSmoke
echo "chart colors in settings: ok"
run_smoke SheetSmoke
echo "properties dialog of the overview: ok"
# Again with the desktop style the app uses, whose controls are sized by
# the platform (M6-B197).
(
    export QT_QUICK_CONTROLS_STYLE=org.kde.desktop
    run_smoke SheetSmoke
)
echo "the same with the desktop style: ok"
