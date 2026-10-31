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
    readonly property string heading: present ? words.resolve(names.titleOf(info), board.periodFor(info)) : ""
    // The separator and the notices have no card around them while the page
    // is only looked at: a line and a message (D-328, D-334).
    readonly property bool isBox: present && info.type === "group.period"
    readonly property bool bare: present && (info.type === "text.separator" || info.type === "notice.fetch_gap" || info.type === "notice.transfers" || info.type === "card.net_worth")
    readonly property bool showsFrame: !bare || (board.editing && !board.preview)
    // What a period box says: its title and the period it gives its widgets.
    readonly property string boxPeriod: isBox ? names.periodName(info.subject?.period?.word ?? "") : ""
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

    TitleWords {
        id: words
    }

    GridNames {
        id: names
    }

    function focusIt() {
        forceActiveFocus();
    }

    onSubjectJsonChanged: subject = info?.subject ?? ({})

    visible: present
    // A period box lies under the widgets it holds (D-333).
    z: isBox ? (moving || resizing ? 3 : -1) : (selected ? 2 : (moving || resizing ? 3 : 0))
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
        color: cell.isBox ? Qt.alpha(Kirigami.Theme.highlightColor, 0.06) : (cell.showsFrame ? Kirigami.Theme.backgroundColor : "transparent")
        radius: Kirigami.Units.smallSpacing
        border.width: cell.selected ? 2 : (cell.showsFrame ? 1 : 0)
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
                : cell.info?.type === "text.heading" ? headingBox
                : cell.info?.type === "group.period" ? boxLabel
                : cell.info?.type === "card.net_worth" ? netWorthBox
                : cell.info?.type === "table.income_sources" ? incomeBox
                : cell.info?.type === "text.separator" ? separatorBox
                : cell.info?.type === "notice.fetch_gap" ? fetchGapBox
                : cell.info?.type === "notice.transfers" ? transfersBox : figuresBox
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

    // A period box: only its title and the period it gives the widgets
    // inside it, along the top; the widgets are drawn over it (D-333).
    Component {
        id: boxLabel

        RowLayout {
            readonly property bool failed: false

            Kirigami.Heading {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                level: 4
                text: cell.heading
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }

            Controls.Label {
                Layout.alignment: Qt.AlignTop
                text: cell.boxPeriod
                textFormat: Text.PlainText
                color: Kirigami.Theme.disabledTextColor
            }
        }
    }

    // What all the accounts hold together less what is owed, a card for each
    // currency (Q-215, D-329). It is today's figure, so it stays when the
    // month changes. The page's own figures, as text (D-065).
    Component {
        id: netWorthBox

        Flickable {
            readonly property bool failed: false
            readonly property var sections: cell.board.pageView.sections ?? []

            contentHeight: worthColumn.implicitHeight
            clip: true

            ColumnLayout {
                id: worthColumn

                width: parent.width
                spacing: Kirigami.Units.smallSpacing

                Repeater {
                    model: sections

                    delegate: ColumnLayout {
                        id: section

                        required property var modelData
                        readonly property string value: modelData.togetherOwes
                            ? names.i18nContext.i18nc("@label %1 is an amount of money", "You owe %1", modelData.together)
                            : modelData.together
                        readonly property string exact: modelData.togetherOwes
                            ? names.i18nContext.i18nc("@label %1 is an amount of money", "You owe %1", modelData.exactTogether)
                            : modelData.exactTogether

                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing

                        // Only a book with several currencies needs to say which.
                        Kirigami.Heading {
                            textFormat: Text.PlainText
                            visible: sections.length > 1
                            text: section.modelData.currency
                            level: 3
                        }

                        KpiGrid {
                            cards: [{
                                measure: "net-worth",
                                currency: section.modelData.currency,
                                value: section.value,
                                exactValue: section.exact,
                                negative: section.modelData.togetherOwes,
                                previous: "",
                                change: "",
                                changePercent: "",
                                direction: "none"
                            }]
                        }
                    }
                }
            }
        }
    }

    // Where the month's money came from, by category and currency (D-336).
    Component {
        id: incomeBox

        Flickable {
            readonly property bool failed: false
            readonly property var totals: (cell.board.pageView.totals ?? []).filter(entry => entry.income.length > 0)

            contentHeight: incomeColumn.implicitHeight
            clip: true

            ColumnLayout {
                id: incomeColumn

                width: parent.width
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    visible: cell.board.editing && totals.length === 0
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                    text: names.i18nContext.i18nc("@info", "Where the month's money came from appears here once something came in.")
                }

                Repeater {
                    model: totals

                    delegate: ColumnLayout {
                        id: monthTotals

                        required property var modelData

                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing

                        Kirigami.Heading {
                            Layout.fillWidth: true
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            text: totals.length > 1
                                ? names.i18nContext.i18nc("@title %1 is a currency", "Where your money came from (%1)", monthTotals.modelData.currency)
                                : names.i18nContext.i18nc("@title", "Where your money came from")
                            level: 3
                        }

                        Repeater {
                            model: monthTotals.modelData.income

                            delegate: AmountLine {
                                Layout.fillWidth: true
                                name: modelData.unsorted ? names.i18nContext.i18nc("@label money with no category yet", "Not sorted yet") : modelData.name
                            }
                        }
                    }
                }
            }
        }
    }

    // A thin line across the cell (D-334), like Kirigami.Separator.
    Component {
        id: separatorBox

        Item {
            readonly property bool failed: false

            // The line follows the shape of its box: a box that is taller
            // than it is wide holds an upright line (D-337).
            readonly property bool upright: height > width

            Kirigami.Separator {
                anchors.centerIn: parent
                width: parent.upright ? implicitWidth : parent.width
                height: parent.upright ? parent.height : implicitHeight
            }
        }
    }

    // The warnings about bank accounts that were not updated for a long time
    // (M5.U, D-180), one message each, as the page showed them before it
    // was a widget (D-328). Nothing is shown when there is none; while the
    // layout is arranged, a hint says what the widget is.
    Component {
        id: fetchGapBox

        Flickable {
            readonly property bool failed: false
            readonly property var gaps: cell.board.pageView.fetchGaps ?? []

            contentHeight: gapColumn.implicitHeight
            clip: true

            ColumnLayout {
                id: gapColumn

                width: parent.width
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    visible: cell.board.editing && gaps.length === 0
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                    text: names.i18nContext.i18nc("@info", "Warnings about bank accounts that were not updated for a long time appear here. There are none right now.")
                }

                Repeater {
                    model: gaps

                    delegate: Kirigami.InlineMessage {
                        id: gap

                        required property var modelData

                        Layout.fillWidth: true
                        type: Kirigami.MessageType.Warning
                        visible: true
                        text: Plain.plain(names.i18nContext.i18nc("@info %1 is how many days, %2 an account's name, %3 a date", "It has been %1 days since you last got new transactions for %2 from your bank (%3). Your bank only lists its recent transactions, so some may be missing if you wait longer.", gap.modelData.days, gap.modelData.account, gap.modelData.lastFetched))
                        actions: [
                            Kirigami.Action {
                                icon.name: "view-refresh"
                                text: names.i18nContext.i18nc("@action:button", "Get new transactions")
                                onTriggered: cell.board.getTransactionsClicked(gap.modelData.accountId)
                            }
                        ]
                    }
                }
            }
        }
    }

    // Transfers are left out of the totals (D-021); the note says so only
    // when the month has one (while arranging, it says what it is).
    Component {
        id: transfersBox

        Controls.Label {
            readonly property bool failed: false
            readonly property int count: cell.board.pageView.transfers ?? 0

            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
            font: Kirigami.Theme.smallFont
            color: count > 0 ? Kirigami.Theme.textColor : Kirigami.Theme.disabledTextColor
            visible: count > 0 || cell.board.editing
            text: count > 0
                ? names.i18nContext.i18ncp("@info", "Money you moved between your accounts (once this month) isn't counted as money in or out.", "Money you moved between your accounts (%1 times this month) isn't counted as money in or out.", count)
                : names.i18nContext.i18nc("@info", "A note appears here when you moved money between your own accounts this month. There is none right now.")
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

    // The toolbar of the selected component only (D-311), and for a period
    // box its tab. A box lies under the widgets it holds, so these are drawn
    // in the grid's top layer: nothing inside a box can cover them (M6-B20).
    Item {
        id: barHost

        parent: cell.isBox ? cell.board.topLayer : cell
        x: cell.isBox ? cell.x : 0
        y: cell.isBox ? cell.y : 0
        width: cell.width
        height: cell.height
        visible: cell.present

        Rectangle {
            id: boxTab

            visible: cell.isBox && cell.board.editing && !cell.board.preview && !cell.selected
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.margins: Kirigami.Units.smallSpacing
            width: Math.min(tabLabel.implicitWidth + Kirigami.Units.largeSpacing * 2, parent.width - Kirigami.Units.smallSpacing * 2)
            height: tabLabel.implicitHeight + Kirigami.Units.smallSpacing * 2
            radius: Kirigami.Units.smallSpacing
            color: Kirigami.Theme.backgroundColor
            border.width: 1
            border.color: Kirigami.Theme.highlightColor

            Controls.Label {
                id: tabLabel

                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.largeSpacing
                text: names.i18nContext.i18nc("@action:button select the period box, to change or move it", "Period box")
                textFormat: Text.PlainText
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignHCenter
            }

            Accessible.role: Accessible.Button
            Accessible.name: names.i18nContext.i18nc("@action:button select the period box, to change or move it", "Period box")
            Accessible.onPressAction: cell.board.select(cell.itemId)

            TapHandler {
                onTapped: cell.board.select(cell.itemId)
            }
        }

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
