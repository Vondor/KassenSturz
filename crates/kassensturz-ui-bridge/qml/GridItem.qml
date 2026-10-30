// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One component in the dashboard grid (M6b.3, D-287, D-288, D-311): shows the
// component with the real figures and, in edit mode, can be selected, dragged,
// resized and given a toolbar. Only the selected component has a toolbar and a
// resize corner (D-311). It moves with the animation of
// `Kirigami.Units.shortDuration`, which is 0 when the user turned animations
// off. The place and size are in cells; what is drawn follows from them, so
// nothing here decides where a component may be: the editor in Rust does, and
// a refused edit leaves the component where it was.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Item {
    id: cell

    // The `DashboardGrid` this component is on.
    required property var board
    // The component's ID in the document.
    required property string itemId

    readonly property var info: board.byId[itemId]
    readonly property bool present: info !== undefined
    readonly property bool selected: board.editing && board.selectedId === itemId
    readonly property bool narrow: board.narrow
    readonly property bool unknown: present && info.unknown
    // The component as ComponentPreview reads it. It is only taken again when
    // the component itself changed, so moving or resizing does not ask the
    // book for the figures again.
    property var subject: ({})
    readonly property string subjectJson: info?.subject?.json ?? ""
    readonly property bool hasProblem: unknown || (content.item?.failed ?? false)
    readonly property string heading: present ? names.titleOf(info) : ""
    property bool settled: false
    // Dragging and resizing by hand: the pointer sets the pixels, and the
    // editor snaps them to cells when the pointer is let go.
    property bool moving: false
    property bool resizing: false
    property real startX: 0
    property real startY: 0
    property real dragX: 0
    property real dragY: 0
    property real startW: 0
    property real startH: 0
    property real growW: 0
    property real growH: 0

    GridNames {
        id: names
    }

    function focusIt() {
        forceActiveFocus();
    }

    onSubjectJsonChanged: subject = info?.subject ?? ({})

    visible: present
    z: selected ? 2 : (moving || resizing ? 3 : 0)
    x: moving ? startX + dragX : (present ? board.frameX(info) : 0)
    y: moving ? startY + dragY : (present ? board.frameY(info) : 0)
    width: resizing ? Math.max(growW, board.cellWidth(info.minCols)) : (present ? board.frameWidth(info) : 0)
    height: resizing ? Math.max(growH, board.frameHeight(info.minRows)) : (present ? board.frameHeight(info.rows) : 0)

    Behavior on x {
        enabled: cell.settled && !cell.moving
        NumberAnimation { duration: Kirigami.Units.shortDuration; easing.type: Easing.OutCubic }
    }
    Behavior on y {
        enabled: cell.settled && !cell.moving
        NumberAnimation { duration: Kirigami.Units.shortDuration; easing.type: Easing.OutCubic }
    }
    Behavior on width {
        enabled: cell.settled && !cell.resizing
        NumberAnimation { duration: Kirigami.Units.shortDuration; easing.type: Easing.OutCubic }
    }
    Behavior on height {
        enabled: cell.settled && !cell.resizing
        NumberAnimation { duration: Kirigami.Units.shortDuration; easing.type: Easing.OutCubic }
    }

    Component.onCompleted: {
        subject = info?.subject ?? ({});
        // The first place a component gets is not an animation.
        Qt.callLater(() => cell.settled = true);
    }

    activeFocusOnTab: (board?.editing && !board.preview) || activeFocus
    Accessible.role: info?.type === "text.heading" ? Accessible.Heading : Accessible.Pane
    Accessible.name: present ? names.name(info) : ""
    Accessible.focusable: board?.editing ?? false

    Keys.onPressed: event => board.keyPressed(cell, event)
    Keys.onTabPressed: event => board.focusNeighbour(itemId, 1, event)
    Keys.onBacktabPressed: event => board.focusNeighbour(itemId, -1, event)

    onHasProblemChanged: board.countProblems()
    onPresentChanged: board.countProblems()

    // Drag by hand, from the component or from the Move button.
    function dragStarted() {
        board.select(itemId);
        startX = x;
        startY = y;
        dragX = 0;
        dragY = 0;
        moving = true;
    }

    function dragMoved(translation: point) {
        dragX = translation.x;
        dragY = translation.y;
        const col = board.layout.snap(startX + dragX, board.cell + board.gap);
        const row = board.layout.snap(startY + dragY, board.rowHeight + board.gap);
        board.ghost = {col: Math.max(0, Math.min(col, board.columns - info.cols)), row: Math.max(0, row), cols: info.cols, rows: info.rows};
    }

    function dragEnded() {
        const col = board.layout.snap(startX + dragX, board.cell + board.gap);
        const row = board.layout.snap(startY + dragY, board.rowHeight + board.gap);
        board.ghost = null;
        moving = false;
        board.edit(board.layout.moveTo(itemId, col, row));
    }

    function growStarted() {
        board.select(itemId);
        startW = width;
        startH = height;
        growW = width;
        growH = height;
        resizing = true;
    }

    function growMoved(translation: point) {
        growW = startW + translation.x;
        growH = startH + translation.y;
        const cols = board.layout.snap(growW + board.gap, board.cell + board.gap);
        const rows = board.layout.snap(growH + board.gap, board.rowHeight + board.gap);
        board.ghost = {col: info.col, row: info.row, cols: Math.max(info.minCols, Math.min(cols, board.columns - info.col)), rows: Math.max(info.minRows, rows)};
    }

    function growEnded() {
        const cols = board.layout.snap(growW + board.gap, board.cell + board.gap);
        const rows = board.layout.snap(growH + board.gap, board.rowHeight + board.gap);
        board.ghost = null;
        resizing = false;
        board.edit(board.layout.resizeTo(itemId, cols, rows));
    }

    function stopDragging() {
        moveBody.enabled = false;
        moveBody.enabled = true;
        moveByHandle.enabled = false;
        moveByHandle.enabled = true;
        grow.enabled = false;
        grow.enabled = true;
    }

    Rectangle {
        id: frame

        anchors.fill: parent
        clip: true
        color: Kirigami.Theme.backgroundColor
        radius: Kirigami.Units.smallSpacing
        border.width: cell.selected ? 2 : 1
        border.color: cell.selected ? Kirigami.Theme.highlightColor
            : cell.hasProblem ? Kirigami.Theme.neutralTextColor
            : cell.board.editing && !cell.board.preview ? Kirigami.Theme.disabledTextColor
            : Qt.alpha(Kirigami.Theme.textColor, 0.15)

        Loader {
            id: content

            anchors.fill: parent
            anchors.margins: Kirigami.Units.smallSpacing
            // In edit mode the component is a picture of itself: the overlay
            // above takes the pointer, which selects and drags it.
            active: cell.present
            sourceComponent: cell.unknown ? unknownBox
                : cell.info?.type === "text.heading" ? headingBox : figuresBox
        }
    }

    Component {
        id: headingBox

        Kirigami.Heading {
            level: 2
            text: cell?.heading ?? ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
            readonly property bool failed: false
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }
    }

    Component {
        id: unknownBox

        Kirigami.PlaceholderMessage {
            readonly property bool failed: true
            text: names.i18nContext.i18nc("@info", "This part of the layout was made by a newer KassenSturz, so this one can't show it. It is kept as it is.")
        }
    }

    Component {
        id: figuresBox

        Controls.ScrollView {
            id: scroller

            readonly property bool failed: figures.failed

            contentWidth: availableWidth
            Accessible.name: names.i18nContext.i18nc("@info the figures of a component, which may need scrolling", "%1, scrolls if it does not fit", cell.present ? names.name(cell.info) : "")

            ComponentPreview {
                id: figures

                width: scroller.availableWidth
                subject: cell.subject
                period: cell.board?.periodFor(cell.info) ?? ""
                fallbackTitle: names.automaticTitle(cell.info)
            }
        }
    }

    // Edit mode: the pointer selects and drags a component.
    Item {
        id: overlay

        anchors.fill: parent
        visible: cell.board.editing && !cell.board.preview

        TapHandler {
            onTapped: cell.board.select(cell.itemId)
        }

        DragHandler {
            id: moveBody

            target: null
            enabled: !cell.narrow
            cursorShape: Qt.SizeAllCursor
            onActiveChanged: active ? cell.dragStarted() : cell.dragEnded()
            onActiveTranslationChanged: if (active) cell.dragMoved(activeTranslation)
        }
    }

    // The toolbar of the selected component only (D-311).
    Rectangle {
        id: bar

        visible: cell.selected && !cell.board.preview && !cell.moving && !cell.resizing
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: Kirigami.Units.smallSpacing
        width: Math.min(barRow.implicitWidth + Kirigami.Units.smallSpacing * 2, parent.width - Kirigami.Units.smallSpacing * 2)
        height: barRow.implicitHeight + Kirigami.Units.smallSpacing
        radius: Kirigami.Units.smallSpacing
        color: Kirigami.Theme.backgroundColor
        border.width: 1
        border.color: Kirigami.Theme.highlightColor

        RowLayout {
            id: barRow

            anchors.fill: parent
            anchors.leftMargin: Kirigami.Units.smallSpacing / 2
            anchors.rightMargin: Kirigami.Units.smallSpacing / 2
            spacing: 0

            Controls.ToolButton {
                id: moveHandle

                visible: !cell.narrow
                icon.name: "transform-move"
                text: names.i18nContext.i18nc("@action:button drag this to move the component", "Move")
                display: Controls.AbstractButton.IconOnly
                Controls.ToolTip.text: names.i18nContext.i18nc("@info:tooltip", "Drag to move. Or use the arrow keys.")
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay

                DragHandler {
                    id: moveByHandle

                    target: null
                    cursorShape: Qt.SizeAllCursor
                    onActiveChanged: active ? cell.dragStarted() : cell.dragEnded()
                    onActiveTranslationChanged: if (active) cell.dragMoved(activeTranslation)
                }
            }

            Kirigami.ActionToolBar {
                Layout.fillWidth: true
                alignment: Qt.AlignRight
                actions: [
                    Kirigami.Action {
                        visible: cell.narrow
                        icon.name: "go-up"
                        text: names.i18nContext.i18nc("@action:button move the component up the list", "Move earlier")
                        displayHint: Kirigami.DisplayHint.IconOnly
                        onTriggered: cell.board.edit(cell.board.layout.moveEarlier(cell.itemId))
                    },
                    Kirigami.Action {
                        visible: cell.narrow
                        icon.name: "go-down"
                        text: names.i18nContext.i18nc("@action:button move the component down the list", "Move later")
                        displayHint: Kirigami.DisplayHint.IconOnly
                        onTriggered: cell.board.edit(cell.board.layout.moveLater(cell.itemId))
                    },
                    Kirigami.Action {
                        icon.name: "edit-copy"
                        text: names.i18nContext.i18nc("@action:button copy the component", "Duplicate")
                        displayHint: Kirigami.DisplayHint.IconOnly
                        onTriggered: cell.board.duplicate(cell.itemId)
                    },
                    Kirigami.Action {
                        icon.name: "configure"
                        text: names.i18nContext.i18nc("@action:button change what the component shows", "Properties")
                        displayHint: Kirigami.DisplayHint.IconOnly
                        enabled: !cell.unknown
                        onTriggered: cell.board.propertiesRequested(cell.itemId)
                    },
                    Kirigami.Action {
                        icon.name: "edit-delete"
                        text: names.i18nContext.i18nc("@action:button take the component out of the layout", "Remove")
                        displayHint: Kirigami.DisplayHint.IconOnly
                        onTriggered: cell.board.remove(cell.itemId)
                    }
                ]
            }
        }
    }

    // The corner a selected component is resized by.
    Rectangle {
        id: corner

        visible: cell.selected && !cell.board.preview && !cell.narrow
        width: Kirigami.Units.gridUnit
        height: Kirigami.Units.gridUnit
        x: cell.width - width
        y: cell.height - height
        color: Kirigami.Theme.highlightColor
        radius: Kirigami.Units.smallSpacing
        opacity: grow.active ? 1 : 0.8

        Kirigami.Icon {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.smallSpacing
            height: width
            source: "transform-scale"
            color: Kirigami.Theme.highlightedTextColor
            isMask: true
        }

        Accessible.role: Accessible.Button
        Accessible.name: names.i18nContext.i18nc("@action:button drag this to make the component larger or smaller", "Resize")

        DragHandler {
            id: grow

            target: null
            cursorShape: Qt.SizeFDiagCursor
            onActiveChanged: active ? cell.growStarted() : cell.growEnded()
            onActiveTranslationChanged: if (active) cell.growMoved(activeTranslation)
        }
    }
}
