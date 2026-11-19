// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What was changed in the book, newest first (M3.8, D-032, D-066, D-086).
// It says which change Undo would take back and which Redo would make again,
// with what they touched, so two similar changes can be told apart (#51).
// The list can be as long as the user keeps the history for; the newest
// changes are shown.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

FormCard.FormCardPage {
    id: page

    required property ChangeHistory history
    // Why an undo or redo that opened this page didn't work, or empty.
    property var failure: ({})
    // What "Undo anyway" did, or empty.
    property var stepped: ({})

    // Undo the newest `count` changes, up to the one the user picked
    // (B-395, D-870): Main undoes them one by one and says what it did.
    signal undoSteps(int count)

    // What the list shows (B-395, D-870): typed words to find, a kind of
    // change, the groups that are open and the row picked for "Undo up to
    // here".
    property string searchText: ""
    property string kind: "all"
    property var openGroups: ({})
    property var picked: null

    // A gap this long between two changes starts a new sitting.
    readonly property int sittingGapSeconds: 30 * 60

    // The kind of a change for the filter: where it belongs, imports apart.
    function kindOf(action: string): string {
        if (action === "import-statement" || action === "fetch-statement") {
            return "imports";
        }
        if (action === "apply-ai-changes") {
            return "suggested";
        }
        const place = names.place(action);
        return place === "accounts" ? "transactions" : place;
    }

    function dayKey(date: var): string {
        return date.getFullYear() + "-" + (date.getMonth() + 1) + "-" + date.getDate();
    }

    function dayName(date: var): string {
        const today = new Date();
        const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
        if (dayKey(date) === dayKey(today)) {
            return i18nContext.i18nc("@title:group a day", "Today");
        }
        if (dayKey(date) === dayKey(yesterday)) {
            return i18nContext.i18nc("@title:group a day", "Yesterday");
        }
        return date.toLocaleDateString(Qt.locale(), Locale.LongFormat);
    }

    function timeOf(date: var): string {
        return date.toLocaleTimeString(Qt.locale(), Locale.ShortFormat);
    }

    // The changes as the page shows them: newest first, grouped by day and
    // sitting, the same step done several times in a row as one row, and
    // only those that match the search and the kind. Each row keeps how many
    // changes Undo takes back up to it (`steps`), counted over all changes,
    // so a filter never changes what "Undo up to here" does.
    function grouped(changes: var, search: string, kind: string): var {
        const groups = [];
        const needle = search.trim().toLowerCase();
        let steps = 0;
        let group = null;
        let previous = null;
        for (const change of Array.from(changes)) {
            if (!change.undone) {
                steps += 1;
            }
            const text = names.describe(change);
            const seconds = Number(change.madeAtSeconds ?? 0);
            const date = new Date(seconds * 1000);
            const matches = (kind === "all" || kindOf(change.action) === kind)
                && (needle === "" || text.toLowerCase().indexOf(needle) >= 0);
            if (!group || dayKey(date) !== group.day || group.oldestSeconds - seconds > sittingGapSeconds) {
                group = { day: dayKey(date), name: dayName(date), newest: timeOf(date), oldest: timeOf(date), oldestSeconds: seconds, count: 0, rows: [] };
                groups.push(group);
                previous = null;
            }
            group.oldest = timeOf(date);
            group.oldestSeconds = seconds;
            if (!matches) {
                previous = null;
                continue;
            }
            group.count += 1;
            if (previous && previous.text === text && previous.undone === change.undone) {
                previous.times += 1;
                previous.steps = steps;
                previous.time = timeOf(date);
                continue;
            }
            previous = { text: text, undone: change.undone, times: 1, steps: steps, time: timeOf(date), key: String(seconds) + ":" + steps };
            group.rows.push(previous);
        }
        return groups.filter(group => group.count > 0);
    }

    readonly property var groups: grouped(history.changes, searchText, kind)

    // The first group is open; the others when the user opens them.
    function isOpen(index: int, group: var): bool {
        const key = group.day + "/" + group.oldestSeconds;
        return openGroups[key] ?? index === 0;
    }

    function toggle(index: int, group: var) {
        const key = group.day + "/" + group.oldestSeconds;
        const next = Object.assign({}, openGroups);
        next[key] = !isOpen(index, group);
        openGroups = next;
    }

    // Undo and redo are in the side menu; this page stays open after them
    // to show the new state.
    function survivesUndo(): bool {
        return true;
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    ChangeNames {
        id: names
    }

    title: i18nContext.i18nc("@title", "History")
    // The one button for the picked row, with what it would do (B-395,
    // D-870): every newer change goes too, since Undo goes newest first. It
    // sits in the page's footer, so it stays in view next to the list
    // however far down the picked row is (B-410).
    footer: Controls.ToolBar {
        visible: page.picked !== null && !page.picked.undone
        padding: Kirigami.Units.largeSpacing

        contentItem: RowLayout {
            spacing: Kirigami.Units.largeSpacing

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    font.bold: true
                    text: page.picked ? page.i18nContext.i18nc("@info %1 is what was done", "Picked: %1", page.picked.text) : ""
                }

                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: page.picked ? page.i18nContext.i18ncp("@info", "Takes back %1 change: this one.", "Takes back %1 changes: this one and every newer one. Redo brings them back.", page.picked.steps) : ""
                }
            }

            Controls.Button {
                objectName: "undoUpToHere"
                icon.name: "edit-undo"
                text: page.i18nContext.i18nc("@action:button", "Undo up to here")
                onClicked: {
                    ActionLog.taken("HistoryPage_undoUpToHere")
                    const count = page.picked.steps;
                    page.picked = null;
                    page.undoSteps(count);
                }
            }
        }
    }

    header: ColumnLayout {
        spacing: 0

        FormError {
            Layout.fillWidth: true
            result: page.failure
            // The deliberate way out when a filed bill can't move back
            // (US7, #333, docs/RECEIPT_INBOX.md §7): the link goes, the file
            // stays where it is.
            actions: [
                Kirigami.Action {
                    visible: page.failure.code === "bill.undo-blocked"
                    icon.name: "edit-undo"
                    text: page.i18nContext.i18nc("@action:button", "Undo anyway, and leave the bills where they are")
                    onTriggered: {
                        ActionLog.taken("HistoryPage_undoAnyway")
                        const result = page.history.undoAnyway();
                        page.failure = result.code !== undefined ? result : ({});
                        page.stepped = result.code === undefined ? result : ({});
                    }
                }
            ]
        }

        // Where the bills stay that an undo unlinked without moving them.
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Information
            visible: (page.stepped.leftInArchive ?? []).length > 0
            text: visible ? Plain.plain(page.i18nContext.i18nc("@info %1 is a list of files", "The link is gone. The bills stay in your archive: %1", Array.from(page.stepped.leftInArchive).join(", "))) : ""
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "Next to undo or redo")
        visible: nextCard.visible
    }

    FormCard.FormCard {
        id: nextCard

        visible: page.history.undoChange.action !== undefined || page.history.redoChange.action !== undefined

        FormCard.FormTextDelegate {
            visible: page.history.undoChange.action !== undefined
            icon.name: "edit-undo"
            text: page.i18nContext.i18nc("@info %1 is what was done", "Undo takes back: %1", names.describe(page.history.undoChange))
            description: names.summary(page.history.undoChange)
            // A file's name could contain anything.
            textItem.textFormat: Text.PlainText
            descriptionItem.textFormat: Text.PlainText
        }

        FormCard.FormDelegateSeparator {
            visible: page.history.undoChange.action !== undefined && page.history.redoChange.action !== undefined
        }

        FormCard.FormTextDelegate {
            visible: page.history.redoChange.action !== undefined
            icon.name: "edit-redo"
            text: page.i18nContext.i18nc("@info %1 is what was done", "Redo makes again: %1", names.describe(page.history.redoChange))
            description: names.summary(page.history.redoChange)
            textItem.textFormat: Text.PlainText
            descriptionItem.textFormat: Text.PlainText
        }
    }

    FormCard.FormHeader {
        title: page.i18nContext.i18nc("@title:group", "All changes")
    }

    // Find a change by its words, or show one kind (B-395, D-870).
    FormCard.FormCard {
        visible: page.history.total > 0

        FormCard.AbstractFormDelegate {
            background: null
            contentItem: RowLayout {
                spacing: Kirigami.Units.largeSpacing

                Kirigami.SearchField {
                    Layout.fillWidth: true
                    placeholderText: page.i18nContext.i18nc("@info:placeholder", "Find a change…")
                    onTextChanged: page.searchText = text
                }

                Controls.ComboBox {
                    id: kindBox

                    textRole: "text"
                    valueRole: "value"
                    model: [
                        { value: "all", text: page.i18nContext.i18nc("@item:inlistbox kinds of changes", "All kinds") },
                        { value: "transactions", text: page.i18nContext.i18nc("@item:inlistbox kinds of changes", "Transactions and accounts") },
                        { value: "imports", text: page.i18nContext.i18nc("@item:inlistbox kinds of changes", "Imports") },
                        { value: "budgets", text: page.i18nContext.i18nc("@item:inlistbox kinds of changes", "Budgets") },
                        { value: "statistics", text: page.i18nContext.i18nc("@item:inlistbox kinds of changes", "Views") },
                        { value: "rules", text: page.i18nContext.i18nc("@item:inlistbox kinds of changes", "Rules") },
                        { value: "categories", text: page.i18nContext.i18nc("@item:inlistbox kinds of changes", "Categories") },
                        { value: "settings", text: page.i18nContext.i18nc("@item:inlistbox kinds of changes", "Settings") },
                        { value: "suggested", text: page.i18nContext.i18nc("@item:inlistbox kinds of changes", "Suggested changes you applied") },
                    ]
                    onActivated: {
                        ActionLog.taken("HistoryPage_kind")
                        page.kind = currentValue
                    }
                }
            }
        }
    }

    Repeater {
        model: page.groups

        delegate: FormCard.FormCard {
            id: groupCard

            required property int index
            required property var modelData
            readonly property bool open: page.isOpen(index, modelData)

            Layout.topMargin: Kirigami.Units.largeSpacing

            FormCard.FormButtonDelegate {
                icon.name: groupCard.open ? "arrow-down" : "arrow-right"
                text: Plain.plain(groupCard.modelData.newest === groupCard.modelData.oldest
                    ? page.i18nContext.i18ncp("@title:group %2 is a day, %3 a time, %1 a number", "%2, %3 · %1 change", "%2, %3 · %1 changes", groupCard.modelData.count, groupCard.modelData.name, groupCard.modelData.newest)
                    : page.i18nContext.i18ncp("@title:group %2 is a day, %3 and %4 times, %1 a number", "%2, %3 to %4 · %1 change", "%2, %3 to %4 · %1 changes", groupCard.modelData.count, groupCard.modelData.name, groupCard.modelData.oldest, groupCard.modelData.newest))
                onClicked: {
                    ActionLog.taken("HistoryPage_toggleGroup")
                    page.toggle(groupCard.index, groupCard.modelData)
                }
            }

            Repeater {
                model: groupCard.open ? groupCard.modelData.rows : []

                delegate: ColumnLayout {
                    id: row

                    required property var modelData

                    Layout.fillWidth: true
                    spacing: 0

                    FormCard.FormDelegateSeparator {}

                    FormCard.FormRadioDelegate {
                        checked: page.picked !== null && page.picked.key === row.modelData.key
                        // A file's name could contain anything, so it is shown
                        // as plain text.
                        text: Plain.plain(row.modelData.times > 1
                            ? page.i18nContext.i18ncp("@info %2 is what was done, %1 how often in a row", "%2 (%1 time)", "%2 (%1 times)", row.modelData.times, row.modelData.text)
                            : row.modelData.text)
                        description: Plain.plain(row.modelData.undone
                            ? page.i18nContext.i18nc("@info %1 is a time", "%1 · taken back; Redo can make it again", row.modelData.time)
                            : row.modelData.time)
                        opacity: row.modelData.undone ? 0.6 : 1
                        onClicked: {
                            ActionLog.taken("HistoryPage_pick")
                            page.picked = row.modelData
                        }
                    }
                }
            }
        }
    }

    FormCard.FormSectionText {
        visible: page.history.total > 0 && page.groups.length === 0
        text: page.i18nContext.i18nc("@info", "No change matches. Clear the search or choose All kinds.")
    }

    FormCard.FormSectionText {
        visible: page.history.total === 0
        text: page.i18nContext.i18nc("@info", "Nothing has been changed yet, or the history was cleaned up. What you change from now on shows up here, and you can undo it.")
    }

    FormCard.FormSectionText {
        textFormat: Text.PlainText
        visible: page.history.total > page.history.shown
        text: page.i18nContext.i18nc("@info %1 is how many changes there are, %2 how many are shown", "The newest %2 of %1 changes are shown. Undo goes back through all of them, newest first.", page.history.total, page.history.shown)
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Undo takes back the newest change first, and an older one only after the newer ones. How long the history is kept is set in Settings.")
    }
}
