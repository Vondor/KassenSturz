// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Step 3 of the guided setup (#1031, docs/GUIDED_SETUP.md): the
// transactions without a category, grouped by the name they carry. Choosing
// a category for a group sorts all of its transactions as one change (one
// Undo); afterwards the page offers a Rule for that name (D-911), which the
// existing Rule page shows and the user confirms there. Counts and sums come
// from Rust as text (D-065). Nothing has to be sorted: "Continue" always
// works (D-342).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property GuidedSort sorter
    // Offers the categories a transaction of an account can get.
    required property AccountRegister register
    // This page's step and how many there are; 0 when opened from Help.
    property int step: 0
    property int count: 0

    // "Yes, make a Rule…" for a name just sorted: the Rule page, filled from
    // this transaction.
    signal createRuleClicked(string accountId, string transactionId)
    signal nextClicked()
    signal skipClicked()
    signal endClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // What `sorter.unsorted()` gave last.
    property var shown: ({ count: 0, more: 0, groups: [] })
    // The group sorted last, for the Rule offer: `name`, `sorted` (how
    // many), `accountId` and `transactionId`; null when there is none.
    property var offer: null
    // The group "New category …" was chosen for.
    property var pending: null

    function reload() {
        shown = sorter.unsorted();
    }

    function nameOf(group: var): string {
        return group.name === ""
            ? i18nContext.i18nc("@info transactions that carry no name", "Without a name")
            : group.name;
    }

    // The categories a group can get, or none if they can't be read.
    function choicesOf(group: var): var {
        const found = register.categoryChoices(group.accountId, group.moneyIn, "");
        // The list the bridge returns is a QVariantList, which QML doesn't
        // make a JS array: Array.isArray() said no and the page showed no
        // category at all (B-424). Array.from() copies it, as in
        // ChangeNames.qml; an error object has a `code` instead.
        if (found === undefined || found === null || found.code !== undefined) {
            return [];
        }
        return Array.from(found);
    }

    function sortGroup(group: var, categoryId: string) {
        const result = sorter.sort(group.key, categoryId);
        if (result.code !== undefined) {
            failed.text = Plain.plain(errors.messageFor(result));
            offer = null;
        } else {
            failed.text = "";
            offer = { name: nameOf(group), sorted: result.sorted, accountId: group.accountId, transactionId: group.transactionId };
        }
        reload();
    }

    Component.onCompleted: reload()

    // Writes error codes as messages.
    FormError {
        id: errors

        visible: false
    }

    NewCategoryDialog {
        id: newCategory

        income: page.pending !== null && page.pending.moneyIn
        onCreated: categoryId => {
            if (page.pending !== null) {
                page.sortGroup(page.pending, categoryId);
                page.pending = null;
            }
        }
    }

    title: i18nContext.i18nc("@title a step of the guided setup", "Sort your transactions")
    header: GuidedBar {
        step: page.step
        count: page.count
        onEndClicked: page.endClicked()
    }
    footer: GuidedFooter {
        nextText: page.step > 0
            ? page.i18nContext.i18nc("@action:button", "Continue")
            : page.i18nContext.i18nc("@action:button closes a step opened from Help", "Done")
        skipVisible: page.step > 0
        onSkipClicked: page.skipClicked()
        onNextClicked: page.nextClicked()
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.gridUnit
            Layout.maximumWidth: Kirigami.Units.gridUnit * 36
            Layout.preferredWidth: Kirigami.Units.gridUnit * 36
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: page.shown.count > 0
                    ? page.i18nContext.i18ncp("@info %1 is a number of transactions", "Charts and budgets only show money that has a category. %1 transaction has none yet. Choose a category for a name, and every transaction with that name is sorted at once.", "Charts and budgets only show money that has a category. %1 transactions have none yet. Choose a category for a name, and every transaction with that name is sorted at once.", page.shown.count)
                    : page.i18nContext.i18nc("@info", "Every transaction has a category. There is nothing to sort.")
            }

            // The Rule offer for the name sorted last.
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: page.offer !== null
                type: Kirigami.MessageType.Positive
                text: page.offer === null ? "" : Plain.plain(page.i18nContext.i18ncp("@info %1 is a number of transactions, %2 a name", "%1 transaction of “%2” is sorted. You can take it back with Undo. Should KassenSturz sort this name by itself in future?", "%1 transactions of “%2” are sorted. You can take it back with Undo. Should KassenSturz sort this name by itself in future?", page.offer.sorted, page.offer.name))
                actions: [
                    Kirigami.Action {
                        text: page.i18nContext.i18nc("@action:button opens the Rule page, filled in", "Yes, make a Rule…")
                        icon.name: "list-add"
                        onTriggered: {
                            ActionLog.taken("GuidedSortPage_makeRule")
                            const offer = page.offer;
                            page.offer = null;
                            page.createRuleClicked(offer.accountId, offer.transactionId);
                        }
                    },
                    Kirigami.Action {
                        text: page.i18nContext.i18nc("@action:button declines the offer of a Rule", "No, thanks")
                        onTriggered: {
                            ActionLog.taken("GuidedSortPage_noRule")
                            page.offer = null;
                        }
                    }
                ]
            }

            Kirigami.InlineMessage {
                id: failed

                Layout.fillWidth: true
                visible: text !== ""
                type: Kirigami.MessageType.Error
            }

            Repeater {
                model: page.shown.groups

                delegate: Kirigami.AbstractCard {
                    id: card

                    required property var modelData
                    readonly property var choices: page.choicesOf(modelData)

                    Layout.fillWidth: true

                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0

                            Controls.Label {
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                textFormat: Text.PlainText
                                text: Plain.plain(page.nameOf(card.modelData))
                            }

                            Controls.Label {
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                opacity: 0.7
                                font: Kirigami.Theme.smallFont
                                textFormat: Text.PlainText
                                text: page.i18nContext.i18ncp("@info %1 is a number of transactions, %2 their sum as an amount of money", "%1 transaction, %2", "%1 transactions, together %2", card.modelData.count, card.modelData.total)
                                Controls.ToolTip.text: card.modelData.exactTotal
                                Controls.ToolTip.visible: sumHover.hovered && card.modelData.exactTotal !== card.modelData.total

                                HoverHandler {
                                    id: sumHover
                                }
                            }
                        }

                        Controls.ComboBox {
                            id: categoryBox

                            Layout.preferredWidth: Kirigami.Units.gridUnit * 14
                            Accessible.name: page.i18nContext.i18nc("@label:listbox %1 is a name", "The category for “%1”", page.nameOf(card.modelData))
                            model: [page.i18nContext.i18nc("@item:inlistbox nothing is chosen yet", "Choose a category …")]
                                .concat(card.choices.map(choice => choice.path))
                                .concat([page.i18nContext.i18nc("@item:inlistbox the last entry of a category list: makes a new category", "New category …")])
                            onActivated: index => {
                                ActionLog.taken("GuidedSortPage_chooseCategory")
                                if (index <= 0) {
                                    return;
                                }
                                if (index > card.choices.length) {
                                    // The last entry makes a new category
                                    // (D-910), which the group then gets.
                                    categoryBox.currentIndex = 0;
                                    page.pending = card.modelData;
                                    newCategory.start();
                                    return;
                                }
                                page.sortGroup(card.modelData, card.choices[index - 1].id);
                            }
                        }
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: page.shown.more > 0
                wrapMode: Text.Wrap
                opacity: 0.7
                textFormat: Text.PlainText
                text: page.i18nContext.i18ncp("@info %1 is a number of names", "%1 more name isn't listed here. It shows once names above are sorted.", "%1 more names aren't listed here. They show once names above are sorted.", page.shown.more)
            }

        }
    }
}
