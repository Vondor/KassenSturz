// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One transaction in a list: who, what for, when, and how much. Money that
// came in shows who it came from, money paid shows who it went to (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

RowLayout {
    id: row

    // A row from the Rust side: date, otherParty, category, amount and
    // exactAmount, incoming, and either account (on the overview) or
    // startingBalance (in a register).
    required property var modelData

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    readonly property string title: modelData.startingBalance
        ? i18nContext.i18nc("@label", "Starting balance")
        : modelData.otherParty === ""
            ? modelData.category
            : modelData.incoming
                ? i18nContext.i18nc("@label %1 is who the money came from", "From: %1", modelData.otherParty)
                : i18nContext.i18nc("@label %1 is who the money went to", "To: %1", modelData.otherParty)
    // What it was for, and on the overview which account it belongs to.
    readonly property string details: [modelData.date, modelData.otherParty === "" ? "" : modelData.category, modelData.account ?? ""]
        .filter(text => text !== "").join(" · ")

    spacing: Kirigami.Units.largeSpacing
    Accessible.role: Accessible.ListItem
    Accessible.name: [title, details, modelData.exactAmount].filter(text => text !== "").join(", ")

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 0

        Controls.Label {
            Layout.fillWidth: true
            text: row.title
            elide: Text.ElideRight
        }

        Controls.Label {
            Layout.fillWidth: true
            text: row.details
            elide: Text.ElideRight
            font: Kirigami.Theme.smallFont
        }
    }

    AmountLabel {
        text: row.modelData.amount
        exactText: row.modelData.exactAmount
        color: row.modelData.incoming ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.textColor
    }
}
