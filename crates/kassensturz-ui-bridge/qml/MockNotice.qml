// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Says on every page of the first-run mock that it is one (M1.M).

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
    text: i18nContext.i18nc("@info", "This is a first draft of how KassenSturz starts. Try it with made-up numbers: nothing is saved.")
}
