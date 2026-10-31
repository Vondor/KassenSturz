// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The board of the Statistic Builder's "Choose categories" step (D-340): a
// column "Unused" that holds every category that is not counted, and any
// number of named columns. A category card is dragged into a column, or put
// there with the card's menu, which also works with the keyboard and a screen
// reader. Each named column becomes one series, bar, slice or row of the chart
// (D-339). The board holds no figures: the preview below it shows them, and a
// category that sits in two columns is explained there.
//
// Every change goes to the workbench as the whole list of columns, so the
// board can never hold something the document could not.

import QtQuick
import QtQml
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: board

    // A `ComponentProperties`.
    required property var workbench
    // A `CategoryGroups`: where the categories and groups to pick from are
    // listed.
    property var choices: null

    // The column a card is being dragged out of, so that its column is drawn
    // on top: -1 for Unused, -2 for none.
    property int dragFrom: -2
    // What went wrong with the last change, in words, or empty.
    property string problem: ""
    // What each column adds up to for the period, as the preview worked it
    // out: a list of {column, amounts}, amounts being one text per currency.
    property var totals: []
    // The columns (by name) the user has turned into a formula and not yet
    // typed one for.
    property var formulaOn: ({})
    // What saving a column as a group answered, and the words for success.
    property var groupResult: ({})
    property string saved: ""

    readonly property var columns: {
        try {
            return JSON.parse(workbench.view.columns ?? "[]");
        } catch (error) {
            return [];
        }
    }
    readonly property var book: choices?.view ?? ({})
    // Every card: the groups of money spent, then the categories of money
    // spent, as their IDs, the words on the card and what they hold.
    readonly property var cards: {
        const groups = (book.groups ?? [])
            .filter(group => !group.income)
            .map(group => ({id: group.id, label: i18nContext.i18nc("@option:check %1 is the name of a group", "%1 (group)", group.name), isGroup: true, path: "", members: (group.members ?? []).map(member => member.id)}));
        const categories = (book.categories ?? [])
            .filter(category => !category.income)
            .map(category => ({id: category.id, label: category.path ?? category.name, isGroup: false, path: category.path ?? category.name, members: []}));
        return groups.concat(categories);
    }
    // The cards that no column counts yet. A category inside a card that is
    // in a column is counted there, so it is not offered again.
    readonly property var unused: cards.filter(card => !counted(card))

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The words for what the column `name` adds up to, or empty when the
    // preview has no figure for it (yet, or it has no money).
    function totalOf(name: string): string {
        const found = totals.find(each => each.column === name);
        return found === undefined ? "" : found.amounts.join(" + ");
    }

    function cardOf(id: string): var {
        return cards.find(card => card.id === id);
    }

    // The words on the card of `id`; a category the book no longer has is
    // still shown, so it can be taken out.
    function labelOf(id: string): string {
        return cardOf(id)?.label ?? i18nContext.i18nc("@item something in a column that is gone from the book", "Something that no longer exists");
    }

    function counted(card): bool {
        return columns.some(column => column.members.some(id => {
            if (id === card.id) {
                return true;
            }
            const holder = cardOf(id);
            if (holder === undefined) {
                return false;
            }
            return holder.isGroup ? holder.members.includes(card.id) : (!card.isGroup && card.path.startsWith(holder.path + " › "));
        }));
    }

    function save(list) {
        board.problem = "";
        board.groupResult = ({});
        board.saved = "";
        workbench.setColumns(JSON.stringify(list));
    }

    function nameTaken(name: string, except: int): bool {
        const wanted = name.trim().toLowerCase();
        return columns.some((column, index) => index !== except && column.name.trim().toLowerCase() === wanted);
    }

    // Checks a name for a column; empty when it is fine.
    function nameProblem(name: string, except: int): string {
        if (name.trim() === "") {
            return i18nContext.i18nc("@info", "Give the column a name first. For example: Fast food.");
        }
        if (nameTaken(name, except)) {
            return i18nContext.i18nc("@info", "Another column already has that name. Choose a different one.");
        }
        return "";
    }

    function addColumn(name: string): bool {
        const wrong = nameProblem(name, -1);
        if (wrong !== "") {
            board.problem = wrong;
            return false;
        }
        const list = JSON.parse(JSON.stringify(columns));
        list.push({name: name.trim(), members: []});
        save(list);
        return true;
    }

    function renameColumn(index: int, name: string) {
        const wrong = nameProblem(name, index);
        if (wrong !== "") {
            board.problem = wrong;
            return;
        }
        const list = JSON.parse(JSON.stringify(columns));
        list[index].name = name.trim();
        save(list);
    }

    // Keeps a column as one of My groups, under its name, so it can be used
    // again in other statistics (D-341). The column stays as it is.
    function saveAsGroup(index: int) {
        board.problem = "";
        board.groupResult = ({});
        board.saved = "";
        const column = columns[index];
        const result = choices.addGroup(column.name, false, column.members.join(","));
        board.groupResult = result;
        if (result.code === undefined) {
            board.saved = i18nContext.i18nc("@info %1 is the name of a column", "Saved “%1” as a group. You find it under My groups.", column.name);
        }
    }

    // Whether the column is worked out by a formula (D-341): it has one, or
    // the user has just asked for one and not typed it yet.
    function isFormula(column): bool {
        return (column.formula ?? "") !== "" || formulaOn[column.name] === true;
    }

    // Turns the formula of a column on or off. Only an empty column can
    // become a formula, so no card is lost; turning it off forgets the
    // formula.
    function toggleFormula(index: int) {
        const column = columns[index];
        const on = !isFormula(column);
        const marks = Object.assign({}, formulaOn);
        if (on) {
            marks[column.name] = true;
        } else {
            delete marks[column.name];
            if ((column.formula ?? "") !== "") {
                setColumnFormula(index, "");
            }
        }
        formulaOn = marks;
    }

    // Keeps what was typed as the formula of the column, in the user's
    // number format; the book keeps it in its own form.
    function setColumnFormula(index: int, typed: string) {
        const list = JSON.parse(JSON.stringify(columns));
        list[index].formula = typed.trim();
        list[index].members = [];
        save(list);
    }

    function removeColumn(index: int) {
        const list = JSON.parse(JSON.stringify(columns));
        list.splice(index, 1);
        save(list);
    }

    // Takes the card out of `from` (-1 is Unused) and puts it in `to`.
    function moveCard(id: string, from: int, to: int) {
        if (from === to) {
            return;
        }
        if (to >= 0 && isFormula(columns[to])) {
            board.problem = i18nContext.i18nc("@info", "A column with a formula can't hold categories. Name them in its formula instead.");
            return;
        }
        const list = JSON.parse(JSON.stringify(columns));
        if (from >= 0) {
            list[from].members = list[from].members.filter(member => member !== id);
        }
        if (to >= 0 && !list[to].members.includes(id)) {
            list[to].members.push(id);
        }
        save(list);
    }

    // Puts the card in `to` as well, where it already is.
    function alsoPut(id: string, to: int) {
        if (isFormula(columns[to])) {
            board.problem = i18nContext.i18nc("@info", "A column with a formula can't hold categories. Name them in its formula instead.");
            return;
        }
        const list = JSON.parse(JSON.stringify(columns));
        if (!list[to].members.includes(id)) {
            list[to].members.push(id);
        }
        save(list);
    }

    // What the card menu offers for a card in `from`.
    function choicesFor(id: string, from: int): var {
        const entries = [];
        if (from >= 0) {
            entries.push({move: true, to: -1, text: i18nContext.i18nc("@action:inmenu take the card out of its column", "Move to Unused")});
        }
        columns.forEach((column, index) => {
            if (index !== from && !isFormula(column)) {
                entries.push({move: true, to: index, text: i18nContext.i18nc("@action:inmenu %1 is the name of a column", "Move to %1", column.name)});
            }
        });
        columns.forEach((column, index) => {
            if (index !== from && !isFormula(column) && !column.members.includes(id) && from >= 0) {
                entries.push({move: false, to: index, text: i18nContext.i18nc("@action:inmenu %1 is the name of a column; the card stays where it is too", "Also put in %1", column.name)});
            }
        });
        // With no column yet there is nowhere to move a card to; say so
        // instead of opening an empty menu.
        if (entries.length === 0) {
            entries.push({hint: true, text: i18nContext.i18nc("@action:inmenu shown when there is no column yet", "Add a column first, then move this card into it")});
        }
        return entries;
    }

    function openMenu(button, id: string, from: int) {
        cardMenu.cardId = id;
        cardMenu.from = from;
        cardMenu.entries = choicesFor(id, from);
        cardMenu.popup(button);
    }

    spacing: Kirigami.Units.largeSpacing

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        text: board.i18nContext.i18nc("@info", "Drag categories into columns, or use the menu on a card. Each column becomes one bar, slice, line or row, and adds up its categories. A category may be in more than one column. What is in Unused is not counted.")
    }

    FormError {
        Layout.fillWidth: true
        result: board.groupResult
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: board.saved !== ""
        type: Kirigami.MessageType.Positive
        text: board.saved
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: board.problem !== ""
        type: Kirigami.MessageType.Warning
        text: board.problem
    }

    Controls.Menu {
        id: cardMenu

        property string cardId: ""
        property int from: -1
        property var entries: []

        Instantiator {
            model: cardMenu.entries

            delegate: Controls.MenuItem {
                required property var modelData

                text: modelData.text
                enabled: modelData.hint !== true
                onTriggered: modelData.move ? board.moveCard(cardMenu.cardId, cardMenu.from, modelData.to) : board.alsoPut(cardMenu.cardId, modelData.to)
            }

            onObjectAdded: (index, object) => cardMenu.insertItem(index, object)
            onObjectRemoved: (index, object) => cardMenu.removeItem(object)
        }
    }

    Flow {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

        // Unused first, then the named columns, then the one that adds a
        // column.
        Repeater {
            model: [{index: -1, name: "", members: board.unused.map(card => card.id), formula: "", asFormula: false}].concat(board.columns.map((column, index) => ({index: index, name: column.name, members: column.members, formula: column.formula ?? "", asFormula: board.isFormula(column)})))

            delegate: Rectangle {
                id: slotColumn

                required property var modelData

                readonly property bool isUnused: modelData.index < 0
                readonly property bool asFormula: !isUnused && modelData.asFormula

                width: Kirigami.Units.gridUnit * 14
                height: Math.max(Kirigami.Units.gridUnit * 12, content.implicitHeight + Kirigami.Units.largeSpacing * 2)
                radius: Kirigami.Units.cornerRadius
                color: isUnused ? Kirigami.Theme.alternateBackgroundColor : Kirigami.Theme.backgroundColor
                border.width: dropHere.containsDrag ? 2 : 1
                border.color: dropHere.containsDrag ? Kirigami.Theme.highlightColor : Kirigami.Theme.disabledTextColor
                z: board.dragFrom === modelData.index ? 10 : 0

                DropArea {
                    id: dropHere

                    anchors.fill: parent
                    keys: ["card"]
                    onDropped: drop => board.moveCard(drop.source.cardId, drop.source.from, slotColumn.modelData.index)
                }

                ColumnLayout {
                    id: content

                    anchors {
                        left: parent.left
                        right: parent.right
                        top: parent.top
                        margins: Kirigami.Units.largeSpacing
                    }
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true

                        Kirigami.Heading {
                            Layout.fillWidth: true
                            visible: slotColumn.isUnused
                            level: 3
                            text: board.i18nContext.i18nc("@title:group the column of categories that are not counted", "Unused")
                            textFormat: Text.PlainText
                        }

                        Controls.TextField {
                            Layout.fillWidth: true
                            visible: !slotColumn.isUnused
                            text: slotColumn.modelData.name
                            maximumLength: 200
                            Accessible.name: board.i18nContext.i18nc("@label:textbox", "Name of the column")
                            onEditingFinished: {
                                if (text !== slotColumn.modelData.name) {
                                    board.renameColumn(slotColumn.modelData.index, text);
                                    text = Qt.binding(() => slotColumn.modelData.name);
                                }
                            }
                        }

                        Controls.ToolButton {
                            visible: !slotColumn.isUnused && (slotColumn.asFormula || slotColumn.modelData.members.length === 0)
                            checkable: true
                            checked: slotColumn.asFormula
                            icon.name: "view-calculator"
                            text: board.i18nContext.i18nc("@action:button %1 is the name of a column", "Work out the column %1 with a formula", slotColumn.modelData.name)
                            display: Controls.AbstractButton.IconOnly
                            Controls.ToolTip.text: text
                            Controls.ToolTip.visible: hovered
                            onClicked: board.toggleFormula(slotColumn.modelData.index)
                        }

                        Controls.ToolButton {
                            visible: !slotColumn.isUnused && slotColumn.modelData.members.length > 0
                            icon.name: "folder-new"
                            text: board.i18nContext.i18nc("@action:button %1 is the name of a column", "Save the column %1 as a group", slotColumn.modelData.name)
                            display: Controls.AbstractButton.IconOnly
                            Controls.ToolTip.text: text
                            Controls.ToolTip.visible: hovered
                            onClicked: board.saveAsGroup(slotColumn.modelData.index)
                        }

                        Controls.ToolButton {
                            visible: !slotColumn.isUnused
                            icon.name: "edit-delete"
                            text: board.i18nContext.i18nc("@action:button %1 is the name of a column", "Remove the column %1", slotColumn.modelData.name)
                            display: Controls.AbstractButton.IconOnly
                            Controls.ToolTip.text: text
                            Controls.ToolTip.visible: hovered
                            onClicked: board.removeColumn(slotColumn.modelData.index)
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        font: Kirigami.Theme.smallFont
                        opacity: 0.7
                        text: slotColumn.isUnused
                            ? board.i18nContext.i18nc("@info", "Not counted. A category you add under Categories shows up here.")
                            : slotColumn.asFormula && board.totalOf(slotColumn.modelData.name) === ""
                                ? board.i18nContext.i18nc("@info", "Worked out from a formula. Use the names of columns, categories and groups, like Home + Eating.")
                            : board.totalOf(slotColumn.modelData.name) !== ""
                                ? board.i18nContext.i18nc("@info %1 is an amount of money, or several with + between them", "Adds up to %1", board.totalOf(slotColumn.modelData.name))
                                : board.i18nContext.i18nc("@info", "Counted together as one figure.")
                    }

                    Controls.TextField {
                        Layout.fillWidth: true
                        visible: slotColumn.asFormula
                        text: slotColumn.modelData.formula ?? ""
                        placeholderText: board.i18nContext.i18nc("@info:placeholder an example of a formula that uses the names of two columns", "For example: Home + Eating")
                        maximumLength: 500
                        Accessible.name: board.i18nContext.i18nc("@label:textbox", "Formula of the column")
                        onEditingFinished: {
                            if (text.trim() !== (slotColumn.modelData.formula ?? "")) {
                                board.setColumnFormula(slotColumn.modelData.index, text);
                            }
                        }
                    }

                    Repeater {
                        model: slotColumn.modelData.members

                        delegate: Item {
                            id: slot

                            required property string modelData
                            // What the drop reads.
                            readonly property string cardId: modelData
                            readonly property int from: slotColumn.modelData.index

                            Layout.fillWidth: true
                            Layout.preferredHeight: tile.height

                            Rectangle {
                                id: tile

                                width: slot.width
                                height: Kirigami.Units.gridUnit * 2
                                radius: Kirigami.Units.cornerRadius
                                color: Kirigami.Theme.backgroundColor
                                border.width: 1
                                border.color: handler.active ? Kirigami.Theme.highlightColor : Kirigami.Theme.disabledTextColor
                                z: handler.active ? 100 : 0
                                Drag.dragType: Drag.None
                                Drag.keys: ["card"]
                                Drag.source: slot
                                Drag.hotSpot.x: width / 2
                                Drag.hotSpot.y: height / 2

                                DragHandler {
                                    id: handler

                                    onActiveChanged: {
                                        if (active) {
                                            board.dragFrom = slot.from;
                                            tile.Drag.start();
                                        } else {
                                            tile.Drag.drop();
                                            tile.x = 0;
                                            tile.y = 0;
                                            board.dragFrom = -2;
                                        }
                                    }
                                }

                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: Kirigami.Units.smallSpacing

                                    Controls.Label {
                                        Layout.fillWidth: true
                                        elide: Text.ElideRight
                                        textFormat: Text.PlainText
                                        text: board.labelOf(slot.modelData)
                                        opacity: board.cardOf(slot.modelData) === undefined ? 0.6 : 1
                                    }

                                    Controls.ToolButton {
                                        id: menuButton

                                        icon.name: "overflow-menu"
                                        text: board.i18nContext.i18nc("@action:button %1 is the name of a category; opens a menu to put it in a column", "Move %1 to…", board.labelOf(slot.modelData))
                                        display: Controls.AbstractButton.IconOnly
                                        Controls.ToolTip.text: text
                                        Controls.ToolTip.visible: hovered
                                        onClicked: board.openMenu(menuButton, slot.modelData, slot.from)
                                    }
                                }
                            }
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: slotColumn.modelData.members.length === 0 && !slotColumn.asFormula
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        opacity: 0.6
                        text: slotColumn.isUnused
                            ? board.i18nContext.i18nc("@info", "Everything is in a column.")
                            : board.i18nContext.i18nc("@info", "Drag categories here")
                    }
                }
            }
        }

        // Adds a column.
        Rectangle {
            width: Kirigami.Units.gridUnit * 14
            height: Kirigami.Units.gridUnit * 12
            radius: Kirigami.Units.cornerRadius
            color: Kirigami.Theme.backgroundColor
            border.width: 1
            border.color: Kirigami.Theme.highlightColor

            ColumnLayout {
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                    margins: Kirigami.Units.largeSpacing
                }
                spacing: Kirigami.Units.smallSpacing

                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 3
                    text: board.i18nContext.i18nc("@title:group a place to add a column", "Add a column")
                    textFormat: Text.PlainText
                }

                Controls.TextField {
                    id: newName

                    Layout.fillWidth: true
                    maximumLength: 200
                    placeholderText: board.i18nContext.i18nc("@info:placeholder an example of a column name", "For example: Fast food")
                    Accessible.name: board.i18nContext.i18nc("@label:textbox", "Name of the new column")
                    onAccepted: addButton.clicked()
                }

                Controls.Button {
                    id: addButton

                    text: board.i18nContext.i18nc("@action:button", "Add column")
                    icon.name: "list-add"
                    onClicked: {
                        if (board.addColumn(newName.text)) {
                            newName.text = "";
                        }
                    }
                }

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    font: Kirigami.Theme.smallFont
                    opacity: 0.7
                    text: board.i18nContext.i18nc("@info", "Name it, then drag categories in. For example, put Restaurants and Coffee into a column called Fast food.")
                }
            }
        }
    }
}
