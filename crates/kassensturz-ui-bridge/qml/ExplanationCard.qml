// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A short explanation at the top of a page, a question as its heading and
// one or two sentences under it (B-388, D-863): "What is a budget?" on the
// Budgets page, "What is a plan?" on the Plans page, so the two words are
// never mixed up.
//
//     ExplanationCard { heading: "What is a plan?"; explanation: "…" }

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.AbstractCard {
    id: card

    property string heading
    property string explanation

    Layout.fillWidth: true

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: card.heading
            font.bold: true
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }

        Controls.Label {
            Layout.fillWidth: true
            text: card.explanation
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
        }
    }
}
