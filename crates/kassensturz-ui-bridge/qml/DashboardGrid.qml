// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Overview's dashboard on its 12-column grid (M6b.3, D-287, D-288,
// D-311), in view mode and in edit mode. A column is the width less the gaps
// divided by 12 and a row is `gridUnit * 4`, so nothing is a fixed number of
// pixels. When a column would be narrower than 3 grid units the components are
// shown in one column in reading order, each as high as its frame; the stored
// frames do not change by looking at them in a narrow window.
//
// What may stand where is decided by the editor in Rust (`DashboardLayout`):
// this file draws what it is given and passes the user's edits on. A refused
// edit changes nothing and is reported through `failed`.
//
// Keys in edit mode (every mouse action has one): Tab and Shift+Tab go to the
// next and previous component in reading order, the arrow keys move it by one
// cell, Shift and the arrow keys make it larger or smaller, Enter opens its
// properties, Delete removes it, Ctrl+D copies it and Escape stops a drag.

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Item {
    id: grid

    // A `DashboardLayout`.
    required property var layout
    // The month the Overview shows; a component without a period of its own
    // asks for it.
    property string monthKey: ""
    // Edit mode without the grid lines and the toolbar.
    property bool preview: false

    readonly property var view: layout.view ?? ({})
    readonly property var items: view.items ?? []
    readonly property bool editing: view.editing ?? false
    readonly property int columns: view.columns ?? 12
    readonly property real gap: Kirigami.Units.largeSpacing
    readonly property real rowHeight: Kirigami.Units.gridUnit * 4
    readonly property bool narrow: width > 0 && layout.narrow(Math.round(width), Math.round(gap), Kirigami.Units.gridUnit)
    readonly property real cell: Math.max(0, (width - gap * (columns - 1)) / columns)
    // Rows below the lowest component, so a component can be dropped there.
    readonly property int shownRows: (view.rows ?? 0) + (editing && !narrow ? 2 : 0)

    // The components by ID, and where each starts in a narrow window.
    readonly property var byId: {
        const found = ({});
        for (const entry of items) {
            found[entry.id] = entry;
        }
        return found;
    }
    readonly property var offsets: {
        const found = ({});
        let top = 0;
        for (const entry of items) {
            found[entry.id] = top;
            if (!isCollapsed(entry)) {
                top += frameHeight(entry.rows) + gap;
            }
        }
        found._end = Math.max(0, top - gap);
        return found;
    }
    // How many budgets the book has (the overview's page view); the budget
    // bars take no room in view mode while there is none (D-357, Q-264).
    readonly property int budgetCount: pageView.budgets ?? 0

    property string selectedId: ""
    // The cells a drag would end on, or null.
    property var ghost: null
    // The IDs of components that have a problem.
    property var problemIds: []
    readonly property int problemCount: problemIds.length
    // The IDs the Repeater keeps one component for, in alphabetical order, so
    // a component keeps its item (and its animation and focus) when it moves.
    property var ids: []
    // Where what must not be covered by a widget is drawn.
    readonly property alias topLayer: topLayerItem

    // The app's `BankConnection` and whether bank connections can be used
    // (D-054), for the Update all accounts widget (M6d.7); null and false
    // where the grid has no bank, which hides the widget.
    property var bank: null
    property bool bankReady: false
    // What the widget shows before it is pressed, asked again whenever the
    // page's own figures are (after an import, a link, an undo).
    readonly property var updateAll: {
        void pageView;
        const state = bankReady && bank ? bank.updateAllState() : ({});
        return state.code === undefined ? state : ({});
    }
    readonly property bool updateAllShown: bankReady && (updateAll.linked ?? 0) > 0

    // What the page knows that is not a figure of the report layer: the
    // overview's fetch-gap warnings and its count of transfers, for the
    // notice widgets (D-328).
    property var pageView: ({})
    // The `BookOverview` the period header widget steps through months with,
    // and the `CategoryGroups` the filter bar widget picks from (M6d.8).
    property var overview: null
    property var choices: null

    // The part of the grid the user can see, in the grid's own coordinates,
    // told by the page that scrolls it (M6-B30). Without a page around it
    // the whole grid counts as seen.
    property real viewTop: 0
    property real viewBottom: Number.MAX_VALUE
    // How many components, in reading order, have been given their turn to
    // make their content. A component's frame is drawn at once; its content
    // follows one component at a time, so the page opens at once and fills
    // in while the user already looks at it (M6-B30).
    property int revealed: 0
    // Each component's place in reading order, by ID.
    readonly property var readingIndex: {
        const found = ({});
        items.map((entry, index) => ({id: entry.id, row: entry.row, col: entry.col, index: index}))
            .sort((a, b) => a.row - b.row || a.col - b.col || a.index - b.index)
            .forEach((entry, place) => { found[entry.id] = place; });
        return found;
    }

    signal propertiesRequested(string id)
    signal getTransactionsClicked(string accountId)
    signal setBudgetClicked()
    // A notice of Update all accounts was clicked (M6d.7).
    signal transactionClicked(string accountId, string transactionId)
    // An edit that was refused: the error object (`code`, ...).
    signal failed(var result)

    function cellWidth(cols: int): real {
        return Math.max(0, cols * cell + (cols - 1) * gap);
    }

    function frameHeight(rows: int): real {
        return Math.max(0, rows * rowHeight + (rows - 1) * gap);
    }

    // A component that shows nothing and takes no room: the budget bars of
    // a book with no budget, and the Update all accounts button while no
    // account is linked or bank connections can't be used (D-446), while the
    // page is only looked at. Arranging the layout shows it, so it can still
    // be moved and removed.
    function isCollapsed(info: var): bool {
        return !editing && ((info.type === "budget.progress" && budgetCount === 0)
            || (info.type === "bank.update_all" && !updateAllShown));
    }

    // The height of the full-width components that take no room and lie
    // completely above `row`: what the components below move up by.
    function collapsedAbove(row: int): real {
        let removed = 0;
        for (const entry of items) {
            if (isCollapsed(entry) && entry.cols === columns && entry.row + entry.rows <= row) {
                removed += entry.rows * (rowHeight + gap);
            }
        }
        return removed;
    }

    function frameX(info: var): real {
        return narrow ? 0 : info.col * (cell + gap);
    }

    function frameY(info: var): real {
        return narrow ? (offsets[info.id] ?? 0) : info.row * (rowHeight + gap) - collapsedAbove(info.row);
    }

    function frameWidth(info: var): real {
        return narrow ? width : cellWidth(info.cols);
    }

    // The period box a widget lies completely inside, if any (D-333). Boxes
    // never overlap each other, so there is at most one.
    function boxFor(info: var): var {
        if (!info || info.type === "group.period") {
            return null;
        }
        const inside = box => box.type === "group.period"
            && box.col <= info.col && box.row <= info.row
            && info.col + info.cols <= box.col + box.cols
            && info.row + info.rows <= box.row + box.rows;
        return items.find(inside) ?? null;
    }

    // The period a component asks for. The report layer works it out (D-350):
    // its own, else the period box it lies in, else the filter bar's, else the
    // page's month. `info.asked.period` is `page` for the last one.
    function periodFor(info: var): string {
        const word = info?.asked?.period;
        return word === undefined || word === "page" ? monthKey : word;
    }

    // The same for a period word on its own (the properties panel shows the
    // component with the period it is being given); `info` is the widget it
    // is for. A word of `page` follows the box, the bar and the page as the
    // component would.
    function periodOf(word: var, info: var): string {
        if (word !== undefined && word !== "page") {
            return word;
        }
        const boxWord = boxFor(info)?.subject?.period?.word;
        if (boxWord !== undefined && boxWord !== "page") {
            return boxWord;
        }
        const barWord = view.filters?.period ?? "";
        return barWord !== "" ? barWord : monthKey;
    }

    // Whether a component should have its content made now (M6-B30): the
    // page is shown, its turn has come, and it is in or near the part of the
    // grid the user sees. Arranging the layout makes all of them, so every
    // problem is found. A component keeps its content once it has it.
    function contentDue(info: var): bool {
        if (!visible) {
            return false;
        }
        if (revealed <= (readingIndex[info.id] ?? 0)) {
            return false;
        }
        if (editing) {
            return true;
        }
        const margin = rowHeight * 3;
        const top = frameY(info);
        return top + frameHeight(info.rows) >= viewTop - margin && top <= viewBottom + margin;
    }

    function itemFor(id: string): var {
        for (let index = 0; index < repeater.count; ++index) {
            const found = repeater.itemAt(index);
            if (found && found.itemId === id) {
                return found;
            }
        }
        return null;
    }

    function select(id: string) {
        selectedId = id;
        const found = itemFor(id);
        if (found) {
            found.focusIt();
        }
    }

    // Takes the answer of an edit: reports a refusal.
    function edit(result: var): var {
        if (result.code !== undefined) {
            failed(result);
        }
        return result;
    }

    // `dataset` is empty for the kind's first dataset (M6c.2).
    function add(kind: string, dataset: var) {
        const result = edit(dataset === undefined || dataset === "" ? layout.add(kind) : layout.addWith(kind, dataset));
        if (result.id !== undefined) {
            // The new item exists after the view has changed.
            Qt.callLater(() => grid.select(result.id));
        }
    }

    function duplicate(id: string) {
        const result = edit(layout.duplicate(id));
        if (result.id !== undefined) {
            Qt.callLater(() => grid.select(result.id));
        }
    }

    function remove(id: string) {
        selectedId = "";
        edit(layout.remove(id));
    }

    function indexOfItem(id: string): int {
        return items.findIndex(entry => entry.id === id);
    }

    function focusNeighbour(id: string, step: int, event: var) {
        const next = items[indexOfItem(id) + step];
        if (next === undefined) {
            event.accepted = false;
            return;
        }
        select(next.id);
        event.accepted = true;
    }

    function keyPressed(item: var, event: var) {
        if (!editing || preview) {
            return;
        }
        const id = item.itemId;
        const shift = (event.modifiers & Qt.ShiftModifier) !== 0;
        const ctrl = (event.modifiers & Qt.ControlModifier) !== 0;
        const step = (cols, rows) => shift ? layout.grow(id, cols, rows) : layout.nudge(id, cols, rows);
        switch (event.key) {
        case Qt.Key_Left:
            narrow ? undefined : edit(step(-1, 0));
            break;
        case Qt.Key_Right:
            narrow ? undefined : edit(step(1, 0));
            break;
        case Qt.Key_Up:
            narrow ? edit(layout.moveEarlier(id)) : edit(step(0, -1));
            break;
        case Qt.Key_Down:
            narrow ? edit(layout.moveLater(id)) : edit(step(0, 1));
            break;
        case Qt.Key_Return:
        case Qt.Key_Enter:
            propertiesRequested(id);
            break;
        case Qt.Key_Delete:
            remove(id);
            break;
        case Qt.Key_D:
            if (!ctrl) {
                return;
            }
            duplicate(id);
            break;
        case Qt.Key_Escape:
            item.stopDragging();
            break;
        default:
            return;
        }
        event.accepted = true;
    }

    function countProblems() {
        Qt.callLater(grid.recount);
    }

    function recount() {
        const found = [];
        for (let index = 0; index < repeater.count; ++index) {
            const entry = repeater.itemAt(index);
            if (entry && entry.present && entry.hasProblem) {
                found.push(entry.itemId);
            }
        }
        // In reading order, so "Show" leads to the first.
        found.sort((a, b) => indexOfItem(a) - indexOfItem(b));
        if (JSON.stringify(found) !== JSON.stringify(problemIds)) {
            problemIds = found;
        }
    }

    function showProblem() {
        if (problemIds.length > 0) {
            select(problemIds[0]);
        }
    }

    onItemsChanged: {
        const next = items.map(entry => entry.id).sort();
        if (JSON.stringify(next) !== JSON.stringify(ids)) {
            ids = next;
        }
        if (selectedId !== "" && byId[selectedId] === undefined) {
            selectedId = "";
        }
        countProblems();
    }

    onEditingChanged: {
        if (!editing) {
            selectedId = "";
            preview = false;
        }
    }

    implicitHeight: narrow ? (offsets._end ?? 0) + (editing ? frameHeight(1) : 0) : Math.max(0, frameHeight(shownRows) - collapsedAbove(shownRows + 1))

    Accessible.role: Accessible.Grouping

    // The grid lines, in edit mode.
    Item {
        anchors.fill: parent
        visible: grid.editing && !grid.preview && !grid.narrow

        Repeater {
            model: parent.visible ? grid.columns : 0

            Rectangle {
                required property int index

                x: index * (grid.cell + grid.gap)
                width: grid.cell
                height: grid.height
                color: Qt.alpha(Kirigami.Theme.highlightColor, 0.07)
            }
        }

        Repeater {
            model: parent.visible ? grid.shownRows : 0

            Rectangle {
                required property int index

                y: index * (grid.rowHeight + grid.gap)
                width: grid.width
                height: grid.rowHeight
                color: Qt.alpha(Kirigami.Theme.highlightColor, 0.05)
            }
        }
    }

    // Where a component being dragged or resized would end.
    Rectangle {
        visible: grid.ghost !== null
        x: grid.ghost ? grid.ghost.col * (grid.cell + grid.gap) : 0
        y: grid.ghost ? grid.ghost.row * (grid.rowHeight + grid.gap) : 0
        width: grid.ghost ? grid.cellWidth(grid.ghost.cols) : 0
        height: grid.ghost ? grid.frameHeight(grid.ghost.rows) : 0
        color: Qt.alpha(Kirigami.Theme.highlightColor, 0.15)
        border.width: 2
        border.color: Kirigami.Theme.highlightColor
        radius: Kirigami.Units.smallSpacing
    }

    // One component's turn at a time, with the event loop free in between,
    // so the first picture of the page is not held back by the figures.
    Timer {
        interval: 10
        repeat: true
        running: grid.visible && grid.revealed < grid.items.length
        onTriggered: ++grid.revealed
    }

    Repeater {
        id: repeater

        model: grid.ids

        delegate: GridItem {
            required property string modelData

            board: grid
            itemId: modelData
        }
    }

    // Over everything: the toolbar and tab of a period box, which lies under
    // the widgets it holds and must stay reachable (M6-B20).
    Item {
        id: topLayerItem

        z: 10
        anchors.fill: parent
    }
}
