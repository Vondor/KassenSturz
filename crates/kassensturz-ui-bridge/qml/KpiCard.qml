// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One key figure (M6a.5): its name, the figure, and how it changed against
// the earlier period. The savings rate also gets a bar. Everything shown is
// text from `BookCards`; nothing here works out a figure (D-065).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.AbstractCard {
    id: card

    // One card of a `BookCards.view` set: measure, value, previous, change,
    // changePercent, direction and, for the savings rate, permille (all text
    // but permille).
    required property var modelData

    readonly property bool hasValue: modelData.value !== ""
    readonly property bool hasBar: modelData.permille !== undefined

    KpiNames {
        id: names
    }

    // The figure, how it changed and what it was, as one sentence for screen
    // readers.
    readonly property string summary: {
        if (!hasValue)
            return names.noValue();
        const parts = [modelData.value];
        if (modelData.direction !== "none")
            parts.push(names.direction(modelData.direction));
        if (modelData.change !== "")
            parts.push(modelData.change);
        if (modelData.changePercent !== "")
            parts.push(modelData.changePercent);
        if (modelData.previous !== "")
            parts.push(names.before(modelData.previous));
        return parts.join(", ");
    }

    Accessible.name: names.measure(modelData.measure)
    Accessible.description: summary

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: names.measure(card.modelData.measure)
            elide: Text.ElideRight
            font: Kirigami.Theme.smallFont
            textFormat: Text.PlainText
        }

        AmountLabel {
            Layout.fillWidth: true
            text: card.hasValue ? card.modelData.value : names.noValue()
            // Pointing at a rounded figure shows it exactly (D-002).
            exactText: card.hasValue ? (card.modelData.exactValue ?? card.modelData.value) : text
            // What is left below zero is red, besides its minus sign.
            color: card.modelData.negative === true && card.modelData.measure === "net"
                ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
            font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.6
            font.bold: true
            wrapMode: Text.Wrap
        }

        RowLayout {
            Layout.fillWidth: true
            visible: card.hasValue && card.modelData.direction !== "none"
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                text: names.arrow(card.modelData.direction)
                textFormat: Text.PlainText
                // The words beside it say the same.
                Accessible.ignored: true
            }

            Controls.Label {
                Layout.fillWidth: true
                text: [names.direction(card.modelData.direction), card.modelData.change, card.modelData.changePercent]
                    .filter(part => part !== "").join(" · ")
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
                textFormat: Text.PlainText
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: card.hasValue && card.modelData.previous !== ""
            text: names.before(card.modelData.previous)
            font: Kirigami.Theme.smallFont
            textFormat: Text.PlainText
            opacity: 0.8
        }

        ProgressMeter {
            Layout.fillWidth: true
            visible: card.hasBar && card.hasValue
            title: ""
            valueText: card.modelData.value
            permille: card.hasBar ? card.modelData.permille : 0
            // The card already says the figure in words.
            Accessible.ignored: true
        }
    }
}
