// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The bottom row of every page of the guided setup (#1031,
// docs/GUIDED_SETUP.md): "Skip" on the left, which writes nothing and goes to
// the next step (D-911), and the page's one main button on the right.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Controls.ToolBar {
    id: footer

    // The main button's words; the page says what its step does.
    required property string nextText
    property bool nextEnabled: true
    // A step opened from Help has nothing to skip to.
    property bool skipVisible: true

    signal skipClicked()
    signal nextClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    position: Controls.ToolBar.Footer

    contentItem: RowLayout {
        spacing: Kirigami.Units.largeSpacing

        Controls.Button {
            Layout.leftMargin: Kirigami.Units.largeSpacing
            visible: footer.skipVisible
            flat: true
            text: footer.i18nContext.i18nc("@action:button goes to the next step of the guided setup without doing this one", "Skip")
            onClicked: {
                ActionLog.taken("GuidedFooter_skip")
                footer.skipClicked()
            }
        }

        Item {
            Layout.fillWidth: true
        }

        Controls.Button {
            Layout.rightMargin: Kirigami.Units.largeSpacing
            highlighted: true
            enabled: footer.nextEnabled
            icon.name: "go-next"
            text: Plain.plain(footer.nextText)
            onClicked: {
                ActionLog.taken("GuidedFooter_next")
                footer.nextClicked()
            }
        }
    }
}
