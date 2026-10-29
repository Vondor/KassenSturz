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

    readonly property bool round: chart.round
    readonly property int count: chart.count
    readonly property var ticks: chart.ticks
    readonly property bool lines: chart.kind === "line" || chart.kind === "area"
    readonly property real low: ticks.length > 0 ? ticks[0].value : 0
    readonly property real high: ticks.length > 0 ? ticks[ticks.length - 1].value : 1

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function seriesName(key: string): string {
        switch (key) {
        case "spending":
            return i18nContext.i18nc("@label a series of a chart", "Spending");
        case "income":
            return i18nContext.i18nc("@label a series of a chart", "Money in");
        case "expenses":
            return i18nContext.i18nc("@label a series of a chart", "Money out");
        }
        return "";
    }

    function categoryName(index: int): string {
        return chart.unsortedIndex === index
            ? i18nContext.i18nc("@label money with no category yet", "Not sorted yet")
            : chart.categories[index];
    }

    // What the point in focus says, for the screen reader and for the eyes.
    function pointText(index: int): string {
        if (index < 0 || index >= count) {
            return "";
        }
        const parts = [];
        for (const series of chart.series) {
            parts.push(chart.series.length > 1
                ? i18nContext.i18nc("@label %1 is a series like Money in, %2 an amount", "%1: %2", seriesName(series.key), series.points[index].label)
                : series.points[index].label);
        }
        return i18nContext.i18nc("@label %1 is a category or month, %2 the values there", "%1: %2", categoryName(index), parts.join(", "));
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
        visible: !view.showTable
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
            // a long name is cut short and shown whole in the point's text.
            Item {
                visible: !view.round
                Layout.fillWidth: true
                Layout.preferredHeight: categoryHeight.rowHeight

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

    // The text of the point in focus, also for those who see it.
    Controls.Label {
        Layout.fillWidth: true
        visible: !view.showTable && view.current >= 0
        text: view.pointText(view.current)
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        Accessible.ignored: true
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
                    color: ChartPalette.color(entry.color)

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
                    text: view.round ? view.categoryName(entry.index) : view.seriesName(entry.series.key)
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    Layout.maximumWidth: Kirigami.Units.gridUnit * 14
                }
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: !view.showTable
        text: view.chart.kind === "pie" || view.chart.kind === "donut" || view.chart.dataset === "spending_by_category"
            ? i18nContext.i18nc("@label %1 is an amount of money", "Total spent: %1", view.chart.total)
            : i18nContext.i18nc("@label %1 is an amount of money", "Left over in total: %1", view.chart.total)
        textFormat: Text.PlainText
        font.bold: true
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
