// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One chart (M6a.4, D-276, D-283): line, bar, stacked bar, area, pie,
// donut or candles (US4.10), drawn by KQuickCharts (the candles by plain
// rectangles) from the drawing values of a chart model, with every word
// and number around it as text from Rust (D-065): the value axis, the
// categories, the legend and the total. KQuickCharts draws no axes, so
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
    // Which way bars go (`props.orientation`, D-679): `auto`, `horizontal`
    // or `vertical`. Only bars and stacked bars read it.
    property string orientation: "auto"
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
    readonly property bool lines: chart.kind === "line" || chart.kind === "area" || chart.kind === "stacked-area"
    // The balance as candles (US4.10, D-718): the series are start, highest,
    // lowest and end. KQuickCharts has no candles, so they are rectangles
    // placed by the drawing values, as the overlays are.
    readonly property bool candles: chart.kind === "candlestick"
    // Which candles have a filled box (`props.candle_fill`, D-748): by
    // default a candle that ended higher is hollow and one that ended lower
    // filled (D-718).
    property string candleFill: "up-hollow"

    // Whether a candle's box is hollow: `rose` when it ended at least where
    // it started.
    function candleHollow(rose: bool): bool {
        switch (candleFill) {
        case "down-hollow":
            return !rose;
        case "all-filled":
            return false;
        case "all-hollow":
            return true;
        }
        return rose;
    }
    // The layers a custom chart puts over its own marks (M6e.2); built-in
    // charts have none.
    readonly property var overlays: chart.overlays ?? []
    // The free colors of a custom chart, as text: a color index from the
    // palette's size on is one of these (M7a.10).
    readonly property var extraColors: chart.extraColors ?? []
    // Every series the picture has, for the legend: the chart's own, then
    // those of its overlays. The names of overlay series are the user's own
    // text from the chart description.
    readonly property var legendEntries: {
        // Candles tell up from down by their color and, by default, by
        // being hollow or filled, so not by the color alone (D-283); the
        // user may fill all or none (D-748), and the legend shows the boxes
        // as drawn.
        if (candles) {
            return [
                {color: 0, marker: -1, hollow: candleHollow(true), name: i18nContext.i18nc("@label legend of a candle chart", "Ended higher than it started")},
                {color: 1, marker: -1, hollow: candleHollow(false), name: i18nContext.i18nc("@label legend of a candle chart", "Ended lower than it started")}
            ];
        }
        const found = chart.series.map(series => ({color: series.color, marker: series.marker, name: seriesName(series.key)}));
        for (const overlay of overlays) {
            for (const series of overlay.series) {
                found.push({color: series.color, marker: series.marker, name: series.name});
            }
        }
        return found;
    }
    readonly property real low: ticks.length > 0 ? ticks[0].value : 0
    readonly property real high: ticks.length > 0 ? ticks[ticks.length - 1].value : 1

    // Bars drawn as rows: the whole name on the left, the bars to its right.
    // The user can choose rows or upright bars for bars and stacked bars
    // (D-679). Automatic: plain bars, of one series or several side by side,
    // whose names would need more than two lines under their bars (M6-B3),
    // or whose longest word is wider than a bar's slot and would break
    // inside the word (M6-B229). The width is an estimate (the chart's own
    // width less the value axis), so the choice never depends on the picture.
    readonly property bool barKind: chart.kind === "bar" || chart.kind === "stacked-bar"
    readonly property real slotEstimate: (view.width - Kirigami.Units.gridUnit * 4) / Math.max(1, count)
    readonly property bool horizontal: barKind && count > 0 && overlays.length === 0
        && (orientation === "horizontal"
            || orientation !== "vertical" && chart.kind === "bar"
                && (longestName.advanceWidth > 2 * slotEstimate
                    || longestWord.advanceWidth > slotEstimate - Kirigami.Units.smallSpacing))
    readonly property bool stacked: chart.kind === "stacked-bar"
    // The series drawn as upright bars side by side: a series that is zero in
    // every category gets no slot, so the other bars sit in the middle of
    // their names instead of beside an empty place (#709, US4 P3 R3b). It
    // stays in the legend and the table. Every series, if all are zero.
    readonly property var barSeries: {
        const all = chart.series.map((series, index) => index);
        if (stacked || chart.series.length < 2) {
            return all;
        }
        const some = all.filter(index => (chart.series[index].points ?? []).some(point => point.value !== 0));
        return some.length > 0 ? some : all;
    }

    // Where `value` sits across a row of `width`, between the axis' lowest
    // and highest value. Drawing values only place marks (AGENTS.md §5).
    function across(value: real, width: real): real {
        const span = high - low;
        return span > 0 ? Math.max(0, Math.min(1, (value - low) / span)) * width : 0;
    }

    // The pieces of the bars of row `point` across `width`: one per series,
    // each `{series, x, width}`. Plain bars start at zero; stacked bars pile
    // up, those above zero to the right and those below to the left, as the
    // axis counts them.
    function rowPieces(point: int, width: real): var {
        const pieces = [];
        let up = 0;
        let down = 0;
        for (let series = 0; series < chart.series.length; ++series) {
            const value = chart.series[series].points[point]?.value ?? 0;
            let from = 0;
            let to = value;
            if (stacked) {
                if (value >= 0) {
                    from = up;
                    up += value;
                    to = up;
                } else {
                    from = down;
                    down += value;
                    to = down;
                }
            }
            const one = across(from, width);
            const two = across(to, width);
            pieces.push({series: series, x: Math.min(one, two), width: Math.abs(two - one)});
        }
        return pieces;
    }

    // The longest single word of the names: a word is never broken.
    TextMetrics {
        id: longestWord

        font: Kirigami.Theme.smallFont
        text: {
            let found = "";
            for (let index = 0; index < view.count; ++index) {
                for (const word of view.categoryName(index).split(/\s+/)) {
                    if (word.length > found.length) {
                        found = word;
                    }
                }
            }
            return found;
        }
    }

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
        case "balance":
            return i18nContext.i18nc("@title:column a table of a chart", "Balance");
        // The four balances of a candle (US4.10).
        case "start":
            return i18nContext.i18nc("@label a balance of a candle: at the start of its days", "Start");
        case "highest":
            return i18nContext.i18nc("@label a balance of a candle: the highest at the end of one of its days", "Highest");
        case "lowest":
            return i18nContext.i18nc("@label a balance of a candle: the lowest at the end of one of its days", "Lowest");
        case "end":
            return i18nContext.i18nc("@label a balance of a candle: at the end of its days", "End");
        // The smaller categories of one bar split into its categories, put
        // together as in a pie (D-666).
        case "other":
            return i18nContext.i18nc("@label the smaller slices of a pie put together", "Other");
        // The second bar of a spending chart with a comparison (US3.1a).
        case "previous_period":
            return i18nContext.i18nc("@label a series of a chart: spending in the period before", "Period before");
        case "same_last_year":
            return i18nContext.i18nc("@label a series of a chart: spending in the same days a year earlier", "Same period last year");
        case "average_3":
            return i18nContext.i18nc("@label a series of a chart: the average spending of earlier periods", "Average of the 3 periods before");
        case "average_6":
            return i18nContext.i18nc("@label a series of a chart: the average spending of earlier periods", "Average of the 6 periods before");
        case "average_12":
            return i18nContext.i18nc("@label a series of a chart: the average spending of earlier periods", "Average of the 12 periods before");
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
        // What the overlays of a custom chart have at this place.
        for (const overlay of overlays) {
            for (const series of overlay.series) {
                for (const item of series.items) {
                    if (item.category === index) {
                        parts.push(i18nContext.i18nc("@label %1 is a series of a custom chart, %2 its value or text there", "%1: %2", series.name, item.text.length > 0 ? item.text : item.label));
                    }
                }
            }
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
                    sourceComponent: view.round ? pie : (view.candles ? candle : (view.lines ? line : bar))
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

                // The layers of a custom chart over the picture (M6e.2).
                ChartOverlays {
                    anchors.fill: parent
                    visible: view.overlays.length > 0
                    chart: view.chart
                    low: view.low
                    high: view.high
                    count: view.count
                    lineAligned: view.lines
                }

                // A bar of zero, where other series have bars: a thin line of
                // its series' colour on the zero line, so the empty slot
                // reads as "nothing" and not as a gap (M6-B229).
                Repeater {
                    model: !view.round && !view.lines && !view.candles && !view.stacked && view.chart.series.length > 1
                        ? view.chart.series.length * view.count : 0

                    delegate: Rectangle {
                        required property int index
                        readonly property int seriesIndex: Math.floor(index / view.count)
                        readonly property int pointIndex: index % view.count
                        readonly property var series: view.chart.series[seriesIndex]
                        readonly property real value: series?.points[pointIndex]?.value ?? 1
                        readonly property real span: view.high - view.low

                        visible: value === 0 && span > 0 && view.barSeries.indexOf(seriesIndex) >= 0
                        width: view.barWidth(plot.width)
                        height: 2
                        x: view.barMiddle(seriesIndex, pointIndex, plot.width) - width / 2
                        y: span > 0 ? (1 - (0 - view.low) / span) * plot.height - height : 0
                        color: series ? ChartPalette.color(series.color, view.extraColors) : "transparent"
                    }
                }

                // The second mark of each bar's series (D-283), where there
                // is more than one series to tell apart: white in a tall bar,
                // in the series' colour just above a short one (M6-B229).
                Repeater {
                    model: !view.round && !view.lines && !view.candles && view.chart.series.length > 1
                        ? view.chart.series.length * view.count : 0

                    delegate: Controls.Label {
                        required property int index
                        readonly property int seriesIndex: Math.floor(index / view.count)
                        readonly property int pointIndex: index % view.count
                        readonly property var place: view.barPlace(seriesIndex, pointIndex, plot.width, plot.height, height)
                        readonly property int marker: view.chart.series[seriesIndex]?.marker ?? -1

                        visible: place !== null && marker >= 0
                        x: place ? place.x - width / 2 : 0
                        y: place ? place.y : 0
                        text: ["■", "●", "▲", "◆"][marker] ?? ""
                        textFormat: Text.PlainText
                        color: place && place.inside
                            ? "white"
                            : ChartPalette.color(view.chart.series[seriesIndex]?.color ?? 0, view.extraColors)
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

                // Pointing at a candle says its four balances, as the keys
                // and the table do.
                HoverHandler {
                    enabled: view.candles && view.count > 0
                    onPointChanged: view.current = Math.max(0, Math.min(view.count - 1, Math.floor(point.position.x / (plot.width / view.count))))
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

    // Bars as rows (M6-B3, D-679): one row per category or month, the name
    // whole on the left. Plain bars of several series lie one under another
    // in their row, each with its value; stacked bars are one bar of pieces,
    // with the value axis under the rows. A series' second mark sits in its
    // piece where there is room (D-283).
    ColumnLayout {
        id: rows

        Layout.fillWidth: true
        visible: !view.showTable && view.horizontal
        spacing: Kirigami.Units.smallSpacing

        readonly property real nameWidth: view.width * 0.4

        Repeater {
            model: view.horizontal ? view.count : 0

            delegate: RowLayout {
                id: barRow

                required property int index

                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    // One fixed width, so the wrapped name settles its height at
                    // once and the row does not rearrange again (M6-B17).
                    Layout.preferredWidth: rows.nameWidth
                    Layout.minimumWidth: rows.nameWidth
                    Layout.maximumWidth: rows.nameWidth
                    Layout.alignment: Qt.AlignVCenter
                    text: view.categoryName(barRow.index)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    font: Kirigami.Theme.smallFont
                    Accessible.ignored: true
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 1

                    Repeater {
                        // Stacked: one line holds every piece.
                        model: view.stacked ? 1 : view.chart.series.length

                        delegate: RowLayout {
                            id: barLine

                            required property int index

                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing

                            Item {
                                id: track

                                Layout.fillWidth: true
                                Layout.preferredHeight: view.chart.series.length > 1 && !view.stacked
                                    ? Kirigami.Units.gridUnit : Kirigami.Units.gridUnit * 1.5

                                readonly property var pieces: view.rowPieces(barRow.index, width)
                                    .filter(piece => view.stacked || piece.series === barLine.index)

                                Repeater {
                                    model: track.pieces

                                    delegate: Rectangle {
                                        id: piece

                                        required property var modelData
                                        readonly property var series: view.chart.series[modelData.series]

                                        x: modelData.x
                                        width: modelData.width
                                        height: track.height
                                        color: ChartPalette.color(piece.series.color, view.extraColors)
                                        border.width: view.current === barRow.index ? 2 : 0
                                        border.color: Kirigami.Theme.highlightColor

                                        // In the piece where there is room, else just
                                        // after a plain bar in its colour (M6-B229).
                                        Controls.Label {
                                            readonly property int marker: view.chart.series.length > 1 ? (piece.series.marker ?? -1) : -1
                                            readonly property bool inside: piece.width > width * 1.4

                                            x: inside ? (piece.width - width) / 2 : piece.width + 2
                                            anchors.verticalCenter: parent.verticalCenter
                                            visible: marker >= 0 && (inside || !view.stacked)
                                            text: ["■", "●", "▲", "◆"][marker] ?? ""
                                            textFormat: Text.PlainText
                                            font: Kirigami.Theme.smallFont
                                            color: inside ? "white" : piece.color
                                            Accessible.ignored: true
                                        }
                                    }
                                }
                            }

                            Controls.Label {
                                visible: !view.stacked
                                text: view.stacked ? "" : (view.chart.series[barLine.index]?.points[barRow.index]?.label ?? "")
                                textFormat: Text.PlainText
                                font: Kirigami.Theme.smallFont
                                Accessible.ignored: true
                            }
                        }
                    }
                }
            }
        }

        // The value axis under stacked rows, whose pieces carry no values:
        // the tick labels, as text from Rust, across the bars' width.
        Item {
            visible: view.stacked && view.ticks.length > 0
            Layout.fillWidth: true
            Layout.leftMargin: rows.nameWidth + Kirigami.Units.smallSpacing
            Layout.preferredHeight: visible ? tickFont.height : 0

            FontMetrics {
                id: tickFont

                font: Kirigami.Theme.smallFont
            }

            Repeater {
                model: view.stacked ? view.ticks : []

                delegate: Controls.Label {
                    required property var modelData
                    required property int index

                    readonly property bool first: index === 0
                    readonly property bool last: index === view.ticks.length - 1
                    x: Math.max(0, Math.min(parent.width - width, view.across(modelData.value, parent.width) - (first ? 0 : (last ? width : width / 2))))
                    text: modelData.label
                    textFormat: Text.PlainText
                    font: Kirigami.Theme.smallFont
                    color: Kirigami.Theme.textColor
                    Accessible.ignored: true
                }
            }
        }
    }

    // The text of the point in focus, also for those who see it. Not while
    // the pointer rests on the legend: the tooltip says the same there, and
    // this line, above the legend, would push the entry away from the pointer,
    // lose the hover and flicker (M6-B163, like M6-B18).
    Controls.Label {
        Layout.fillWidth: true
        visible: !view.showTable && view.current >= 0 && view.legendTip < 0
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
            model: view.round ? view.count : view.legendEntries.length

            delegate: RowLayout {
                id: entry

                required property int index
                readonly property var series: view.round ? null : view.legendEntries[index]
                readonly property int color: view.round
                    ? view.chart.series[0].points[index].color : series.color
                readonly property int marker: view.round
                    ? view.chart.series[0].points[index].marker : series.marker
                readonly property bool hollow: !view.round && series.hollow === true

                spacing: Kirigami.Units.smallSpacing

                Rectangle {
                    implicitWidth: Kirigami.Units.gridUnit
                    implicitHeight: Kirigami.Units.gridUnit
                    radius: entry.marker === 1 ? width / 2 : (entry.marker === 2 ? 0 : 2)
                    rotation: entry.marker === 3 ? 45 : 0
                    // A row that can't be a slice has no color of its own.
                    color: view.undrawn(entry.index) || entry.hollow ? "transparent" : ChartPalette.color(entry.color, view.extraColors)
                    border.width: entry.hollow ? 2 : (view.undrawn(entry.index) ? 1 : 0)
                    border.color: entry.hollow ? ChartPalette.color(entry.color, view.extraColors) : Kirigami.Theme.disabledTextColor

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
                    text: view.round ? view.legendText(entry.index) : entry.series.name
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
        // A custom chart has no fixed meaning for a total and gives none.
        visible: !view.showTable && view.chart.total !== ""
        text: view.chart.per === "per-month"
            ? i18nContext.i18nc("@label %1 is an amount of money", "Spent per month on average: %1", view.chart.total)
            : view.chart.per === "per-quarter"
            ? i18nContext.i18nc("@label %1 is an amount of money", "Spent per quarter on average: %1", view.chart.total)
            : view.chart.per === "per-year"
            ? i18nContext.i18nc("@label %1 is an amount of money", "Spent per year on average: %1", view.chart.total)
            : view.chart.kind === "pie" || view.chart.kind === "donut" || view.chart.dataset === "spending_by_category"
            ? i18nContext.i18nc("@label %1 is an amount of money", "Total spent: %1", view.chart.total)
            : view.chart.dataset === "net_worth_over_time"
            ? i18nContext.i18nc("@label %1 is an amount of money", "What you are worth at the end: %1", view.chart.total)
            : view.chart.dataset === "account_balances"
            ? i18nContext.i18nc("@label %1 is an amount of money: what the user has less what they owe at the end of the period", "What you are worth: %1", view.chart.total)
            : view.candles
            ? i18nContext.i18nc("@label %1 is an amount of money: what the accounts held at the end of the period", "Balance at the end: %1", view.chart.total)
            : view.chart.dataset === "cash_flow_by_month"
            ? i18nContext.i18nc("@label %1 is an amount of money, the money in less the money out of the accounts", "Net change in total: %1", view.chart.total)
            : i18nContext.i18nc("@label %1 is an amount of money", "Left over in total: %1", view.chart.total)
        textFormat: Text.PlainText
        font.bold: true
    }

    // What a candle shows, in plain words ("Explain, don't assume").
    Controls.Label {
        Layout.fillWidth: true
        visible: view.candles && !view.showTable
        text: i18nContext.i18nc("@info under a candle chart", "Each candle is one stretch of days, named below it. Its box runs from the balance at the start to the balance at the end; the thin line runs from the lowest to the highest balance at the end of a day. Several bookings on one day count only with that day's end.")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        font: Kirigami.Theme.smallFont
    }

    // What "Other" holds, under the chart and under its table.
    Controls.Label {
        Layout.fillWidth: true
        // A pie, or one bar split into the categories (D-666).
        visible: view.chart.slices.other >= 0
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
        text: Plain.plain(view.showTable
            ? i18nContext.i18nc("@action:button", "Show as chart")
            : i18nContext.i18nc("@action:button", "Show as table"))
        icon.name: view.showTable ? "office-chart-bar" : "view-list-details"
        onClicked: {
            ActionLog.taken("ChartView_showTable")
            view.showTable = !view.showTable
        }
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
            fillOpacity: view.chart.kind === "area" || view.chart.kind === "stacked-area" ? 0.35 : 0
            interpolate: false
            stacked: view.chart.kind === "stacked-area"
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
                array: view.chart.series.map(series => ChartPalette.color(series.color, view.extraColors))
            }
            valueSources: view.sources(lineChart, null)
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
            // Never wider than a few lines of text: a chart of one category
            // would otherwise fill half the picture with each bar (M6-B229).
            barWidth: view.barWidth(width)
            indexingMode: Charts.Chart.IndexEachSource
            colorSource: Charts.ArraySource {
                array: view.barSeries.map(index => ChartPalette.color(view.chart.series[index]?.color ?? 0, view.extraColors))
            }
            valueSources: view.sources(barChart, view.barSeries)
        }
    }

    // Candles (US4.10): for each a thin line from the lowest to the highest
    // balance and a box from the start to the end, by default hollow when
    // it ended higher and filled when it ended lower, so up and down differ
    // by more than the color; `candleFill` can change that (D-748). The
    // drawing values only place them (AGENTS.md §5).
    Component {
        id: candle

        Item {
            id: candlePlot

            readonly property real span: view.high - view.low
            readonly property real slot: width / Math.max(1, view.count)

            function yOf(value: real): real {
                return span > 0 ? (1 - (value - view.low) / span) * height : height / 2;
            }

            Repeater {
                model: view.chart.series.length === 4 ? view.count : 0

                delegate: Item {
                    required property int index
                    readonly property real open: view.chart.series[0].points[index].value
                    readonly property real highest: view.chart.series[1].points[index].value
                    readonly property real lowest: view.chart.series[2].points[index].value
                    readonly property real close: view.chart.series[3].points[index].value
                    readonly property bool rose: close >= open
                    readonly property color tint: ChartPalette.color(rose ? 0 : 1, view.extraColors)
                    readonly property real bodyWidth: Math.max(3, Math.min(Kirigami.Units.gridUnit * 2, candlePlot.slot * 0.6))

                    x: index * candlePlot.slot
                    width: candlePlot.slot
                    height: candlePlot.height

                    // The line from the lowest to the highest balance.
                    Rectangle {
                        x: (parent.width - width) / 2
                        width: 2
                        y: candlePlot.yOf(parent.highest)
                        height: Math.max(1, candlePlot.yOf(parent.lowest) - y)
                        color: parent.tint
                    }

                    // The box from the start to the end; at least two
                    // pixels high, so a candle without change still shows.
                    Rectangle {
                        x: (parent.width - width) / 2
                        width: parent.bodyWidth
                        y: candlePlot.yOf(Math.max(parent.open, parent.close)) - (height - Math.abs(candlePlot.yOf(parent.open) - candlePlot.yOf(parent.close))) / 2
                        height: Math.max(2, Math.abs(candlePlot.yOf(parent.open) - candlePlot.yOf(parent.close)))
                        color: view.candleHollow(parent.rose) ? Kirigami.Theme.backgroundColor : parent.tint
                        border.width: 2
                        border.color: parent.tint
                    }
                }
            }
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
                array: view.chart.series[0].points.map(point => ChartPalette.color(point.color, view.extraColors))
            }
            valueSources: view.sources(pieChart, null)
        }
    }

    // The sources of values of a chart, one per series, from the drawing
    // values: they only place the marks (AGENTS.md §5).
    // The width of one bar in a picture `width` wide: an equal share of the
    // category's slot, at most three grid units. The bar chart centres each
    // category's bars in its slot (KQuickCharts, a set `barWidth`).
    function barWidth(width: real): real {
        const group = stacked ? 1 : Math.max(1, barSeries.length);
        const share = width / Math.max(1, count) / group - Kirigami.Units.smallSpacing;
        return Math.max(1, Math.min(Kirigami.Units.gridUnit * 3, share));
    }

    // The middle of bar (`series`, `point`) across a picture `width` wide.
    function barMiddle(series: int, point: int, width: real): real {
        const slot = width / Math.max(1, count);
        if (stacked) {
            return (point + 0.5) * slot;
        }
        // A series left out of the drawing (`barSeries`) has no slot of its
        // own; its place is the middle of the category's.
        const place = barSeries.indexOf(series);
        if (place < 0) {
            return (point + 0.5) * slot;
        }
        const group = Math.max(1, barSeries.length);
        const one = barWidth(width);
        const gap = Kirigami.Units.smallSpacing;
        const all = one * group + gap * (group - 1);
        return point * slot + slot / 2 - all / 2 + place * (one + gap) + one / 2;
    }

    // Where the mark of bar (`series`, `point`) goes in a picture of `width` by
    // `height`: near the top of the bar (`inside`), or just above a bar too
    // short for it; null for a bar below zero or a piece of a stacked bar
    // too short for it. `size` is the mark's height.
    function barPlace(series: int, point: int, width: real, height: real, size: real): var {
        // While the charts switch, a mark can ask for a series that is gone.
        const points = chart.series[series]?.points;
        if (!points || points[point] === undefined || !stacked && barSeries.indexOf(series) < 0) {
            return null;
        }
        let from = 0;
        let to = points[point].value;
        if (stacked) {
            for (let before = 0; before < series; ++before) {
                const value = chart.series[before].points[point]?.value ?? 0;
                if (value > 0) {
                    from += value;
                }
            }
            to += from;
        }
        const span = high - low;
        const top = Math.max(0, (1 - (to - low) / span) * height);
        const bottom = (1 - (from - low) / span) * height;
        if (to < from || stacked && (to === from || bottom - top < size * 1.2)) {
            return null;
        }
        const inside = bottom - top >= size * 1.2;
        return {
            x: barMiddle(series, point, width),
            y: inside ? top + size * 0.2 : Math.max(0, top - size - 2),
            inside: inside
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

    // `only`, if given, are the indexes of the series to draw.
    function sources(parentItem: var, only: var): var {
        return chart.series.filter((series, index) => !only || only.indexOf(index) >= 0).map(series => sourceComponent.createObject(parentItem, {
            array: series.points.map(point => point.value)
        }));
    }

    Component {
        id: sourceComponent

        Charts.ArraySource {}
    }
}
