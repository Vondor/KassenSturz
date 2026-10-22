// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Says on every page of the first-run mock that it is one (M1.M). Since
// M3.1 what the user enters is saved in the open book.

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Kirigami.InlineMessage {
    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    position: Kirigami.InlineMessage.Position.Header
    type: Kirigami.MessageType.Information
    visible: true
    text: i18nContext.i18nc("@info", "This is an early test version of KassenSturz. Try it with made-up numbers: what you enter is saved in your book.")
}
