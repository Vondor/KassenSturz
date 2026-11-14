// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The money-flow chart (US7, #821, D-758, docs/MONEY_FLOW.md): money comes in
// on the left, passes one middle node and goes out on the right; the
// thickness of a band is its amount. Drawn with QtQuick Shapes: rectangles
// for nodes, smooth bands for flows.
//
// Rust did the work: the nodes and bands arrive placed in whole millionths of
// the chart's height (`chart.whole`), and every amount and name as text
// (D-065). This file only scales them. Nothing is only in a picture or a
// color: every flow is in the table ("Show as table"), the point in focus is
// said in words under the chart, and the keyboard moves between the flows.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Shapes
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: view

    // The chart object from `BookCharts.view.charts` with `kind` "flow".
    required property var chart
    property string title: ""
    property bool showTable: false
    // The band in focus or under the pointer, an index of `bands`, or -1.
    property int current: -1
    // A click on a band or a node: the transactions behind it open. The
    // band's two nodes by index, or a node alone (`to` is -1).
    signal flowActivated(int from, int to)
    // What the transactions behind a click are narrowed by: the period and
    // the scope text of the component (its accounts, tags and the filter
    // bar's), as `BookTransactions` takes them.
    property string period: ""
    property string scope: ""
    // What the open list is about: a category id ("" for everything that
    // flows through the chart) and the words above it.
    property string behindCategory: ""
    property string behindTitle: ""

    readonly property var nodes: chart.nodes
    readonly property var bands: chart.bands
    readonly property real whole: Math.max(1, chart.whole)
    readonly property real nodeWidth: Math.max(8, Kirigami.Units.smallSpacing * 2)
    readonly property int columns: Math.max(1, chart.columns)
    // The middle node is the first.
    readonly property int middleColumn: nodes.length > 0 ? nodes[0].column : 0
    // Room at both ends for the names of the outermost nodes.
    readonly property real labelWidth: Math.min(Kirigami.Units.gridUnit * 9, width * 0.22)

    readonly property KI18nContext i18nContext: KI18nContext {}

    spacing: Kirigami.Units.smallSpacing

    // The name of a node, in the user's words for the kinds that have none
    // of their own (D-034).
    function nodeName(node: var): string {
        switch (node.kind) {
        case "middle":
            return i18nContext.i18nc("@label the one node in the middle of the money-flow chart", "Your money");
        case "left-over":
            return i18nContext.i18nc("@label a node of the money-flow chart: what is left when you earned more than you spent", "Left over");
        case "reserves":
            return i18nContext.i18nc("@label a node of the money-flow chart: money taken from what you had saved because you spent more than you earned", "From your reserves");
        case "unsorted":
            return i18nContext.i18nc("@label a node of the money-flow chart: money with no category yet", "Not sorted yet");
        case "no-payee":
            return i18nContext.i18nc("@label a node of the money-flow chart: money with no name of who paid or was paid", "No name given");
        case "other":
            return i18nContext.i18nc("@label a node of the money-flow chart that holds the smallest ones; %1 is how many", "Other (%1)", node.holds.length);
        case "other-accounts":
            return i18nContext.i18nc("@label a node of the money-flow chart: the accounts you did not choose, taken together", "Your other accounts");
        case "category-itself":
            return i18nContext.i18nc("@label a node of the money-flow chart: what was booked to a category itself and not to one of its sub-categories; %1 is its name", "%1 itself", node.name);
        default:
            return node.name;
        }
    }

    function bandText(band: var): string {
        return i18nContext.i18nc("@info a flow of the money-flow chart: %1 where it comes from, %2 where it goes, %3 the amount", "%1 to %2: %3",
            nodeName(nodes[band.from]), nodeName(nodes[band.to]), band.label);
    }

    // The scope text of the transactions behind a click: the component's own,
    // narrowed to one category when the click was on one (text only; the
    // amounts and the list come from Rust).
    function behindScope(categoryId: string): string {
        let scope = {};
        try {
            scope = JSON.parse(view.scope);
        } catch (error) {
            scope = {};
        }
        if (categoryId !== "") {
            scope.categories = [categoryId];
            scope.categoriesOwn = true;
            scope.takes = Object.assign({}, scope.takes ?? {}, { categories: true });
        }
        return JSON.stringify(scope);
    }

    // The category a click is about: of a band's two ends the outer one that
    // has a category; "" when it touches none (your money, left over), which
    // means everything that flows through the chart.
    function categoryBehind(from: int, to: int): string {
        const ends = to >= 0 ? [nodes[from], nodes[to]] : [nodes[from]];
        let outer = null;
        for (const node of ends) {
            if ((node.category ?? "") !== "" && (outer === null || Math.abs(node.column) > Math.abs(outer.column))) {
                outer = node;
            }
        }
        return outer?.category ?? "";
    }

    // Opens the transactions behind a band (`to` >= 0) or a node.
    function openBehind(from: int, to: int) {
        behindCategory = categoryBehind(from, to);
        behindTitle = to >= 0 ? bandText(bands.find(b => b.from === from && b.to === to)) : nodeName(nodes[from]);
        behind.open();
    }

    onFlowActivated: (from, to) => openBehind(from, to)

    // The places of the nodes of one column, in order, give each its color.
    readonly property var nodeColors: {
        const result = new Array(nodes.length);
        const byColumn = {};
        for (let i = 0; i < nodes.length; i++) {
            (byColumn[nodes[i].column] = byColumn[nodes[i].column] ?? []).push(i);
        }
        for (const key in byColumn) {
            const members = byColumn[key].sort((a, b) => nodes[a].y - nodes[b].y);
            members.forEach((index, place) => {
                const kind = nodes[index].kind;
                result[index] = kind === "middle" ? Kirigami.Theme.textColor
                    : (kind === "left-over" || kind === "reserves" || kind === "other" || kind === "other-accounts") ? Kirigami.Theme.disabledTextColor
                    : ChartPalette.color(place, undefined);
            });
        }
        return result;
    }

    // A band takes the color of its node that is farther from the middle.
    function bandColor(band: var): color {
        const a = nodes[band.from];
        const b = nodes[band.to];
        const outer = Math.abs(a.column - middleColumn) >= Math.abs(b.column - middleColumn) ? band.from : band.to;
        return nodeColors[outer];
    }

    // The outline of a band: the top edge from its start to its end, then
    // back along the bottom edge. The edge is a smooth S, `3u² - 2u³`.
    function bandPoints(band: var, plotWidth: real, plotHeight: real): var {
        const steps = 24;
        const x0 = xOf(nodes[band.from].column, plotWidth) + (nodes[band.from].column < nodes[band.to].column ? nodeWidth : 0);
        const x1 = xOf(nodes[band.to].column, plotWidth) + (nodes[band.from].column < nodes[band.to].column ? 0 : nodeWidth);
        const thickness = Math.max(1, band.thickness * plotHeight / whole);
        const yf = band.yFrom * plotHeight / whole;
        const yt = band.yTo * plotHeight / whole;
        const top = [];
        const bottom = [];
        for (let i = 0; i <= steps; i++) {
            const u = i / steps;
            const s = 3 * u * u - 2 * u * u * u;
            const x = x0 + (x1 - x0) * u;
            const y = yf + (yt - yf) * s;
            top.push(Qt.point(x, y));
            bottom.unshift(Qt.point(x, y + thickness));
        }
        return top.concat(bottom);
    }

    function xOf(column: int, plotWidth: real): real {
        const index = columnIndex(column);
        return columns > 1 ? index * (plotWidth - nodeWidth) / (columns - 1) : 0;
    }

    // `column` of a node is already the place from the left, 0 up.
    function columnIndex(column: int): int {
        return column;
    }

    // The band under a point of the plot, or -1.
    function bandAt(x: real, y: real, plotWidth: real, plotHeight: real): int {
        for (let i = bands.length - 1; i >= 0; i--) {
            const band = bands[i];
            const forward = nodes[band.from].column < nodes[band.to].column;
            const x0 = xOf(nodes[band.from].column, plotWidth) + (forward ? nodeWidth : 0);
            const x1 = xOf(nodes[band.to].column, plotWidth) + (forward ? 0 : nodeWidth);
            const left = Math.min(x0, x1);
            const right = Math.max(x0, x1);
            if (x < left || x > right || right === left) {
                continue;
            }
            const u = Math.min(1, Math.max(0, (x - x0) / (x1 - x0)));
            const s = 3 * u * u - 2 * u * u * u;
            const top = band.yFrom * plotHeight / whole + (band.yTo - band.yFrom) * plotHeight / whole * s;
            const thickness = Math.max(1, band.thickness * plotHeight / whole);
            if (y >= top && y <= top + thickness) {
                return i;
            }
        }
        return -1;
    }

    function nodeAt(x: real, y: real, plotWidth: real, plotHeight: real): int {
        for (let i = 0; i < nodes.length; i++) {
            const left = xOf(nodes[i].column, plotWidth);
            const top = nodes[i].y * plotHeight / whole;
            const height = Math.max(1, nodes[i].height * plotHeight / whole);
            if (x >= left && x <= left + nodeWidth && y >= top && y <= top + height) {
                return i;
            }
        }
        return -1;
    }

    function summary(): string {
        return i18nContext.i18nc("@info the money-flow chart in words; %1 is how much passed through, %2 the currency", "Where your money came from and where it went. %1 passed through. Use Left and Right to hear each flow, or show the chart as a table.", chart.total);
    }

    Accessible.role: Accessible.Chart
    Accessible.name: current >= 0 && current < bands.length ? bandText(bands[current]) : title
    Accessible.description: summary()
    focus: false
    activeFocusOnTab: true
    Keys.onLeftPressed: current = Math.max(0, current - 1)
    Keys.onRightPressed: current = Math.min(bands.length - 1, current + 1)
    Keys.onReturnPressed: {
        if (current >= 0 && current < bands.length) {
            flowActivated(bands[current].from, bands[current].to);
        }
    }
    Keys.onEscapePressed: current = -1

    Kirigami.Heading {
        Layout.fillWidth: true
        level: 3
        text: view.title
        visible: text.length > 0 && !view.showTable
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
    }

    // The picture.
    Item {
        id: picture

        Layout.fillWidth: true
        Layout.preferredHeight: Math.max(Kirigami.Units.gridUnit * 14, Math.min(Kirigami.Units.gridUnit * 26, width * 0.55))
        visible: !view.showTable

        readonly property real plotX: view.labelWidth
        readonly property real plotWidth: Math.max(1, width - 2 * view.labelWidth)

        Item {
            id: plot

            x: picture.plotX
            width: picture.plotWidth
            height: picture.height

            Repeater {
                model: view.bands

                Shape {
                    id: band

                    required property var modelData
                    required property int index

                    anchors.fill: parent
                    preferredRendererType: Shape.CurveRenderer
                    opacity: view.current < 0 || view.current === index ? 1 : 0.45
                    Accessible.ignored: true

                    ShapePath {
                        strokeWidth: -1
                        fillColor: Qt.alpha(view.bandColor(band.modelData), view.current === band.index ? 0.8 : 0.45)

                        PathPolyline {
                            path: view.bandPoints(band.modelData, plot.width, plot.height)
                        }
                    }
                }
            }

            Repeater {
                model: view.nodes

                Rectangle {
                    id: node

                    required property var modelData
                    required property int index

                    x: view.xOf(modelData.column, plot.width)
                    y: modelData.y * plot.height / view.whole
                    width: view.nodeWidth
                    height: Math.max(1, modelData.height * plot.height / view.whole)
                    color: view.nodeColors[index]
                    Accessible.ignored: true
                }
            }

            // The names beside the nodes. The outermost columns have their
            // names at the sides of the chart, the others beside the node.
            Repeater {
                model: view.nodes

                Controls.Label {
                    id: label

                    required property var modelData
                    required property int index

                    readonly property bool first: modelData.column === 0
                    readonly property bool last: modelData.column === view.columns - 1
                    readonly property real nodeHeight: modelData.height * plot.height / view.whole

                    visible: nodeHeight >= Kirigami.Units.gridUnit * 0.9 || first || last
                    text: view.nodeName(modelData) + "\n" + modelData.label
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    font.pixelSize: Kirigami.Theme.smallFont.pixelSize
                    horizontalAlignment: first ? Text.AlignRight : Text.AlignLeft
                    width: first || last ? view.labelWidth - Kirigami.Units.smallSpacing : implicitWidth
                    maximumLineCount: 2
                    x: first ? -view.labelWidth : view.xOf(modelData.column, plot.width) + view.nodeWidth + Kirigami.Units.smallSpacing
                    y: modelData.y * plot.height / view.whole + Math.max(0, (nodeHeight - implicitHeight) / 2)
                    background: Rectangle {
                        visible: !label.first && !label.last
                        color: Qt.alpha(Kirigami.Theme.backgroundColor, 0.75)
                    }
                    Accessible.ignored: true
                }
            }

            HoverHandler {
                id: hover

                onPointChanged: {
                    if (!hovered) {
                        return;
                    }
                    const found = view.bandAt(point.position.x, point.position.y, plot.width, plot.height);
                    if (found !== view.current) {
                        view.current = found;
                    }
                }
                onHoveredChanged: {
                    if (!hovered) {
                        view.current = -1;
                    }
                }
                cursorShape: view.current >= 0 ? Qt.PointingHandCursor : Qt.ArrowCursor
            }

            TapHandler {
                onTapped: point => {
                    const x = point.position.x;
                    const y = point.position.y;
                    const hit = view.nodeAt(x, y, plot.width, plot.height);
                    if (hit >= 0) {
                        view.flowActivated(hit, -1);
                        return;
                    }
                    const found = view.bandAt(x, y, plot.width, plot.height);
                    if (found >= 0) {
                        view.flowActivated(view.bands[found].from, view.bands[found].to);
                    }
                }
            }
        }
    }

    // What the flow in focus says, for the eyes as well as the screen reader.
    Controls.Label {
        Layout.fillWidth: true
        visible: !view.showTable && view.current >= 0 && view.current < view.bands.length
        text: visible ? view.bandText(view.bands[view.current]) : ""
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        font.bold: true
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: !view.showTable && view.chart.total !== ""
        text: i18nContext.i18nc("@info under the money-flow chart: %1 is how much passed through it", "Total: %1", view.chart.total)
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }

    // What other currencies hold is not drawn, but never silently missing.
    Controls.Label {
        Layout.fillWidth: true
        visible: view.chart.elsewhere.length > 0
        text: visible ? i18nContext.i18nc("@info under the money-flow chart: money in other currencies is not shown; %1 is a list of amounts", "Also %1 in another currency, not shown.", Array.from(view.chart.elsewhere).join(", ")) : ""
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Kirigami.Theme.disabledTextColor
    }

    // What an "Other" holds, so a folded node loses nothing.
    Controls.Label {
        Layout.fillWidth: true
        visible: text.length > 0
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Kirigami.Theme.disabledTextColor
        text: {
            const parts = [];
            for (const node of view.nodes) {
                if (node.kind === "other" && node.holds.length > 0) {
                    const held = Array.from(node.holds).map(h => (h.name !== "" ? h.name : view.nodeName({ kind: "no-payee", name: "", holds: [] })) + " " + h.label);
                    parts.push(view.nodeName(node) + ": " + held.join(", "));
                }
            }
            return parts.join("\n");
        }
    }

    // The same flows as a table: from, to, amount.
    DataTable {
        Layout.fillWidth: true
        visible: view.showTable
        title: i18nContext.i18nc("@label", "Values of the chart as a table")
        columns: ["from", "to", "amount"]
        columnNames: ({
            "from": i18nContext.i18nc("@title:column a table of the money-flow chart: where a flow comes from", "From"),
            "to": i18nContext.i18nc("@title:column a table of the money-flow chart: where a flow goes", "To"),
            "amount": i18nContext.i18nc("@title:column a table of a chart", "Amount")
        })
        rows: Array.from(view.bands).map(band => [view.nodeName(view.nodes[band.from]), view.nodeName(view.nodes[band.to]), band.label])
    }

    Controls.Button {
        text: Plain.plain(view.showTable
            ? i18nContext.i18nc("@action:button", "Show as chart")
            : i18nContext.i18nc("@action:button", "Show as table"))
        icon.name: view.showTable ? "office-chart-bar" : "view-list-details"
        onClicked: {
            ActionLog.taken("FlowChart_showTable")
            view.showTable = !view.showTable
        }
    }

    // The transactions behind a click (`FlowBehind.qml`), made when first
    // needed so the chart stands alone.
    Loader {
        id: behind

        active: false
        source: "FlowBehind.qml"
        onLoaded: show()

        function show() {
            item.period = view.period;
            item.scopeText = view.behindScope(view.behindCategory);
            item.heading = view.behindTitle;
            item.open();
        }

        function open() {
            if (active) {
                show();
            } else {
                active = true;
            }
        }
    }
}
