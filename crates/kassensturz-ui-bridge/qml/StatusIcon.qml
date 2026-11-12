// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The status icon a tile shows below its full size (US5.4, D-734, D-735,
// `docs/STATUS_TILES.md`). One of five states, each with its own shape and
// mark as well as a colour, so it never depends on colour alone: a filled
// circle with a tick (fine), a filled triangle with "!" (something to do), a
// turning ring (running), a filled diamond with "×" (failed) and a hollow
// circle with "?" (unknown). The shapes are drawn here, not taken from the
// icon theme, so they look the same on every platform. The state is decided
// in Rust; this only draws it and its words.
//
// On the page it is a button: a click, a tap, Enter or Space asks for the
// whole tile (`opened`, D-736). While the layout is arranged it takes no
// pointer, so the grid's overlay selects the tile as for every other one.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Controls.AbstractButton {
    id: icon

    // `fine`, `to-do`, `running`, `failed` or `unknown`; any other word,
    // also an empty one (figures not there yet), shows as `unknown`.
    property string status: "unknown"
    // The tile's name, for the screen reader.
    property string name: ""
    // The short words: beside the icon when `wordsShown`, and in the
    // screen-reader name.
    property string words: ""
    // The sentence of the hover text, without "Click to see the whole tile."
    property string sentence: ""
    // Whether the short words stand beside the icon (from 3 columns, and in
    // the one-column view).
    property bool wordsShown: false
    // While arranging the tile is a picture of itself.
    property bool arranging: false

    signal opened

    readonly property string shown: ["fine", "to-do", "running", "failed", "unknown"].includes(status) ? status : "unknown"
    readonly property color tint: shown === "fine" ? Kirigami.Theme.positiveTextColor
        : shown === "to-do" ? Kirigami.Theme.neutralTextColor
        : shown === "running" ? Kirigami.Theme.highlightColor
        : shown === "failed" ? Kirigami.Theme.negativeTextColor
        : Kirigami.Theme.disabledTextColor
    readonly property string hint: sentence === "" ? clickHint : sentence + " " + clickHint
    property string clickHint: ""
    // As large as the shorter side allows, at most 3 grid units.
    readonly property real side: Math.max(Kirigami.Units.iconSizes.small, Math.min(height, wordsShown ? height : width, 3 * Kirigami.Units.gridUnit))

    enabled: !arranging
    focusPolicy: arranging ? Qt.NoFocus : Qt.StrongFocus
    hoverEnabled: true
    padding: 0
    Accessible.role: Accessible.Button
    Accessible.name: name === "" ? words : name + ": " + words
    Accessible.description: hint
    Keys.onReturnPressed: icon.clicked()
    Keys.onEnterPressed: icon.clicked()
    onClicked: {
        ActionLog.taken("StatusIcon_open");
        icon.opened();
    }

    Controls.ToolTip.visible: hovered && !arranging && hint !== ""
    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
    Controls.ToolTip.text: hint

    background: Rectangle {
        radius: Kirigami.Units.smallSpacing
        color: icon.visualFocus ? Qt.alpha(Kirigami.Theme.highlightColor, 0.15) : "transparent"
        border.width: icon.visualFocus ? 1 : 0
        border.color: Kirigami.Theme.highlightColor
    }

    contentItem: Item {
        RowLayout {
            anchors.centerIn: parent
            width: icon.wordsShown ? parent.width : implicitWidth
            spacing: Kirigami.Units.smallSpacing

            Canvas {
                id: shape

                Layout.preferredWidth: icon.side
                Layout.preferredHeight: icon.side
                Layout.alignment: Qt.AlignVCenter
                // Drawn again when the state, the colour or the size changes.
                readonly property string key: icon.shown + String(icon.tint) + width
                onKeyChanged: requestPaint()
                Accessible.ignored: true

                onPaint: {
                    const ctx = getContext("2d");
                    const s = width;
                    ctx.reset();
                    ctx.fillStyle = icon.tint;
                    ctx.strokeStyle = icon.tint;
                    ctx.lineWidth = Math.max(1.5, s / 12);
                    const c = s / 2;
                    const r = s / 2 - ctx.lineWidth;
                    ctx.beginPath();
                    switch (icon.shown) {
                    case "fine":
                        ctx.arc(c, c, r, 0, 2 * Math.PI);
                        ctx.fill();
                        break;
                    case "to-do":
                        ctx.moveTo(c, ctx.lineWidth);
                        ctx.lineTo(s - ctx.lineWidth, s - ctx.lineWidth);
                        ctx.lineTo(ctx.lineWidth, s - ctx.lineWidth);
                        ctx.closePath();
                        ctx.fill();
                        break;
                    case "failed":
                        ctx.moveTo(c, ctx.lineWidth);
                        ctx.lineTo(s - ctx.lineWidth, c);
                        ctx.lineTo(c, s - ctx.lineWidth);
                        ctx.lineTo(ctx.lineWidth, c);
                        ctx.closePath();
                        ctx.fill();
                        break;
                    case "running":
                        // A ring with a gap; the item turns, not the drawing.
                        ctx.arc(c, c, r, 0, 1.6 * Math.PI);
                        ctx.stroke();
                        break;
                    default:
                        ctx.arc(c, c, r, 0, 2 * Math.PI);
                        ctx.stroke();
                        break;
                    }
                }

                RotationAnimator on rotation {
                    running: icon.shown === "running" && Kirigami.Units.longDuration > 1
                    from: 0
                    to: 360
                    duration: 2000
                    loops: Animation.Infinite
                    onRunningChanged: if (!running) shape.rotation = 0
                }

                // The mark inside the shape; the running ring has none.
                Text {
                    anchors.centerIn: parent
                    anchors.verticalCenterOffset: icon.shown === "to-do" ? parent.height * 0.1 : 0
                    visible: icon.shown !== "running"
                    text: icon.shown === "fine" ? "✓" : icon.shown === "to-do" ? "!" : icon.shown === "failed" ? "×" : "?"
                    color: icon.shown === "unknown" ? icon.tint : Kirigami.Theme.backgroundColor
                    font.bold: true
                    font.pixelSize: parent.height * (icon.shown === "to-do" ? 0.45 : 0.55)
                    Accessible.ignored: true
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                visible: icon.wordsShown && icon.words !== ""
                text: icon.words
                textFormat: Text.PlainText
                elide: Text.ElideRight
                maximumLineCount: 1
                Accessible.ignored: true
            }
        }
    }
}
