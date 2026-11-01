// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A small picture of a template's grid in *New dashboard…* (M6d.2, D-440):
// each widget's frame as a box with the icon of its kind, on the 12 columns
// of a dashboard. The frames come from Rust (`DashboardLayout.templates`);
// an empty template is an empty page with a plus.

import QtQuick
import org.kde.kirigami as Kirigami

Rectangle {
    id: picture

    // The template's `frames`: objects with `type`, `col`, `row`, `cols`
    // and `rows`.
    property var frames: []
    // How many grid rows the template spans.
    property int rows: 0

    readonly property real cell: width / 12
    // Grid cells are wider than high, as on the page.
    readonly property real rowHeight: cell * 0.6
    readonly property int shownRows: Math.max(rows, 10)

    function iconOf(type: string): string {
        switch (type) {
        case "chart.bar":
        case "progress.bar":
            return "office-chart-bar";
        case "chart.stacked_bar":
            return "office-chart-bar-stacked";
        case "chart.pie":
            return "office-chart-pie";
        case "chart.donut":
            return "office-chart-ring";
        case "chart.line":
            return "office-chart-line";
        case "chart.area":
            return "office-chart-area";
        case "kpi.card":
        case "card.net_worth":
            return "view-statistics";
        case "table.rows":
        case "table.income_sources":
            return "view-list-details";
        case "budget.progress":
            return "view-financial-budget";
        case "text.heading":
            return "format-text-bold";
        case "notice.fetch_gap":
        case "notice.transfers":
            return "dialog-information";
        case "bank.update_all":
            return "view-refresh";
        }
        return "view-grid";
    }

    implicitHeight: shownRows * rowHeight
    radius: Kirigami.Units.cornerRadius
    color: Kirigami.Theme.backgroundColor
    border.width: 1
    border.color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.25)
    clip: true
    // The name and the line under the picture say what it shows.
    Accessible.ignored: true

    Repeater {
        model: picture.frames

        Rectangle {
            id: box

            required property var modelData

            x: box.modelData.col * picture.cell + 1
            y: box.modelData.row * picture.rowHeight + 1
            width: box.modelData.cols * picture.cell - 2
            height: box.modelData.rows * picture.rowHeight - 2
            radius: 2
            color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.highlightColor, 0.15)
            border.width: 1
            border.color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.highlightColor, 0.5)

            Kirigami.Icon {
                anchors.centerIn: parent
                width: Math.min(Kirigami.Units.iconSizes.small, parent.height - 2)
                height: width
                source: picture.iconOf(box.modelData.type)
            }
        }
    }

    Kirigami.Icon {
        anchors.centerIn: parent
        visible: picture.frames.length === 0
        width: Kirigami.Units.iconSizes.medium
        height: width
        source: "list-add"
        opacity: 0.5
    }
}
