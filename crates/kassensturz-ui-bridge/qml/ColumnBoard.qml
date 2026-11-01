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
import io.github.vondor.kassensturz

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
    // The column that was just made from a card: its name is asked for at
    // once, so its field takes the focus.
    property string focusName: ""
    // What saving a column as a group answered, and the words for success.
    property var groupResult: ({})
    property string saved: ""
    // The parents the user split into subcategory columns: money booked
    // straight to one has no column, which the board says (the report layer
    // counts a category with everything below it, so it cannot be a column).
    property var splitParents: []
    readonly property var leftOut: splitParents.map(id => cardOf(id)).filter(card => card !== undefined && !counted(card)).map(card => card.label)

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

    // The card at the top of Unused that makes a formula column. It is not a
    // category and is never used up: it can be dragged again and again.
    readonly property string formulaCard: "@formula"

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The words for what the column `name` adds up to, or empty when the
    // preview has no figure for it (yet, or it has no money).
    function totalOf(name: string): string {
        const found = totals.find(each => each.column === name);
        return found === undefined ? "" : found.amounts.join(" + ");
    }

    // Whether the preview worked this column out and found no money in it
    // (M6-B15): it is in the totals with no amount, unlike a column the
    // preview has not seen yet.
    function hasNoSpending(name: string): bool {
        const found = totals.find(each => each.column === name);
        return found !== undefined && found.amounts.length === 0;
    }

    function cardOf(id: string): var {
        return cards.find(card => card.id === id);
    }

    // The words on the card of `id`; a category the book no longer has is
    // still shown, so it can be taken out.
    function labelOf(id: string): string {
        if (id === formulaCard) {
            return i18nContext.i18nc("@item a card that makes a column worked out by a formula", "Formula");
        }
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

    // A name for a column made from a card: the one typed in the Add a
    // column box when it is free, otherwise "New column", "New column 2", ...
    function freeName(wanted: string): string {
        if (wanted.trim() !== "" && !nameTaken(wanted, -1)) {
            return wanted.trim();
        }
        let name = i18nContext.i18nc("@label the name given to a column made from a card, to be changed", "New column");
        for (let number = 2; nameTaken(name, -1); ++number) {
            name = i18nContext.i18nc("@label %1 is a number; the name given to another column made from a card", "New column %1", number);
        }
        return name;
    }

    // Makes a column that holds the card, taking it out of `from` (-1 is
    // Unused), and asks for the column's name.
    function moveToNew(id: string, from: int, wanted: string): string {
        const name = freeName(wanted);
        const list = JSON.parse(JSON.stringify(columns));
        if (id === formulaCard) {
            // A new column that is worked out by a formula.
            const marks = Object.assign({}, formulaOn);
            marks[name] = true;
            formulaOn = marks;
            list.push({name: name, members: []});
            focusName = name;
            save(list);
            return name;
        }
        if (from >= 0) {
            list[from].members = list[from].members.filter(member => member !== id);
        }
        list.push({name: name, members: [id]});
        focusName = name;
        save(list);
        return name;
    }

    // Takes the card out of `from` (-1 is Unused) and puts it in `to`.
    function moveCard(id: string, from: int, to: int) {
        if (from === to) {
            return;
        }
        if (id === formulaCard) {
            formulaInto(to);
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

    // Puts the card in `to` and leaves it where it is: a drag with Ctrl held
    // (-1 is Unused; from Unused it is the same as a move).
    function copyCard(id: string, from: int, to: int) {
        if (from === to) {
            return;
        }
        if (id === formulaCard) {
            formulaInto(to);
            return;
        }
        alsoPut(id, to);
    }

    // Puts the card in `to` as well, where it already is.
    function alsoPut(id: string, to: int) {
        if (id === formulaCard) {
            formulaInto(to);
            return;
        }
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

    // The words the formula field of a column offers while the user types
    // (M6b Phase 2): the names of other columns, of categories and groups, and
    // the functions, each with one plain line.
    BookFormula {
        id: formulaBook
    }

    function quoted(name: string): string {
        return /^[A-Za-z0-9_]+$/.test(name) ? name : "\"" + name.replace(/"/g, "") + "\"";
    }

    // One plain line, with an example, for each function offered.
    function functionHelp(): var {
        return [
            {name: "SUM", text: i18nContext.i18nc("@info a function of formulas, then an example", "Adds the figures up. Example: SUM(Food; Habitat)")},
            {name: "AVERAGE", text: i18nContext.i18nc("@info a function of formulas, then an example", "The sum divided by how many figures there are. Example: AVERAGE(Food; Habitat)")},
            {name: "PERCENT_OF", text: i18nContext.i18nc("@info a function of formulas, then an example", "What share one figure is of another. Example: PERCENT_OF(Food; Food + Habitat)")},
            {name: "CHANGE", text: i18nContext.i18nc("@info a function of formulas, then an example", "How much a figure changed compared with an older one. Example: CHANGE(Food; Habitat)")},
            {name: "ROUND", text: i18nContext.i18nc("@info a function of formulas, then an example", "Rounds to that many decimal places. Example: ROUND(Food / 3; 2)")},
            {name: "IFERROR", text: i18nContext.i18nc("@info a function of formulas, then an example", "Uses a fallback when the first figure can't be worked out. Example: IFERROR(Food / Habitat; 0)")}
        ];
    }

    // A category or group as a formula writes it: bare when it is one word,
    // otherwise in single quotes with a quote inside doubled.
    function writtenName(name: string): string {
        return /^[A-Za-z_][A-Za-z_0-9]*$/.test(name) ? name : "'" + name.replace(/'/g, "''") + "'";
    }

    // Where the formula says what comes next (right after an opening
    // bracket, a `;`, a `,` or an operator, or at its start), no word is typed
    // yet: offer the names to use and, inside a call, the call's syntax
    // (M6-B23).
    function argumentSuggestions(index: int, before: string, cursor: int): var {
        const last = before.replace(/\s+$/, "").slice(-1);
        if (before.trim() !== "" && !["(", ";", ",", "+", "-", "*", "/"].includes(last)) {
            return [];
        }
        const found = [];
        const call = enclosingCall(before);
        if (call !== "") {
            const entry = functionHelp().find(help => help.name === call);
            if (entry) {
                found.push({label: call + "(…)", detail: entry.text, start: cursor, end: cursor, insert: ""});
            }
        }
        columns.forEach((column, other) => {
            if (other !== index && column.name.trim() !== "") {
                found.push({label: column.name, detail: i18nContext.i18nc("@item a column of the board, in the list of suggestions", "column"), start: cursor, end: cursor, insert: quoted(column.name)});
            }
        });
        cards.forEach(card => found.push({
            label: card.isGroup ? card.label : card.path,
            detail: card.isGroup ? i18nContext.i18nc("@item a group, in the list of suggestions", "group") : i18nContext.i18nc("@item a category, in the list of suggestions", "category"),
            start: cursor, end: cursor, insert: writtenName(card.isGroup ? card.label.replace(/ \(group\)$/, "") : card.path)
        }));
        return found.slice(0, 8);
    }

    // The name of the function whose brackets are still open at the end of
    // `before`, or "".
    function enclosingCall(before: string): string {
        const text = before.replace(/"[^"]*"/g, "\"\"");
        let depth = 0;
        for (let at = text.length - 1; at >= 0; --at) {
            if (text[at] === ")") {
                ++depth;
            } else if (text[at] === "(") {
                if (depth === 0) {
                    return (/([A-Za-z_]+)\s*$/.exec(text.slice(0, at)) ?? ["", ""])[1].toUpperCase();
                }
                --depth;
            }
        }
        return "";
    }

    // Every match of a global expression, as lists (the engine's own
    // `matchAll` is left out so older Qt versions work too).
    function matches(text: string, pattern: var): var {
        const found = [];
        pattern.lastIndex = 0;
        let hit = pattern.exec(text);
        while (hit !== null) {
            found.push(hit);
            hit = pattern.exec(text);
        }
        return found;
    }

    // What is wrong with a formula as typed, in words, or "": a bracket left
    // open or closed too often, a name that is none of the columns,
    // categories or groups, a function that doesn't exist (M6-B23).
    function formulaProblem(index: int, text: string): string {
        const plain = text.replace(/"[^"]*"/g, "\"\"").replace(/'(?:[^']|'')*'/g, "''").replace(/@\S*/g, "");
        let depth = 0;
        for (const c of plain) {
            if (c === "(") {
                ++depth;
            } else if (c === ")" && --depth < 0) {
                return i18nContext.i18nc("@info a mistake in a formula", "There is a ) too many. Take it out, or add a ( before it.");
            }
        }
        if (depth > 0) {
            return i18nContext.i18nc("@info a mistake in a formula", "A ) is missing at the end. Add it to close the bracket.");
        }
        const known = functionHelp().map(entry => entry.name);
        const names = columns.filter((column, other) => other !== index).map(column => column.name.toLowerCase())
            .concat(cards.map(card => (card.isGroup ? card.label : card.path).toLowerCase()));
        for (const found of matches(plain, /[A-Za-z_][A-Za-z_0-9]*(\s*\()?/g)) {
            const word = found[0].replace(/\s*\($/, "");
            if (found[1] !== undefined) {
                if (!known.includes(word.toUpperCase())) {
                    return i18nContext.i18nc("@info a mistake in a formula; %1 is what was typed", "“%1” is not a function. The functions are %2.", word, known.join(", "));
                }
            } else if (!names.some(name => name.includes(word.toLowerCase()))) {
                return i18nContext.i18nc("@info a mistake in a formula; %1 is what was typed", "“%1” is not the name of a column, category or group. Pick one from the suggestions.", word);
            }
        }
        // Names written in quotes must exist too.
        for (const found of matches(text, /"([^"]+)"|'((?:[^']|'')+)'/g)) {
            const typed = found[1] ?? found[2].replace(/''/g, "'");
            const word = typed.toLowerCase();
            if (!names.some(name => name.includes(word))) {
                return i18nContext.i18nc("@info a mistake in a formula; %1 is what was typed", "“%1” is not the name of a column, category or group. Pick one from the suggestions.", typed);
            }
        }
        return "";
    }

    // The suggestions for the word at the cursor in the formula of column
    // `index`: {label, detail, start, end, insert}.
    function suggest(index: int, text: string, cursor: int): var {
        const before = text.slice(0, cursor);
        const word = (/[^\s+\-*\/();,"]*$/.exec(before) ?? [""])[0];
        if (word === "") {
            return argumentSuggestions(index, before, cursor);
        }
        const start = cursor - word.length;
        const lower = word.toLowerCase();
        const found = [];
        columns.forEach((column, other) => {
            if (other !== index && column.name.toLowerCase().includes(lower)) {
                found.push({label: column.name, detail: i18nContext.i18nc("@item a column of the board, in the list of suggestions", "column"), start: start, end: cursor, insert: quoted(column.name)});
            }
        });
        const completion = formulaBook.complete(text, cursor);
        (completion.names ?? []).forEach(name => found.push({
            label: name.label,
            detail: name.group ? i18nContext.i18nc("@item a group, in the list of suggestions", "group") : i18nContext.i18nc("@item a category, in the list of suggestions", "category"),
            start: completion.start, end: completion.end, insert: name.insert
        }));
        functionHelp().forEach(entry => {
            if (entry.name.startsWith(word.toUpperCase())) {
                found.push({label: entry.name + "(…)", detail: entry.text, start: start, end: cursor, insert: entry.name + "("});
            }
        });
        return found.slice(0, 8);
    }

    // Ready-made formulas made from the other columns, to start from.
    function examples(index: int): var {
        const names = columns.filter((column, other) => other !== index && !isFormula(column)).map(column => quoted(column.name));
        if (names.length < 2) {
            return [];
        }
        const [first, second] = names;
        return [
            {text: first + " - " + second, detail: i18nContext.i18nc("@info a ready-made formula", "The difference between two columns")},
            {text: first + " + " + second, detail: i18nContext.i18nc("@info a ready-made formula", "Two columns added together")},
            {text: "PERCENT_OF(" + first + "; " + first + " + " + second + ")", detail: i18nContext.i18nc("@info a ready-made formula", "The share of the first column in both together")}
        ];
    }

    // The Formula card dropped on a column: an empty column becomes a
    // formula column. One that holds categories is refused, so none are lost.
    function formulaInto(to: int) {
        if (to < 0) {
            return;
        }
        const column = columns[to];
        if (isFormula(column)) {
            return;
        }
        if (column.members.length > 0) {
            board.problem = i18nContext.i18nc("@info", "Only an empty column can be worked out by a formula. Take its categories out first, or drop the Formula card on Add a column.");
            return;
        }
        board.problem = "";
        toggleFormula(to);
    }

    // The categories directly under a category card: the fastest way to a
    // standard pie of one parent (M6b Phase 2).
    function childrenOf(id: string): var {
        const parent = cardOf(id);
        if (parent === undefined || parent.isGroup) {
            return [];
        }
        const prefix = parent.path + " › ";
        return cards.filter(card => !card.isGroup && card.path.startsWith(prefix) && !card.path.slice(prefix.length).includes(" › "));
    }

    // Makes one column for each subcategory of the card, named after it, and
    // takes the card itself out of `from`. A subcategory's own subcategories
    // are counted with it. Money booked straight to the parent has no column,
    // as the parent card counts everything below it.
    function splitCard(id: string, from: int) {
        const children = childrenOf(id);
        if (children.length === 0) {
            return;
        }
        const list = JSON.parse(JSON.stringify(columns));
        if (from >= 0) {
            list[from].members = list[from].members.filter(member => member !== id);
        }
        children.forEach(child => {
            let name = child.path.split(" › ").pop();
            for (let number = 2; list.some(column => column.name.trim().toLowerCase() === name.trim().toLowerCase()); ++number) {
                name = child.path.split(" › ").pop() + " " + number;
            }
            list.push({name: name, members: [child.id]});
        });
        if (!splitParents.includes(id)) {
            splitParents = splitParents.concat([id]);
        }
        save(list);
    }

    // What the card menu offers for a card in `from`.
    function choicesFor(id: string, from: int): var {
        const entries = [];
        if (id === formulaCard) {
            columns.forEach((column, index) => {
                if (!isFormula(column) && column.members.length === 0) {
                    entries.push({move: true, to: index, text: i18nContext.i18nc("@action:inmenu %1 is the name of an empty column", "Work out %1 with a formula", column.name)});
                }
            });
            entries.push({create: true, text: i18nContext.i18nc("@action:inmenu starts a new column that is worked out by a formula", "Make a new formula column")});
            return entries;
        }
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
        if (childrenOf(id).length > 0) {
            entries.push({split: true, text: i18nContext.i18nc("@action:inmenu makes a column for each subcategory of the card", "Make a column for each subcategory")});
        }
        // A card can always start a column of its own, so the menu is never
        // empty, also while there is no column yet.
        entries.push({create: true, text: i18nContext.i18nc("@action:inmenu starts a new column with the card", "Move to a new column")});
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

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
        opacity: 0.7
        text: board.i18nContext.i18nc("@info", "To count a category in more than one column, hold Ctrl while you drag it, or choose Also put in … in its menu.")
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: board.leftOut.length > 0
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: Kirigami.Theme.neutralTextColor
        text: board.i18nContext.i18nc("@info %1 is a list of category names", "Money booked straight to %1 is not in any column. Only its subcategories are counted. Put the category itself in a column to count everything.", board.leftOut.join(", "))
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
                onTriggered: {
                    ActionLog.taken("ColumnBoard_triggered")
                    modelData.split ? board.splitCard(cardMenu.cardId, cardMenu.from) : modelData.create ? board.moveToNew(cardMenu.cardId, cardMenu.from, "") : modelData.move ? board.moveCard(cardMenu.cardId, cardMenu.from, modelData.to) : board.alsoPut(cardMenu.cardId, modelData.to)
                }
            }

            onObjectAdded: (index, object) => cardMenu.insertItem(index, object)
            onObjectRemoved: (index, object) => cardMenu.removeItem(object)
        }
    }

    // One place for categories: the list of what is not counted, or a column.
    Component {
        id: slotDelegate

        Rectangle {
            id: slotColumn

            required property var modelData

            readonly property bool isUnused: modelData.index < 0
            readonly property bool asFormula: !isUnused && modelData.asFormula

            // In the row of the list of what is not counted; the columns sit in
            // a Flow, which ignores these.
            Layout.alignment: Qt.AlignTop
            Layout.preferredWidth: width
            Layout.preferredHeight: height
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
                onDropped: drop => drop.source.copying ? board.copyCard(drop.source.cardId, drop.source.from, slotColumn.modelData.index) : board.moveCard(drop.source.cardId, drop.source.from, slotColumn.modelData.index)
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
                    Component.onCompleted: {
                        if (!slotColumn.isUnused && board.focusName !== "" && slotColumn.modelData.name === board.focusName) {
                            board.focusName = "";
                            forceActiveFocus();
                            selectAll();
                        }
                    }
                        onEditingFinished: {
                            if (text !== slotColumn.modelData.name) {
                                board.renameColumn(slotColumn.modelData.index, text);
                                text = Qt.binding(() => slotColumn.modelData.name);
                            }
                        }
                    }

                    Controls.ToolButton {
                        visible: !slotColumn.isUnused && slotColumn.modelData.members.length > 0
                        icon.name: "folder-new"
                        text: board.i18nContext.i18nc("@action:button %1 is the name of a column", "Save the column %1 as a group", slotColumn.modelData.name)
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        onClicked: {
                            ActionLog.taken("ColumnBoard_saveColumnGroup")
                            board.saveAsGroup(slotColumn.modelData.index)
                        }
                    }

                    Controls.ToolButton {
                        visible: !slotColumn.isUnused
                        icon.name: "edit-delete"
                        text: board.i18nContext.i18nc("@action:button %1 is the name of a column", "Remove the column %1", slotColumn.modelData.name)
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        onClicked: {
                            ActionLog.taken("ColumnBoard_removeColumn")
                            board.removeColumn(slotColumn.modelData.index)
                        }
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
                        : board.hasNoSpending(slotColumn.modelData.name)
                            ? board.i18nContext.i18nc("@info", "Nothing was spent on this in the period.")
                        : board.totalOf(slotColumn.modelData.name) !== ""
                            ? board.i18nContext.i18nc("@info %1 is an amount of money, or several with + between them", "Adds up to %1", board.totalOf(slotColumn.modelData.name))
                            : board.i18nContext.i18nc("@info", "Counted together as one figure.")
                }

                // Written out, not an icon alone: nobody should have to guess
                // where formulas are.
                Rectangle {
                    id: formulaToggle

                    readonly property string text: slotColumn.asFormula
                        ? board.i18nContext.i18nc("@action:button %1 is the name of a column", "Use categories in %1 instead of a formula", slotColumn.modelData.name)
                        : board.i18nContext.i18nc("@action:button %1 is the name of a column", "Work out %1 with a formula", slotColumn.modelData.name)

                    Layout.fillWidth: true
                    Layout.preferredHeight: toggleLabel.implicitHeight + Kirigami.Units.largeSpacing
                    visible: !slotColumn.isUnused && (slotColumn.asFormula || slotColumn.modelData.members.length === 0)
                    radius: Kirigami.Units.cornerRadius
                    color: toggleTap.pressed ? Qt.alpha(Kirigami.Theme.highlightColor, 0.25) : (toggleHover.hovered ? Qt.alpha(Kirigami.Theme.highlightColor, 0.1) : "transparent")
                    border.width: activeFocus ? 1 : 0
                    border.color: Kirigami.Theme.highlightColor
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: text
                    Accessible.onPressAction: board.toggleFormula(slotColumn.modelData.index)
                    Keys.onReturnPressed: board.toggleFormula(slotColumn.modelData.index)
                    Keys.onSpacePressed: board.toggleFormula(slotColumn.modelData.index)

                    // Written out, not an icon alone: nobody should have to
                    // guess where formulas are. A long name wraps.
                    Controls.Label {
                        id: toggleLabel

                        anchors.verticalCenter: parent.verticalCenter
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.margins: Kirigami.Units.smallSpacing
                        text: formulaToggle.text
                        wrapMode: Text.Wrap
                        horizontalAlignment: Text.AlignHCenter
                        textFormat: Text.PlainText
                        font.bold: true
                    }

                    HoverHandler {
                        id: toggleHover
                    }

                    TapHandler {
                        id: toggleTap

                        onTapped: board.toggleFormula(slotColumn.modelData.index)
                    }
                }

                Controls.TextField {
                    id: formulaInput

                    Layout.fillWidth: true
                    visible: slotColumn.asFormula
                    text: slotColumn.modelData.formula ?? ""
                    placeholderText: board.i18nContext.i18nc("@info:placeholder an example of a formula that uses the names of two columns", "For example: Home + Eating")
                    maximumLength: 500
                    Accessible.name: board.i18nContext.i18nc("@label:textbox", "Formula of the column")
                    // A bracket is closed as it is opened, and typing the closing
                    // one over an existing one just steps past it.
                    Keys.onPressed: event => {
                        if (event.text === "(" && !event.modifiers) {
                            const at = cursorPosition;
                            text = text.slice(0, at) + "()" + text.slice(selectionEnd);
                            cursorPosition = at + 1;
                            event.accepted = true;
                        } else if (event.text === ")" && text.charAt(cursorPosition) === ")") {
                            cursorPosition += 1;
                            event.accepted = true;
                        }
                    }
                    onEditingFinished: {
                        if (text.trim() !== (slotColumn.modelData.formula ?? "")) {
                            board.setColumnFormula(slotColumn.modelData.index, text);
                        }
                    }
                }

                // Help for people who do not write formulas every day: names
                // that fit what is typed, each function with a plain line, and
                // ready-made formulas to start from. Clicking keeps the focus
                // in the field; the keyboard can use Tab to reach the buttons.
                ColumnLayout {
                    id: formulaHelp

                    Layout.fillWidth: true
                    visible: slotColumn.asFormula
                    spacing: 0

                    readonly property var fitting: slotColumn.asFormula ? board.suggest(slotColumn.modelData.index, formulaInput.text, formulaInput.cursorPosition) : []
                    readonly property var ready: slotColumn.asFormula && formulaInput.text.trim() === "" ? board.examples(slotColumn.modelData.index) : []

                    function put(entry: var) {
                        const text = formulaInput.text;
                        // A function comes with its brackets closed.
                        const closes = entry.insert.endsWith("(") && text.charAt(entry.end) !== "(";
                        const put = closes ? entry.insert + ")" : entry.insert;
                        formulaInput.text = text.slice(0, entry.start) + put + text.slice(entry.end);
                        formulaInput.cursorPosition = entry.start + entry.insert.length;
                        formulaInput.forceActiveFocus();
                    }

                    // What is wrong so far, in words, as it is typed.
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: text !== ""
                        text: slotColumn.asFormula ? board.formulaProblem(slotColumn.modelData.index, formulaInput.text) : ""
                        color: Kirigami.Theme.negativeTextColor
                        font: Kirigami.Theme.smallFont
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: formulaHelp.fitting.length > 0
                        text: board.i18nContext.i18nc("@info above a list of names and functions that fit what is typed", "Suggestions. Click one to use it:")
                        font: Kirigami.Theme.smallFont
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                    }

                    Repeater {
                        model: formulaHelp.fitting

                        delegate: Controls.ItemDelegate {
                            required property var modelData

                            Layout.fillWidth: true
                            focusPolicy: Qt.NoFocus
                            padding: Kirigami.Units.smallSpacing
                            contentItem: ColumnLayout {
                                spacing: 0

                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: modelData.label
                                    font.bold: true
                                    elide: Text.ElideRight
                                    textFormat: Text.PlainText
                                }

                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: modelData.detail
                                    font: Kirigami.Theme.smallFont
                                    wrapMode: Text.Wrap
                                    textFormat: Text.PlainText
                                }
                            }
                            Accessible.name: modelData.label + ", " + modelData.detail
                            onClicked: {
                                ActionLog.taken("ColumnBoard_put")
                                formulaHelp.put(modelData)
                            }
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        visible: formulaHelp.ready.length > 0
                        text: board.i18nContext.i18nc("@info above a list of ready-made formulas", "Start from an example, then change it:")
                        font: Kirigami.Theme.smallFont
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                    }

                    Repeater {
                        model: formulaHelp.ready

                        delegate: Controls.ItemDelegate {
                            required property var modelData

                            Layout.fillWidth: true
                            focusPolicy: Qt.NoFocus
                            padding: Kirigami.Units.smallSpacing
                            contentItem: ColumnLayout {
                                spacing: 0

                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: modelData.text
                                    font.bold: true
                                    wrapMode: Text.Wrap
                                    textFormat: Text.PlainText
                                }

                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: modelData.detail
                                    font: Kirigami.Theme.smallFont
                                    wrapMode: Text.Wrap
                                    textFormat: Text.PlainText
                                }
                            }
                            Accessible.name: modelData.text + ", " + modelData.detail
                            onClicked: {
                                ActionLog.taken("ColumnBoard_clicked")
                                formulaInput.text = modelData.text;
                                board.setColumnFormula(slotColumn.modelData.index, modelData.text);
                            }
                        }
                    }
                }

                Repeater {
                    model: slotColumn.isUnused ? [board.formulaCard].concat(slotColumn.modelData.members) : slotColumn.modelData.members

                    delegate: Item {
                        id: slot

                        required property string modelData
                        // What the drop reads.
                        readonly property string cardId: modelData
                        readonly property int from: slotColumn.modelData.index
                        // Whether the drag holds Ctrl: the card is copied, not moved.
                        property bool copying: false

                        Layout.fillWidth: true
                        Layout.preferredHeight: tile.height

                        function dragged(active: bool) {
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

                        Rectangle {
                            id: tile

                            width: slot.width
                            height: Kirigami.Units.gridUnit * 2
                            radius: Kirigami.Units.cornerRadius
                            color: Kirigami.Theme.backgroundColor
                            border.width: 1
                            border.color: handler.active || copyHandler.active ? Kirigami.Theme.highlightColor : Kirigami.Theme.disabledTextColor
                            z: handler.active || copyHandler.active ? 100 : 0
                            Drag.dragType: Drag.None
                            Drag.keys: ["card"]
                            Drag.source: slot
                            Drag.hotSpot.x: width / 2
                            Drag.hotSpot.y: height / 2

                            // A plain drag moves the card, a drag with Ctrl held copies
                            // it into another column too.
                            DragHandler {
                                id: handler

                                acceptedModifiers: Qt.NoModifier
                                onActiveChanged: {
                                    slot.copying = false;
                                    slot.dragged(active);
                                }
                            }

                            DragHandler {
                                id: copyHandler

                                acceptedModifiers: Qt.ControlModifier
                                onActiveChanged: {
                                    slot.copying = true;
                                    slot.dragged(active);
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
                                    opacity: board.cardOf(slot.modelData) === undefined && slot.modelData !== board.formulaCard ? 0.6 : 1
                                }

                                // Shown while the card is being copied.
                                Controls.Label {
                                    visible: copyHandler.active
                                    font.bold: true
                                    text: "+"
                                    Accessible.ignored: true
                                }

                                Controls.ToolButton {
                                    id: menuButton

                                    icon.name: "overflow-menu"
                                    text: board.i18nContext.i18nc("@action:button %1 is the name of a category; opens a menu to put it in a column", "Move %1 to…", board.labelOf(slot.modelData))
                                    display: Controls.AbstractButton.IconOnly
                                    Controls.ToolTip.text: text
                                    Controls.ToolTip.visible: hovered
                                    onClicked: {
                                        ActionLog.taken("ColumnBoard_move")
                                        board.openMenu(menuButton, slot.modelData, slot.from)
                                    }
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

    // The list of what is not counted comes first on the left, and the rest
    // wraps to its right: when a row is full, the next column starts a new
    // row just below it, never below the tall list out of sight (M6-B22).
    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

        Repeater {
            model: [{index: -1, name: "", members: board.unused.map(card => card.id), formula: "", asFormula: false}]
            delegate: slotDelegate
        }

    Flow {
        Layout.fillWidth: true
        Layout.alignment: Qt.AlignTop
        Layout.minimumWidth: Kirigami.Units.gridUnit * 14
        Layout.preferredWidth: Kirigami.Units.gridUnit * 14
        spacing: Kirigami.Units.largeSpacing

        // The one that adds a column comes first, then the named columns.
        // Adds a column.
        Rectangle {
            width: Kirigami.Units.gridUnit * 14
            height: Kirigami.Units.gridUnit * 12
            radius: Kirigami.Units.cornerRadius
            color: Kirigami.Theme.backgroundColor
            border.width: addDrop.containsDrag ? 2 : 1
            border.color: Kirigami.Theme.highlightColor

            // A card dropped here starts a new column.
            DropArea {
                id: addDrop

                anchors.fill: parent
                keys: ["card"]
                onDropped: drop => {
                    board.moveToNew(drop.source.cardId, drop.source.copying ? -2 : drop.source.from, newName.text);
                    newName.text = "";
                }
            }

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
                    onAccepted: {
                        ActionLog.taken("ColumnBoard_nameNewColumn")
                        addButton.clicked()
                    }
                }

                Controls.Button {
                    id: addButton

                    text: board.i18nContext.i18nc("@action:button", "Add column")
                    icon.name: "list-add"
                    onClicked: {
                        ActionLog.taken("ColumnBoard_addColumn")
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
                    text: board.i18nContext.i18nc("@info", "Name it, then drag categories in, or drop a category here to start a column with it. For example, put Restaurants and Coffee into a column called Fast food.")
                }
            }
        }

        Repeater {
            model: board.columns.map((column, index) => ({index: index, name: column.name, members: column.members, formula: column.formula ?? "", asFormula: board.isFormula(column)}))
            delegate: slotDelegate
        }
    }
    }
}
