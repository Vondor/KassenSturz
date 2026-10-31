// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One chart (M6a.4, D-276, D-283): line, bar, stacked bar, area, pie or
// donut, drawn by KQuickCharts from the drawing values of a chart model, with
// every word and number around it as text from Rust (D-065): the value axis,
// the categories, the legend and the total. KQuickCharts draws no axes, so
// they are drawn here from that text.
//
// It is a keyboard stop: Left and Right move between points and the text of
// the point in focus is its accessible name; "Show as table" swaps the
// picture for the same values as a table, which is also what a screen reader
// reads. Nothing is only in a color: the first four series have a second
// mark, and nothing shows only on hover.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import org.kde.quickcharts as Charts

ColumnLayout {
    id: view

    // One chart from `BookCharts.view.charts`.
    required property var chart
    // What the chart is called, e.g. "Where your money went".
    property string title: ""
    property bool showTable: false
    // The point in focus, or -1.
    property int current: -1
    // The legend entry the pointer rests on and where, for the one tooltip
    // below, which is a popup of the chart and never part of the legend's
    // layout.
    property int legendTip: -1
    property point legendPoint: Qt.point(0, 0)

    readonly property bool round: chart.round
    readonly property int count: chart.count
    readonly property var ticks: chart.ticks
    readonly property bool lines: chart.kind === "line" || chart.kind === "area"
    readonly property real low: ticks.length > 0 ? ticks[0].value : 0
    readonly property real high: ticks.length > 0 ? ticks[ticks.length - 1].value : 1

    // One series of plain bars whose names would need more than two lines
    // under their bars is drawn as rows instead: the whole name on the left,
    // the bar to its right (M6-B3). The width is an estimate (the chart's own
    // width less the value axis), so the choice never depends on the picture.
    readonly property bool horizontal: chart.kind === "bar" && chart.series.length === 1 && count > 0
        && longestName.advanceWidth > 2 * (view.width - Kirigami.Units.gridUnit * 4) / count

    TextMetrics {
        id: longestName

        font: Kirigami.Theme.smallFont
        text: {
            let found = "";
            for (let index = 0; index < view.count; ++index) {
                const name = view.categoryName(index);
                if (name.length > found.length) {
                    found = name;
                }
            }
            return found;
        }
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function seriesName(key: string): string {
        // A series the user chose (a category or one of their groups) is
        // named by the book.
        for (const series of chart.series) {
            if (series.key === key && series.name !== "") {
                return series.name;
            }
        }
        switch (key) {
        case "spending":
            return i18nContext.i18nc("@label a series of a chart", "Spending");
        case "income":
            return i18nContext.i18nc("@label a series of a chart", "Money in");
        case "expenses":
        case "money_out":
            return i18nContext.i18nc("@label a series of a chart", "Money out");
        case "money_in":
            return i18nContext.i18nc("@label a series of a chart", "Money in");
        case "net_worth":
            return i18nContext.i18nc("@label a series of a chart: what the user has less what they owe", "What you are worth");
        }
        return "";
    }

    // "Hidden: Rent and 1 more (1,250.00 € not shown)"; empty when the chart
    // leaves nothing out.
    function hiddenNote(): string {
        const hidden = chart.hidden;
        if (hidden.first === undefined) {
            return "";
        }
        return hidden.more > 0
            ? i18nContext.i18nc("@info %1 is a category name, %2 an amount, %3 the number of other hidden categories", "Hidden: %1 and %3 more (%2 not shown)", hidden.first, hidden.total, hidden.more)
            : i18nContext.i18nc("@info %1 is a category name, %2 an amount", "Hidden: %1 (%2 not shown)", hidden.first, hidden.total);
    }

    FontMetrics {
        id: nameFont

        font: Kirigami.Theme.smallFont
    }

    function categoryName(index: int): string {
        if (round && chart.slices.other === index) {
            return i18nContext.i18nc("@label the smaller slices of a pie put together", "Other");
        }
        return chart.unsortedIndex === index
            ? i18nContext.i18nc("@label money with no category yet", "Not sorted yet")
            : chart.categories[index];
    }

    function isOther(index: int): bool {
        return round && chart.slices.other === index;
    }

    // A row of a round chart that can't be a slice: money that came back.
    function undrawn(index: int): bool {
        return round && chart.slices.undrawn[index] === true;
    }

    // What "Other" holds, largest first.
    function holdsText(): string {
        const parts = [];
        for (const held of chart.slices.holds) {
            const name = held.name === "" ? i18nContext.i18nc("@label money with no category yet", "Not sorted yet") : held.name;
            parts.push(i18nContext.i18nc("@label %1 is a category, %2 an amount", "%1 %2", name, held.label));
        }
        return parts.join(", ");
    }

    // One row of a round chart's legend: name, amount and share.
    function legendText(index: int): string {
        const label = chart.series[0].points[index].label;
        return undrawn(index)
            ? i18nContext.i18nc("@label %1 is a category, %2 an amount of money that came back", "%1: %2 (money that came back, not drawn)", categoryName(index), label)
            : i18nContext.i18nc("@label %1 is a category, %2 an amount, %3 its share of the total", "%1: %2, %3", categoryName(index), label, chart.slices.shares[index] ?? "");
    }

    // What the point in focus says, for the screen reader and for the eyes.
    function pointText(index: int): string {
        if (index < 0 || index >= count) {
            return "";
        }
        if (round) {
            return isOther(index)
                ? i18nContext.i18nc("@label %1 is the text of a legend row, %2 the categories in Other", "%1. It holds: %2", legendText(index), holdsText())
                : legendText(index);
        }
        const parts = [];
        for (const series of chart.series) {
            parts.push(chart.series.length > 1
                ? i18nContext.i18nc("@label %1 is a series like Money in, %2 an amount", "%1: %2", seriesName(series.key), series.points[index].label)
                : series.points[index].label);
        }
        return i18nContext.i18nc("@label %1 is a category or month, %2 the values there", "%1: %2", categoryName(index), parts.join(", "));
    }

    // Where the names of a round chart's slices go in a picture of `width`
    // by `height`: for each slice a box (`x`, `y`, `width`, `height`) or null
    // when the slice is too small to carry its name (the model's `named`) or
    // when its name would cover a bigger slice's name in every place tried.
    // The biggest slices are placed first, each at the usual distance from the
    // middle, then nearer to and farther from it; the legend and the table
    // have every name, so a name left out loses nothing. The places depend on
    // the chart and the size only (hover must never change geometry).
    function sliceLabelBoxes(width: real, height: real): var {
        const boxes = [];
        for (let index = 0; index < count; ++index) {
            boxes.push(null);
        }
        if (!round || count === 0 || width <= 0 || height <= 0) {
            return boxes;
        }
        const padding = Kirigami.Units.smallSpacing;
        const radius = Math.min(width, height) / 2;
        const donut = chart.kind === "donut";
        const reaches = donut
            ? [radius - Kirigami.Units.gridUnit * 1.25, radius - Kirigami.Units.gridUnit * 0.75, radius - Kirigami.Units.gridUnit * 1.75]
            : [radius * 0.62, radius * 0.82, radius * 0.42];
        const points = chart.series[0].points;
        const order = [];
        for (let index = 0; index < count; ++index) {
            if (chart.slices.named[index] === true) {
                order.push(index);
            }
        }
        order.sort((a, b) => points[b].value - points[a].value || a - b);
        const height1 = nameFont.height + padding;
        const placed = [];
        for (const index of order) {
            const boxWidth = Math.min(nameFont.advanceWidth(categoryName(index)), Kirigami.Units.gridUnit * 7) + padding * 2;
            const angle = sliceMiddle(index);
            for (const reach of reaches) {
                const box = {
                    x: Math.max(0, Math.min(width - boxWidth, width / 2 + reach * Math.sin(angle) - boxWidth / 2)),
                    y: Math.max(0, Math.min(height - height1, height / 2 - reach * Math.cos(angle) - height1 / 2)),
                    width: boxWidth,
                    height: height1
                };
                if (!placed.some(other => boxesMeet(box, other, padding / 2))) {
                    boxes[index] = box;
                    placed.push(box);
                    break;
                }
            }
        }
        return boxes;
    }

    // Whether two boxes touch or cover each other, with `gap` to spare.
    function boxesMeet(one: var, other: var, gap: real): bool {
        return one.x < other.x + other.width + gap && other.x < one.x + one.width + gap
            && one.y < other.y + other.height + gap && other.y < one.y + one.height + gap;
    }

    // How many pairs of slice names on the picture cover each other; the
    // checks of the charts (tools/check-charts.sh) read this.
    function nameOverlaps(): int {
        let found = 0;
        for (let one = 0; one < sliceNames.count; ++one) {
            for (let other = one + 1; other < sliceNames.count; ++other) {
                const first = sliceNames.itemAt(one);
                const second = sliceNames.itemAt(other);
                if (first && second && first.visible && second.visible && boxesMeet(first, second, 0)) {
                    ++found;
                }
            }
        }
        return found;
    }

    // Where slice `index` of a round chart is, as the angle of its middle in
    // radians from twelve o'clock, clockwise. The drawing values only place
    // marks (AGENTS.md §5).
    function sliceMiddle(index: int): real {
        const points = chart.series[0].points;
        let whole = 0;
        for (const point of points) {
            whole += point.value;
        }
        let before = 0;
        for (let at = 0; at < index; ++at) {
            before += points[at].value;
        }
        return whole > 0 ? (before + points[index].value / 2) / whole * 2 * Math.PI : 0;
    }

    // The slice under a point (`dx`, `dy` from the middle, `radius` of the
    // picture), or -1 outside the circle or on the hole of a donut.
    function sliceAt(dx: real, dy: real, radius: real): int {
        const distance = Math.sqrt(dx * dx + dy * dy);
        const hole = chart.kind === "donut" ? radius - Kirigami.Units.gridUnit * 2.5 : 0;
        if (distance > radius || distance < hole) {
            return -1;
        }
        let angle = Math.atan2(dx, -dy);
        if (angle < 0) {
            angle += 2 * Math.PI;
        }
        const points = chart.series[0].points;
        let whole = 0;
        for (const point of points) {
            whole += point.value;
        }
        let upTo = 0;
        for (let index = 0; index < points.length; ++index) {
            upTo += points[index].value;
            if (whole > 0 && angle <= upTo / whole * 2 * Math.PI && points[index].value > 0) {
                return index;
            }
        }
        return -1;
    }

    function summary(): string {
        return i18nContext.i18nc("@label %1 is the title, %2 the number of points, %3 the currency, %4 an amount", "%1. %2 values in %3, total %4.", title, count, chart.currency, chart.total);
    }

    spacing: Kirigami.Units.smallSpacing
    activeFocusOnTab: true
    focus: true

    Accessible.role: Accessible.Chart
    Accessible.name: current >= 0 ? pointText(current) : title
    Accessible.description: summary()

    Keys.onRightPressed: current = Math.min(count - 1, current + 1)
    Keys.onLeftPressed: current = Math.max(0, current - 1)
    Keys.onDownPressed: current = Math.min(count - 1, current + 1)
    Keys.onUpPressed: current = Math.max(0, current - 1)
    onActiveFocusChanged: if (!activeFocus) current = -1

    Kirigami.Heading {
        Layout.fillWidth: true
        level: 3
        text: view.title
        visible: text.length > 0
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }

    // The chart: the value axis on the left, the picture, the categories
    // below.
    RowLayout {
        Layout.fillWidth: true
        visible: !view.showTable && !view.horizontal
        spacing: Kirigami.Units.smallSpacing

        Item {
            id: axis

            visible: !view.round && view.ticks.length > 0
            Layout.preferredWidth: visible ? labelWidth.width + Kirigami.Units.smallSpacing : 0
            Layout.preferredHeight: plot.height
            Layout.alignment: Qt.AlignTop

            // The widest tick label sets the width of the axis.
            TextMetrics {
                id: labelWidth
                property string widest: {
                    let found = "";
                    for (const tick of view.ticks) {
                        if (tick.label.length > found.length) {
                            found = tick.label;
                        }
                    }
                    return found;
                }
                text: widest
            }

            Repeater {
                model: view.ticks

                delegate: Controls.Label {
                    required property var modelData

                    anchors.right: parent.right
                    y: (1 - (modelData.value - view.low) / (view.high - view.low)) * (parent.height - height) 
                    text: modelData.label
                    textFormat: Text.PlainText
                    horizontalAlignment: Text.AlignRight
                    color: Kirigami.Theme.textColor
                    Accessible.ignored: true
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Item {
                id: plot

                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.minimumHeight: Kirigami.Units.gridUnit * 12
                Layout.minimumWidth: Kirigami.Units.gridUnit * 12

                // Where the names of a round chart's slices go; null for a
                // slice whose name is left to the legend.
                readonly property var sliceBoxes: view.sliceLabelBoxes(width, height)

                // Grid lines at the ticks.
                Repeater {
                    model: view.round ? [] : view.ticks

                    delegate: Rectangle {
                        required property var modelData

                        width: plot.width
                        height: 1
                        y: (1 - (modelData.value - view.low) / (view.high - view.low)) * (plot.height - 1)
                        color: Kirigami.Theme.disabledTextColor
                        opacity: 0.35
                    }
                }

                Loader {
                    id: drawing

                    anchors.fill: parent
                    sourceComponent: view.round ? pie : (view.lines ? line : bar)
                }

                // A new chart gets new sources of values; the old ones go with
                // the old picture.
                Connections {
                    target: view

                    function onChartChanged() {
                        drawing.active = false;
                        drawing.active = true;
                    }
                }

                // The second mark of each bar's series, in the bar (D-283),
                // where there is more than one series to tell apart.
                Repeater {
                    model: !view.round && !view.lines && view.chart.series.length > 1
                        ? view.chart.series.length * view.count : 0

                    delegate: Controls.Label {
                        required property int index
                        readonly property int seriesIndex: Math.floor(index / view.count)
                        readonly property int pointIndex: index % view.count
                        readonly property var place: view.barPlace(seriesIndex, pointIndex, plot.width, plot.height, height)
                        readonly property int marker: view.chart.series[seriesIndex].marker

                        visible: place !== null && marker >= 0
                        x: place ? place.x - width / 2 : 0
                        y: place ? place.y : 0
                        text: ["■", "●", "▲", "◆"][marker] ?? ""
                        textFormat: Text.PlainText
                        color: "white"
                        Accessible.ignored: true
                    }
                }

                // The names of the slices that are big enough (5 %), on the
                // slices themselves; the legend has every slice. Where two
                // names would cover each other the smaller slice's name is
                // left to the legend (`sliceLabelBoxes`). The places depend
                // only on the chart and the picture's size, never on the
                // pointer, so pointing at a slice moves nothing.
                Repeater {
                    id: sliceNames

                    model: view.round ? view.count : 0

                    delegate: Rectangle {
                        required property int index
                        readonly property var box: plot.sliceBoxes[index] ?? null

                        visible: box !== null
                        x: box ? box.x : 0
                        y: box ? box.y : 0
                        width: box ? box.width : 0
                        height: box ? box.height : 0
                        radius: 3
                        color: Qt.rgba(Kirigami.Theme.backgroundColor.r, Kirigami.Theme.backgroundColor.g, Kirigami.Theme.backgroundColor.b, 0.8)

                        Controls.Label {
                            anchors.centerIn: parent
                            width: Math.max(0, parent.width - Kirigami.Units.smallSpacing * 2)
                            text: view.categoryName(parent.index)
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            horizontalAlignment: Text.AlignHCenter
                            font: Kirigami.Theme.smallFont
                            Accessible.ignored: true
                        }
                    }
                }

                // Pointing at a slice says what the legend says about it.
                HoverHandler {
                    enabled: view.round
                    onPointChanged: view.current = view.sliceAt(point.position.x - plot.width / 2, point.position.y - plot.height / 2, Math.min(plot.width, plot.height) / 2)
                    onHoveredChanged: if (!hovered) view.current = -1
                }

                // The point in focus: a frame around its place.
                Rectangle {
                    visible: view.current >= 0 && !view.round
                    color: "transparent"
                    border.width: 2
                    border.color: Kirigami.Theme.highlightColor
                    y: 0
                    height: plot.height
                    width: view.lines
                        ? Math.max(Kirigami.Units.gridUnit, plot.width / Math.max(1, view.count - 1))
                        : plot.width / Math.max(1, view.count)
                    x: view.lines
                        ? (view.count > 1 ? view.current * plot.width / (view.count - 1) : 0) - width / 2
                        : view.current * plot.width / Math.max(1, view.count)
                }
            }

            // The categories under the picture; every category gets its slot,
            // a long name wraps to at most two lines and is shown whole in the
            // point's text.
            Item {
                visible: !view.round
                Layout.fillWidth: true
                Layout.preferredHeight: categoryHeight.rowHeight * 2

                TextMetrics {
                    id: categoryHeight
                    text: "M"
                    property real rowHeight: tightBoundingRect.height + Kirigami.Units.smallSpacing * 2
                }

                Repeater {
                    model: view.round ? 0 : view.count

                    delegate: Controls.Label {
                        required property int index

                        readonly property real slot: view.lines && view.count > 1
                            ? plot.width / (view.count - 1) : plot.width / Math.max(1, view.count)
                        readonly property bool first: view.lines && index === 0
                        readonly property bool last: view.lines && view.count > 1 && index === view.count - 1
                        width: slot
                        x: view.lines
                            ? (view.count > 1 ? index * slot : 0) - (first ? 0 : (last ? slot : slot / 2))
                            : index * slot
                        horizontalAlignment: first ? Text.AlignLeft : (last ? Text.AlignRight : Text.AlignHCenter)
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        text: view.categoryName(index)
                        textFormat: Text.PlainText
                        font: Kirigami.Theme.smallFont
                        color: Kirigami.Theme.textColor
                        Accessible.ignored: true
                    }
                }
            }
        }
    }

    // Long category names: one row per category, the name whole (M6-B3).
    ColumnLayout {
        Layout.fillWidth: true
        visible: !view.showTable && view.horizontal
        spacing: Kirigami.Units.smallSpacing

        Repeater {
            model: view.horizontal ? view.count : 0

            delegate: RowLayout {
                id: barRow

                required property int index
                readonly property var point: view.chart.series[0].points[index]

                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    // One fixed width, so the wrapped name settles its height at
                    // once and the row does not rearrange again (M6-B17).
                    Layout.preferredWidth: view.width * 0.4
                    Layout.minimumWidth: view.width * 0.4
                    Layout.maximumWidth: view.width * 0.4
                    text: view.categoryName(barRow.index)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    font: Kirigami.Theme.smallFont
                    Accessible.ignored: true
                }

                Item {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 1.5

                    Rectangle {
                        width: view.high > 0 ? Math.max(0, Math.min(1, barRow.point.value / view.high)) * parent.width : 0
                        height: parent.height
                        color: ChartPalette.color(barRow.point.color)
                        border.width: view.current === barRow.index ? 2 : 0
                        border.color: Kirigami.Theme.highlightColor
                    }
                }

                Controls.Label {
                    text: barRow.point.label
                    textFormat: Text.PlainText
                    font: Kirigami.Theme.smallFont
                    Accessible.ignored: true
                }
            }
        }
    }

    // The text of the point in focus, also for those who see it.
    Controls.Label {
        Layout.fillWidth: true
        visible: !view.showTable && view.current >= 0
        text: view.pointText(view.current)
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        Accessible.ignored: true
    }

    Controls.ToolTip {
        parent: view
        x: view.legendPoint.x + Kirigami.Units.largeSpacing
        y: view.legendPoint.y + Kirigami.Units.gridUnit
        visible: view.round && view.legendTip >= 0
        text: view.legendTip >= 0 ? view.pointText(view.legendTip) : ""
        delay: Kirigami.Units.toolTipDelay
    }

    // The legend: color, second mark and name, never the color alone.
    Flow {
        Layout.fillWidth: true
        visible: !view.showTable
        spacing: Kirigami.Units.largeSpacing

        Repeater {
            model: view.round ? view.count : view.chart.series.length

            delegate: RowLayout {
                id: entry

                required property int index
                readonly property var series: view.round ? null : view.chart.series[index]
                readonly property int color: view.round
                    ? view.chart.series[0].points[index].color : series.color
                readonly property int marker: view.round
                    ? view.chart.series[0].points[index].marker : series.marker

                spacing: Kirigami.Units.smallSpacing

                Rectangle {
                    implicitWidth: Kirigami.Units.gridUnit
                    implicitHeight: Kirigami.Units.gridUnit
                    radius: entry.marker === 1 ? width / 2 : (entry.marker === 2 ? 0 : 2)
                    rotation: entry.marker === 3 ? 45 : 0
                    // A row that can't be a slice has no color of its own.
                    color: view.undrawn(entry.index) ? "transparent" : ChartPalette.color(entry.color)
                    border.width: view.undrawn(entry.index) ? 1 : 0
                    border.color: Kirigami.Theme.disabledTextColor

                    Controls.Label {
                        anchors.centerIn: parent
                        // The second mark, for those who can't tell the colors
                        // apart: a letter-free glyph per series.
                        text: ["■", "●", "▲", "◆"][entry.marker] ?? ""
                        visible: entry.marker >= 0 && entry.marker <= 3
                        textFormat: Text.PlainText
                        font.pixelSize: parent.height * 0.6
                        color: "white"
                        rotation: entry.marker === 3 ? -45 : 0
                        Accessible.ignored: true
                    }
                }

                Controls.Label {
                    text: view.round ? view.legendText(entry.index) : view.seriesName(entry.series.key)
                    textFormat: Text.PlainText
                    wrapMode: view.round ? Text.Wrap : Text.NoWrap
                    elide: view.round ? Text.ElideNone : Text.ElideRight
                    Layout.maximumWidth: Kirigami.Units.gridUnit * (view.round ? 22 : 14)
                    // Underlined, not bold: bold is wider, which re-wrapped the
                    // legend under the pointer, lost the hover, and made it
                    // flicker (M6-B18).
                    font.underline: view.current === entry.index
                }

                HoverHandler {
                    id: legendHover

                    enabled: view.round
                    // Hover only sets a number and a point; nothing that
                    // hover shows may take room in the legend (M6-B18).
                    onHoveredChanged: {
                        view.current = hovered ? entry.index : -1;
                        view.legendTip = hovered ? entry.index : -1;
                    }
                    onPointChanged: if (hovered) view.legendPoint = entry.mapToItem(view, point.position)
                }

            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: !view.showTable
        text: view.chart.kind === "pie" || view.chart.kind === "donut" || view.chart.dataset === "spending_by_category"
            ? i18nContext.i18nc("@label %1 is an amount of money", "Total spent: %1", view.chart.total)
            : view.chart.dataset === "net_worth_over_time"
            ? i18nContext.i18nc("@label %1 is an amount of money", "What you are worth at the end: %1", view.chart.total)
            : view.chart.dataset === "cash_flow_by_month"
            ? i18nContext.i18nc("@label %1 is an amount of money, the money in less the money out of the accounts", "Net change in total: %1", view.chart.total)
            : i18nContext.i18nc("@label %1 is an amount of money", "Left over in total: %1", view.chart.total)
        textFormat: Text.PlainText
        font.bold: true
    }

    // What "Other" holds, under the chart and under its table.
    Controls.Label {
        Layout.fillWidth: true
        visible: view.round && view.chart.slices.other >= 0
        text: i18nContext.i18nc("@info %1 is the categories in Other with their amounts", "Other holds: %1", view.holdsText())
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }

    // Hidden categories are never silently missing (D-291): the same line
    // stays under the chart and under its table.
    Controls.Label {
        Layout.fillWidth: true
        visible: view.chart.hidden.first !== undefined
        text: view.hiddenNote()
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }

    ChartTable {
        Layout.fillWidth: true
        visible: view.showTable
        chart: view.chart
    }

    Controls.Button {
        text: view.showTable
            ? i18nContext.i18nc("@action:button", "Show as chart")
            : i18nContext.i18nc("@action:button", "Show as table")
        icon.name: view.showTable ? "office-chart-bar" : "view-list-details"
        onClicked: view.showTable = !view.showTable
    }

    Item {
        Layout.fillHeight: true
    }

    // Line and area: one source of values per series.
    Component {
        id: line

        Charts.LineChart {
            id: lineChart

            yRange { from: view.low; to: view.high; automatic: false }
            lineWidth: 3
            fillOpacity: view.chart.kind === "area" ? 0.35 : 0
            interpolate: false
            stacked: false
            indexingMode: Charts.Chart.IndexEachSource
            // The second mark of a series, on each of its points (D-283).
            nameSource: Charts.ArraySource {
                array: view.chart.series.map(series => series.key)
            }
            pointDelegate: Controls.Label {
                readonly property string key: Charts.LineChart.name
                readonly property int marker: view.markerOf(key)

                // The chart puts the delegate's corner on the point.
                transform: Translate {
                    x: -width / 2
                    y: -height / 2
                }
                visible: marker >= 0
                text: ["■", "●", "▲", "◆"][marker] ?? ""
                color: Charts.LineChart.color
                textFormat: Text.PlainText
                Accessible.ignored: true
            }
            colorSource: Charts.ArraySource {
                array: view.chart.series.map(series => ChartPalette.color(series.color))
            }
            valueSources: view.sources(lineChart)
        }
    }

    // Bars side by side or piled up.
    Component {
        id: bar

        Charts.BarChart {
            id: barChart

            yRange { from: view.low; to: view.high; automatic: false }
            stacked: view.chart.kind === "stacked-bar"
            spacing: Kirigami.Units.smallSpacing
            indexingMode: Charts.Chart.IndexEachSource
            colorSource: Charts.ArraySource {
                array: view.chart.series.map(series => ChartPalette.color(series.color))
            }
            valueSources: view.sources(barChart)
        }
    }

    // Slices; a donut is a pie with a thickness.
    Component {
        id: pie

        Charts.PieChart {
            id: pieChart

            filled: view.chart.kind !== "donut"
            thickness: Kirigami.Units.gridUnit * 2.5
            spacing: 1
            indexingMode: Charts.Chart.IndexSourceValues
            colorSource: Charts.ArraySource {
                array: view.chart.series[0].points.map(point => ChartPalette.color(point.color))
            }
            valueSources: view.sources(pieChart)
        }
    }

    // The sources of values of a chart, one per series, from the drawing
    // values: they only place the marks (AGENTS.md §5).
    // Where the mark of bar (`series`, `point`) goes in a picture of `width` by
    // `height`: near the top of the bar, or null when the bar is too short
    // for it, then the table has the value. `size` is the mark's height.
    function barPlace(series: int, point: int, width: real, height: real, size: real): var {
        const stacked = chart.kind === "stacked-bar";
        const slot = width / count;
        const group = chart.series.length;
        let from = 0;
        let to = chart.series[series].points[point].value;
        if (stacked) {
            for (let before = 0; before < series; ++before) {
                const value = chart.series[before].points[point].value;
                if (value > 0) {
                    from += value;
                }
            }
            to += from;
        }
        const span = high - low;
        const top = Math.max(0, (1 - (to - low) / span) * height);
        const bottom = (1 - (from - low) / span) * height;
        if (to <= from || bottom - top < size * 1.2) {
            return null;
        }
        return {
            x: stacked ? (point + 0.5) * slot : point * slot + (series + 0.5) * slot / group,
            y: top + size * 0.2
        };
    }

    function markerOf(key: string): int {
        for (const series of chart.series) {
            if (series.key === key) {
                return series.marker;
            }
        }
        return -1;
    }

    function sources(parentItem: var): var {
        return chart.series.map(series => sourceComponent.createObject(parentItem, {
            array: series.points.map(point => point.value)
        }));
    }

    Component {
        id: sourceComponent

        Charts.ArraySource {}
    }
}
