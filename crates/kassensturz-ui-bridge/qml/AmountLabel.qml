// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Text with an amount the user may have chosen to see rounded, e.g. "54 €"
// for 53,90 € (D-002). Pointing at it shows the text with the exact amount,
// and screen readers get that too.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Label {
    id: label

    // The same text with the exact amount; nothing extra shows if it is the
    // same.
    property string exactText: text

    readonly property bool rounded: exactText !== text

    Accessible.name: exactText

    HoverHandler {
        id: hover
    }

    Controls.ToolTip.visible: rounded && hover.hovered
    Controls.ToolTip.text: exactText
    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
}
