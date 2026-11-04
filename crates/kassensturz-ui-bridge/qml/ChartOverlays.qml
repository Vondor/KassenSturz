// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The layers of a custom chart that lie on top of its own marks (M6e.2,
// D-503): lines, areas and bars after the first, dots, texts and rules. They
// share the chart's axes: `ChartView` puts this over its picture, with the
// same value range and the same places for the categories. Every position
// comes from the drawing values of the chart model (they only place marks,
// AGENTS.md §5) and every word is text from Rust. The marks are decoration for
// the eyes: the same values are in the chart's table, the point text and the
// legend, so nothing is only in a picture or a color.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Shapes
import org.kde.kirigami as Kirigami

Item {
    id: overlays

    // The chart object from `BookCharts.view.charts`.
    required property var chart
    // The value range of the axis, as in `ChartView`.
    required property real low
    required property real high
    // The number of categories, and whether they sit on the marks of a line
    // or area (the first at the left edge, the last at the right) or in the
    // middle of a slot as bars do.
    required property int count
    required property bool lineAligned

    readonly property var layers: chart.overlays ?? []
    readonly property real span: Math.max(1, high - low)

    function xOf(category: int): real {
        return lineAligned
            ? (count > 1 ? category * width / (count - 1) : 0)
            : (category + 0.5) * width / Math.max(1, count);
    }

    function yOf(value: real): real {
        return (1 - (value - low) / span) * height;
    }

    // The items of one series that sit at a category, for polylines.
    function pointsOf(series: var): var {
        const found = [];
        for (const item of series.items) {
            if (item.category >= 0 && item.hasValue) {
                found.push(Qt.point(xOf(item.category), yOf(item.value)));
            }
        }
        return found;
    }

    function areaOf(series: var): var {
        const found = pointsOf(series);
        if (found.length === 0) {
            return found;
        }
        const floor = yOf(Math.max(low, Math.min(high, 0)));
        return [Qt.point(found[0].x, floor), ...found, Qt.point(found[found.length - 1].x, floor)];
    }

    // Every series of every layer with the mark of its layer, in the order
    // they are drawn (a flat list: a nested repeater's delegates can't read
    // the ids of the one around them).
    readonly property var drawn: {
        const found = [];
        for (const layer of layers) {
            for (const series of layer.series) {
                found.push({mark: layer.mark, color: series.color, items: series.items});
            }
        }
        return found;
    }

    Repeater {
        model: overlays.drawn

        delegate: Item {
            id: series

            required property var modelData
            readonly property color color: ChartPalette.color(modelData?.color ?? 0, overlays.chart?.extraColors)
            readonly property string mark: modelData?.mark ?? ""

            anchors.fill: parent

            // A line, or the area under it.
            Shape {
                anchors.fill: parent
                visible: series.mark === "line" || series.mark === "area"
                preferredRendererType: Shape.CurveRenderer

                ShapePath {
                    strokeColor: series.mark === "line" ? series.color : "transparent"
                    strokeWidth: 3
                    fillColor: series.mark === "area"
                        ? Qt.rgba(series.color.r, series.color.g, series.color.b, 0.35) : "transparent"

                    PathPolyline {
                        path: series.mark === "area" ? overlays.areaOf(series.modelData) : overlays.pointsOf(series.modelData)
                    }
                }
            }

            // Bars from zero.
            Repeater {
                model: series.mark === "bar" ? (series.modelData?.items ?? []) : []

                delegate: Rectangle {
                    required property var modelData
                    readonly property real barTop: overlays.yOf(Math.max(0, modelData.value))
                    readonly property real barBottom: overlays.yOf(Math.min(0, modelData.value))

                    visible: modelData.category >= 0 && modelData.hasValue
                    width: Math.max(2, overlays.width / Math.max(1, overlays.count) * 0.4)
                    x: overlays.xOf(Math.max(0, modelData.category)) - width / 2
                    y: barTop
                    height: Math.max(1, barBottom - barTop)
                    color: series.color
                    Accessible.ignored: true
                }
            }

            // Dots; the bigger the size, the bigger the dot.
            Repeater {
                model: series.mark === "point" ? (series.modelData?.items ?? []) : []

                delegate: Rectangle {
                    required property var modelData
                    readonly property real diameter: Kirigami.Units.gridUnit * (0.3 + 0.15 * modelData.size)

                    visible: modelData.category >= 0 && modelData.hasValue
                    width: diameter
                    height: diameter
                    radius: diameter / 2
                    x: overlays.xOf(Math.max(0, modelData.category)) - diameter / 2
                    y: overlays.yOf(modelData.value) - diameter / 2
                    color: series.color
                    border.width: 1
                    border.color: Kirigami.Theme.backgroundColor
                    Accessible.ignored: true
                }
            }

            // Texts: the user's data, always plain text.
            Repeater {
                model: series.mark === "text" ? (series.modelData?.items ?? []) : []

                delegate: Controls.Label {
                    required property var modelData

                    visible: modelData.category >= 0 && modelData.hasValue && modelData.text.length > 0
                    x: Math.max(0, Math.min(overlays.width - width, overlays.xOf(Math.max(0, modelData.category)) - width / 2))
                    y: Math.max(0, overlays.yOf(modelData.value) - height)
                    text: modelData.text
                    textFormat: Text.PlainText
                    font: Kirigami.Theme.smallFont
                    color: series.color
                    Accessible.ignored: true
                }
            }

            // A level across the chart.
            Repeater {
                model: series.mark === "rule-level" ? (series.modelData?.items ?? []) : []

                delegate: Rectangle {
                    required property var modelData

                    visible: modelData.hasValue
                    width: overlays.width
                    height: 2
                    y: Math.round(overlays.yOf(modelData.value)) - 1
                    color: series.color
                    Accessible.ignored: true
                }
            }

            // A line at one category, from bottom to top.
            Repeater {
                model: series.mark === "rule-category" ? (series.modelData?.items ?? []) : []

                delegate: Rectangle {
                    required property var modelData

                    visible: modelData.category >= 0
                    width: 2
                    height: overlays.height
                    x: Math.round(overlays.xOf(Math.max(0, modelData.category))) - 1
                    color: series.color
                    Accessible.ignored: true
                }
            }
        }
    }
}
