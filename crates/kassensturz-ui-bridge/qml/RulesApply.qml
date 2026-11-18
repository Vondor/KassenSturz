// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Sorts the transactions already in the book with the Rules (US1.7,
// D-630): "Apply now" on one Rule, or "Apply all" on the Rules page. A new
// Rule otherwise only sorts what comes in later. Nothing changes until the
// user has seen a preview, how many transactions it sorts with a few of
// them as they will look, and pressed the button that says so; the whole
// run is one change that one Undo takes back. By default only transactions
// without a category are sorted, and what the user sorted or changed is
// never touched (D-031). Sorting again what Rules sorted before is its own
// switch, off whenever the dialog opens (D-631).
//
//     applyDialog.openFor(ruleId, ruleName);   // "Apply now"
//     applyDialog.openFor("", "");             // "Apply all"
//
// `sorted(changed)` follows a run that was confirmed. It is not called
// `applied`: Kirigami.Dialog already has an `applied()` signal, and QML
// rejects a second signal of that name (M6-B199).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    required property RuleBook rules
    // The Rule to apply, by ID; empty for every Rule.
    property string ruleId: ""
    property string ruleName: ""
    // What `rules.previewApply()` returned last; empty while it failed.
    property var preview: ({})
    readonly property int count: preview.changed === undefined ? 0 : parseInt(preview.changed)
    // How many get a tag a Rule adds, sorted or not (D-827).
    readonly property int tagged: preview.tagged === undefined ? 0 : parseInt(preview.tagged)

    // The user confirmed, and `changed` transactions were sorted.
    signal sorted(int changed)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    function openFor(id: string, name: string) {
        ruleId = id;
        ruleName = name;
        resortBox.checked = false;
        refresh();
        open();
    }

    function refresh() {
        const result = rules.previewApply(ruleId, resortBox.checked);
        error.result = result;
        preview = result.code === undefined ? result : ({});
    }

    function apply() {
        const result = rules.applyRules(ruleId, resortBox.checked);
        error.result = result;
        if (result.code === undefined) {
            close();
            sorted(parseInt(result.changed));
        } else {
            preview = ({});
        }
    }

    // Why some transactions a Rule fits stay as they are, one line each.
    function skippedText(): string {
        const lines = [];
        const add = (count, line) => {
            if (count > 0) {
                lines.push(line);
            }
        };
        const transfers = parseInt(preview.transfers ?? 0);
        const noRate = parseInt(preview.noRate ?? 0);
        const closed = parseInt(preview.closed ?? 0);
        const other = parseInt(preview.other ?? 0);
        add(transfers, i18nContext.i18ncp("@info %1 is a number", "%1 transaction looks like money moved between your own accounts. Decide that in the account first; it stays as it is.", "%1 transactions look like money moved between your own accounts. Decide that in the account first; they stay as they are.", transfers));
        add(noRate, i18nContext.i18ncp("@info %1 is a number", "%1 transaction stays as it is because an exchange rate for its day is missing.", "%1 transactions stay as they are because an exchange rate for their day is missing.", noRate));
        add(closed, i18nContext.i18ncp("@info %1 is a number", "%1 transaction stays as it is because an account it would go to or come from is closed.", "%1 transactions stay as they are because an account they would go to or come from is closed.", closed));
        add(other, i18nContext.i18ncp("@info %1 is a number", "%1 more transaction a Rule fits can't be sorted and stays as it is.", "%1 more transactions a Rule fits can't be sorted and stay as they are.", other));
        return lines.join("\n");
    }

    title: Plain.plain(ruleId === ""
        ? i18nContext.i18nc("@title:window", "Sort your transactions with all Rules")
        : i18nContext.i18nc("@title:window %1 is the name of a Rule", "Sort your transactions with %1", ruleName))
    padding: Kirigami.Units.largeSpacing
    // Fixed, so the dialog doesn't shrink when nothing is left to sort.
    preferredWidth: Kirigami.Units.gridUnit * 32
    preferredHeight: Kirigami.Units.gridUnit * 30
    standardButtons: Kirigami.Dialog.NoButton
    customFooterActions: [
        Kirigami.Action {
            text: dialog.count === 0 && dialog.tagged > 0
                ? dialog.i18nContext.i18ncp("@action:button %1 is a number", "Tag %1 transaction", "Tag %1 transactions", dialog.tagged)
                : dialog.i18nContext.i18ncp("@action:button %1 is a number", "Sort %1 transaction", "Sort %1 transactions", dialog.count)
            icon.name: "view-sort"
            enabled: dialog.count > 0 || dialog.tagged > 0
            onTriggered: {
                ActionLog.taken("RulesApply_sortTransactions")
                dialog.apply()
            }
        },
        Kirigami.Action {
            text: dialog.i18nContext.i18nc("@action:button", "Cancel")
            icon.name: "dialog-cancel"
            onTriggered: {
                ActionLog.taken("RulesApply_cancel")
                dialog.close()
            }
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        FormError {
            id: error
            Layout.fillWidth: true
        }

        Controls.Label {
            Layout.fillWidth: true
            textFormat: Text.PlainText
            text: dialog.ruleId === ""
                ? dialog.i18nContext.i18nc("@info", "Your Rules sort the transactions already in your book, in the order of the list: the first Rule that fits a transaction decides. Transactions you sorted or changed yourself are never touched.")
                : dialog.i18nContext.i18nc("@info", "This Rule sorts the transactions already in your book that it decides. A Rule before it in the list still takes a transaction first. Transactions you sorted or changed yourself are never touched.")
            wrapMode: Text.Wrap
        }

        Controls.CheckBox {
            id: resortBox
            Layout.fillWidth: true
            text: dialog.i18nContext.i18nc("@option:check", "Also sort again what Rules sorted before")
            onToggled: {
                ActionLog.taken("RulesApply_alsoSortAgain")
                dialog.refresh()
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            text: dialog.i18nContext.i18nc("@info", "Without this, only transactions without a category are sorted. With it, what a Rule sorted earlier is sorted again as the Rules say now, for example after you changed a Rule or the order. What no Rule fits anymore stays as it is.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }

        Kirigami.Heading {
            Layout.fillWidth: true
            visible: dialog.preview.changed !== undefined
            level: 3
            textFormat: Text.PlainText
            text: dialog.count === 0
                ? (dialog.tagged === 0
                    ? dialog.i18nContext.i18nc("@info", "There is nothing to sort.")
                    : dialog.i18nContext.i18nc("@info", "There is nothing to sort, only tags to add."))
                : dialog.i18nContext.i18ncp("@info %1 is a number", "%1 transaction will be sorted.", "%1 transactions will be sorted.", dialog.count)
            wrapMode: Text.Wrap
        }

        // The tags Rules add go on whatever they fit, sorted or not; a tag
        // the user took off by hand is never put back (D-827).
        Controls.Label {
            Layout.fillWidth: true
            visible: dialog.tagged > 0
            textFormat: Text.PlainText
            text: dialog.i18nContext.i18ncp("@info %1 is a number", "%1 transaction gets tags from your Rules. A tag you took off yourself is not put back.", "%1 transactions get tags from your Rules. A tag you took off yourself is not put back.", dialog.tagged)
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: text !== ""
            textFormat: Text.PlainText
            text: dialog.preview.changed === undefined ? "" : dialog.skippedText()
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: dialog.count > 0
            textFormat: Text.PlainText
            text: dialog.count > (dialog.preview.examples ?? []).length
                ? dialog.i18nContext.i18nc("@info", "The newest of them, as they will look:")
                : dialog.i18nContext.i18nc("@info", "As they will look:")
            font: Kirigami.Theme.smallFont
        }

        Repeater {
            model: dialog.preview.examples ?? []

            delegate: TransactionRow {
                required property var modelData

                Layout.fillWidth: true
                date: modelData.date
                otherParty: modelData.otherParty
                category: modelData.category
                amount: modelData.amount
                exactAmount: modelData.exactAmount
                incoming: modelData.incoming
                kind: modelData.kind
                account: modelData.account
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: dialog.count > 0 || dialog.tagged > 0
            text: dialog.i18nContext.i18nc("@info", "All of it is one change: one Undo takes it back.")
            wrapMode: Text.Wrap
            font: Kirigami.Theme.smallFont
        }
    }
}
