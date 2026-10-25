// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The way from Settings to the AI provider's page (M5.7). It is hidden with
// everything else about AI while the switch is off (D-013), so Settings
// itself never names AI outside the one toggle.

import QtQuick
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCard {
    id: entry

    required property AiSwitch ai

    // The user wants to set up the provider or read what is sent.
    signal openClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    visible: ai.enabled

    FormCard.FormButtonDelegate {
        icon.name: "configure"
        text: entry.i18nContext.i18nc("@action:button", "AI provider and privacy…")
        description: entry.i18nContext.i18nc("@info", "Choose which AI to use and where it runs, and read what KassenSturz may send to it.")
        onClicked: entry.openClicked()
    }
}
