// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The top row of every page of the guided setup (#1031,
// docs/GUIDED_SETUP.md): which step this is, how many there are, and "End the
// setup", which always leads to the overview (D-342). A step opened from Help
// stands alone: `step` is 0 there and the row shows nothing.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Controls.ToolBar {
    id: bar

    // This page's step, counted from 1; 0 when the step was opened from Help.
    property int step: 0
    // How many steps the guided setup has in this build.
    property int count: 0

    signal endClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    visible: step > 0
    // A hidden row takes no room.
    height: visible ? implicitHeight : 0

    contentItem: RowLayout {
        spacing: Kirigami.Units.largeSpacing

        Controls.Label {
            Layout.leftMargin: Kirigami.Units.largeSpacing
            textFormat: Text.PlainText
            text: bar.i18nContext.i18nc("@info %1 is this step's number, %2 how many steps there are", "Step %1 of %2", bar.step, bar.count)
        }

        Controls.ProgressBar {
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 12
            from: 0
            to: Math.max(1, bar.count)
            value: bar.step
            // The label next to it says the same in words.
            Accessible.ignored: true
        }

        Item {
            Layout.fillWidth: true
        }

        Controls.ToolButton {
            icon.name: "window-close"
            text: bar.i18nContext.i18nc("@action:button leaves the guided setup and shows the overview", "End the setup")
            onClicked: {
                ActionLog.taken("GuidedBar_end")
                bar.endClicked()
            }
        }
    }
}
