// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The first thing a new user sees: what KassenSturz does, and one button to
// start (M1.M).

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Kirigami.Page {
    id: page

    signal addAccountClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Welcome")
    header: MockNotice {}

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: parent.width - Kirigami.Units.gridUnit * 4
        icon.name: "wallet-open"
        text: page.i18nContext.i18nc("@title", "Welcome to KassenSturz")
        explanation: page.i18nContext.i18nc("@info", "KassenSturz shows you how much money you have and where it goes. Start with one account, such as your checking account or the cash in your wallet. Everything stays on this computer.")
        helpfulAction: Kirigami.Action {
            icon.name: "list-add"
            text: page.i18nContext.i18nc("@action:button", "Add your first account")
            onTriggered: page.addAccountClicked()
        }
    }
}
