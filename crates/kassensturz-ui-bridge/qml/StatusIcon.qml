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
// A tile that names its `emblem` (D-750) shows its own drawn symbol in the
// theme's text colour, so tiles can be told apart, and the state becomes a
// badge in its lower right corner; while something is to do or failed, the
// icon also gets a tinted frame.
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
    // Which tile it is, by its type: `notice.alarms` (a bell),
    // `notice.unsorted` (a tag), `notice.fetch_gap` (a bank with a clock) or
    // `bank.update_all` (two turning arrows). Any other word draws no emblem
    // and the state shape fills the icon on its own.
    property string emblem: ""

    signal opened

    readonly property string shown: ["fine", "to-do", "running", "failed", "unknown"].includes(status) ? status : "unknown"
    readonly property color tint: shown === "fine" ? Kirigami.Theme.positiveTextColor
        : shown === "to-do" ? Kirigami.Theme.neutralTextColor
        : shown === "running" ? Kirigami.Theme.highlightColor
        : shown === "failed" ? Kirigami.Theme.negativeTextColor
        : Kirigami.Theme.disabledTextColor
    readonly property bool hasEmblem: ["notice.alarms", "notice.unsorted", "notice.fetch_gap", "bank.update_all"].includes(emblem)
    // A frame in the state's colour when something wants a look.
    readonly property bool flagged: hasEmblem && (shown === "to-do" || shown === "failed")
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
        color: icon.visualFocus ? Qt.alpha(Kirigami.Theme.highlightColor, 0.15)
            : icon.flagged ? Qt.alpha(icon.tint, 0.12) : "transparent"
        border.width: icon.visualFocus || icon.flagged ? 1 : 0
        border.color: icon.visualFocus ? Kirigami.Theme.highlightColor : icon.tint
    }

    contentItem: Item {
        RowLayout {
            anchors.centerIn: parent
            width: icon.wordsShown ? parent.width : implicitWidth
            spacing: Kirigami.Units.smallSpacing

            Item {
                Layout.preferredWidth: icon.side
                Layout.preferredHeight: icon.side
                Layout.alignment: Qt.AlignVCenter

            // The tile's own symbol, drawn from 24 × 24 outlines like the
            // shapes below, in the text colour of the theme (light or dark).
            Canvas {
                id: emblemShape

                objectName: "emblem"
                anchors.fill: parent
                visible: icon.hasEmblem
                readonly property string key: icon.emblem + String(Kirigami.Theme.textColor) + width
                onKeyChanged: requestPaint()
                Accessible.ignored: true

                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();
                    if (!icon.hasEmblem) {
                        return;
                    }
                    ctx.scale(width / 24, height / 24);
                    ctx.strokeStyle = Kirigami.Theme.textColor;
                    ctx.lineWidth = 1.6;
                    ctx.lineCap = "round";
                    ctx.lineJoin = "round";
                    const outline = icon.emblem === "notice.alarms" ? "M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z M10 20.5a2 2 0 0 0 4 0"
                        : icon.emblem === "notice.unsorted" ? "M3 12V4h8l10 10-8 8z"
                        : icon.emblem === "notice.fetch_gap" ? "M2.5 9L11 4l8.5 5z M4.5 9.5v7 M8 9.5v7 M11.5 9.5v4 M2.5 19h9 M17.5 14v2.5l1.8 1.2"
                        : "M19.5 8.5A8 8 0 0 0 5 7 M19.5 4v4.5H15 M4.5 15.5A8 8 0 0 0 19 17 M4.5 20v-4.5H9";
                    ctx.path = outline;
                    ctx.stroke();
                    // The round parts: the tag's hole and the bank's clock.
                    if (icon.emblem === "notice.unsorted") {
                        ctx.beginPath();
                        ctx.arc(7.5, 8.5, 1.5, 0, 2 * Math.PI);
                        ctx.stroke();
                    } else if (icon.emblem === "notice.fetch_gap") {
                        ctx.beginPath();
                        ctx.arc(17.5, 16.5, 4.5, 0, 2 * Math.PI);
                        ctx.stroke();
                    }
                }
            }

            Canvas {
                id: shape

                objectName: "state"
                // Alone it fills the icon; beside an emblem it is a badge.
                width: icon.hasEmblem ? Math.round(parent.width * 0.5) : parent.width
                height: width
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.rightMargin: icon.hasEmblem ? -Math.round(parent.width * 0.08) : 0
                anchors.bottomMargin: icon.hasEmblem ? -Math.round(parent.width * 0.08) : 0
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
