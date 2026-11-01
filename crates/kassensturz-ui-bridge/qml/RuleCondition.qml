// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// One condition of a Rule in the editor (M3.11, D-088): what to look at and
// what it has to be. Text is matched as plain text by default; a pattern
// (a regular expression) is a deliberate, advanced choice per condition
// (D-015). Amounts are typed without a sign next to the direction (Q-045).
// The condition itself lives in the editor's list model; this item shows one
// row of it and reports edits.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Kirigami.AbstractCard {
    id: card

    // The row of the editor's list model, with all its fields.
    required property var condition
    // What the editor offers: accounts, and the usual currency.
    required property var choices
    // An example date and amount, so the fields say how to type them.
    property string dateExample: ""
    property string amountExample: ""

    // The condition changed: these fields have these values now.
    signal edited(var values)
    signal removeClicked()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // What a condition can look at: a kind, and for text the field.
    readonly property var subjects: [
        { kind: "text", field: "other-party-name", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at: the name the bank gave for who paid or was paid", "Other party") },
        { kind: "text", field: "other-party-iban", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at: the other party's account number", "Other party's IBAN") },
        { kind: "text", field: "purpose", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at: the text a payment came with", "Purpose") },
        { kind: "text", field: "payee", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at: the name shown as From or To", "Name shown") },
        { kind: "amount", field: "", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at", "Amount") },
        { kind: "date", field: "", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at", "Date") },
        { kind: "currency", field: "", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at", "Currency") },
        { kind: "account", field: "", label: i18nContext.i18nc("@item:inlistbox what a Rule looks at", "Account") }
    ]
    readonly property var operators: [
        { key: "contains", label: i18nContext.i18nc("@item:inlistbox a text has to", "contains") },
        { key: "is", label: i18nContext.i18nc("@item:inlistbox a text has to", "is exactly") },
        { key: "starts-with", label: i18nContext.i18nc("@item:inlistbox a text has to", "starts with") },
        { key: "ends-with", label: i18nContext.i18nc("@item:inlistbox a text has to", "ends with") }
    ]
    readonly property bool isText: condition.kind === "text"
    readonly property bool isPattern: condition.operator === "pattern"

    function subjectIndex(): int {
        return subjects.findIndex(subject => subject.kind === condition.kind
            && (condition.kind !== "text" || subject.field === condition.field));
    }

    // The fields a new kind of condition starts with.
    function starting(subject: var): var {
        switch (subject.kind) {
        case "text":
            return { kind: "text", field: subject.field, operator: condition.operator || "contains", value: condition.value };
        case "amount":
            return { kind: "amount", direction: "out", amountTest: "any", amount: "", amountTo: "", currency: choices.currency };
        case "date":
            return { kind: "date", firstDate: "", lastDate: "" };
        case "currency":
            return { kind: "currency", currency: choices.currency };
        default:
            return { kind: "account", account: choices.accounts.length > 0 ? choices.accounts[0].id : "" };
        }
    }

    contentItem: RowLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.ComboBox {
                id: subjectBox
                Kirigami.FormData.label: card.i18nContext.i18nc("@label:listbox", "Look at:")
                model: card.subjects.map(subject => subject.label)
                currentIndex: card.subjectIndex()
                onActivated: index => {
                    ActionLog.taken("RuleCondition_whatConditionLooks")
                    card.edited(card.starting(card.subjects[index]))
                }
                Accessible.name: card.i18nContext.i18nc("@label:listbox", "What the condition looks at")
            }

            // Text.
            Controls.ComboBox {
                visible: card.isText && !card.isPattern
                Kirigami.FormData.label: card.i18nContext.i18nc("@label:listbox", "It")
                model: card.operators.map(operator => operator.label)
                currentIndex: Math.max(0, card.operators.findIndex(operator => operator.key === card.condition.operator))
                onActivated: index => {
                    ActionLog.taken("RuleCondition_howTextCompared")
                    card.edited({ operator: card.operators[index].key })
                }
                Accessible.name: card.i18nContext.i18nc("@label:listbox", "How the text is compared")
            }

            Controls.TextField {
                visible: card.isText
                Kirigami.FormData.label: card.isPattern
                    ? card.i18nContext.i18nc("@label:textbox", "Pattern:")
                    : card.i18nContext.i18nc("@label:textbox", "Text:")
                text: card.condition.value
                onTextEdited: card.edited({ value: text })
                Accessible.name: card.i18nContext.i18nc("@label:textbox", "The text or pattern to look for")
            }

            Controls.CheckBox {
                visible: card.isText && !card.isPattern
                text: card.i18nContext.i18nc("@option:check", "Capital letters must match")
                checked: card.condition.caseSensitive
                onToggled: {
                    ActionLog.taken("RuleCondition_capitalLettersMust")
                    card.edited({ caseSensitive: checked })
                }
            }

            // Advanced: a regular expression instead of plain text.
            Controls.CheckBox {
                visible: card.isText
                text: card.i18nContext.i18nc("@option:check an advanced choice", "Use a pattern (advanced)")
                checked: card.isPattern
                onToggled: {
                    ActionLog.taken("RuleCondition_usePatternAdvanced")
                    card.edited({ operator: checked ? "pattern" : "contains" })
                }
            }

            Controls.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                visible: card.isPattern
                text: card.i18nContext.i18nc("@info", "A pattern is a regular expression, for example ^REWE .* DANKE$. It can match part of the text. If you only want to look for some words, switch this off and use \"contains\".")
                wrapMode: Text.Wrap
                font: Kirigami.Theme.smallFont
            }

            // Amount.
            Controls.ComboBox {
                id: directionBox
                visible: card.condition.kind === "amount"
                Kirigami.FormData.label: card.i18nContext.i18nc("@label:listbox", "Money that was")
                model: card.condition.direction === "signed"
                    ? [card.i18nContext.i18nc("@item:inlistbox", "paid"), card.i18nContext.i18nc("@item:inlistbox", "received"), card.i18nContext.i18nc("@item:inlistbox a range across zero, typed with minus signs for money paid", "paid or received (use minus signs)")]
                    : [card.i18nContext.i18nc("@item:inlistbox", "paid"), card.i18nContext.i18nc("@item:inlistbox", "received")]
                currentIndex: card.condition.direction === "in" ? 1 : card.condition.direction === "signed" ? 2 : 0
                onActivated: index => {
                    ActionLog.taken("RuleCondition_whichWayMoney")
                    card.edited({ direction: index === 1 ? "in" : index === 2 ? "signed" : "out" })
                }
                Accessible.name: card.i18nContext.i18nc("@label:listbox", "Which way the money moved")
            }

            Controls.ComboBox {
                visible: card.condition.kind === "amount"
                Kirigami.FormData.label: card.i18nContext.i18nc("@label:listbox", "How much:")
                model: [
                    card.i18nContext.i18nc("@item:inlistbox any amount", "Any amount"),
                    card.i18nContext.i18nc("@item:inlistbox an amount of exactly", "Exactly"),
                    card.i18nContext.i18nc("@item:inlistbox an amount in a range", "Between")
                ]
                currentIndex: card.condition.amountTest === "exactly" ? 1 : card.condition.amountTest === "between" ? 2 : 0
                onActivated: index => {
                    ActionLog.taken("RuleCondition_howMuchMoney")
                    card.edited({ amountTest: ["any", "exactly", "between"][index] })
                }
                Accessible.name: card.i18nContext.i18nc("@label:listbox", "How much money")
            }

            Controls.TextField {
                visible: card.condition.kind === "amount" && card.condition.amountTest !== "any"
                Kirigami.FormData.label: card.condition.amountTest === "between"
                    ? card.i18nContext.i18nc("@label:textbox the lower end of a range", "From:")
                    : card.i18nContext.i18nc("@label:textbox an amount", "Amount:")
                text: card.condition.amount
                placeholderText: card.i18nContext.i18nc("@info:placeholder %1 is an example amount", "For example %1", card.amountExample)
                inputMethodHints: Qt.ImhFormattedNumbersOnly
                onTextEdited: card.edited({ amount: text })
            }

            Controls.TextField {
                visible: card.condition.kind === "amount" && card.condition.amountTest === "between"
                Kirigami.FormData.label: card.i18nContext.i18nc("@label:textbox the upper end of a range", "To:")
                text: card.condition.amountTo
                placeholderText: card.i18nContext.i18nc("@info:placeholder %1 is an example amount", "For example %1", card.amountExample)
                inputMethodHints: Qt.ImhFormattedNumbersOnly
                onTextEdited: card.edited({ amountTo: text })
            }

            Controls.Label {
                textFormat: Text.PlainText
                visible: card.condition.kind === "amount" && card.condition.amountTest !== "any"
                Kirigami.FormData.label: card.i18nContext.i18nc("@label", "Currency:")
                text: card.condition.currency
            }

            // Date.
            Controls.TextField {
                visible: card.condition.kind === "date"
                Kirigami.FormData.label: card.i18nContext.i18nc("@label:textbox the first day", "From:")
                text: card.condition.firstDate
                placeholderText: card.i18nContext.i18nc("@info:placeholder %1 is an example date", "For example %1", card.dateExample)
                onTextEdited: card.edited({ firstDate: text })
            }

            Controls.TextField {
                visible: card.condition.kind === "date"
                Kirigami.FormData.label: card.i18nContext.i18nc("@label:textbox the last day", "To:")
                text: card.condition.lastDate
                placeholderText: card.i18nContext.i18nc("@info:placeholder %1 is an example date", "For example %1", card.dateExample)
                onTextEdited: card.edited({ lastDate: text })
            }

            // Currency.
            Controls.TextField {
                visible: card.condition.kind === "currency"
                Kirigami.FormData.label: card.i18nContext.i18nc("@label:textbox", "Currency code:")
                text: card.condition.currency
                placeholderText: card.i18nContext.i18nc("@info:placeholder", "For example EUR")
                maximumLength: 3
                onTextEdited: card.edited({ currency: text.toUpperCase() })
            }

            // Account.
            Controls.ComboBox {
                visible: card.condition.kind === "account"
                Kirigami.FormData.label: card.i18nContext.i18nc("@label:listbox", "Account:")
                model: card.choices.accounts.map(account => account.name)
                currentIndex: Math.max(0, card.choices.accounts.findIndex(account => account.id === card.condition.account))
                onActivated: index => {
                    ActionLog.taken("RuleCondition_account")
                    card.edited({ account: card.choices.accounts[index].id })
                }
                Accessible.name: card.i18nContext.i18nc("@label:listbox", "The account")
            }
        }

        Controls.ToolButton {
            Layout.alignment: Qt.AlignTop
            icon.name: "edit-delete"
            text: card.i18nContext.i18nc("@action:button", "Remove this condition")
            display: Controls.AbstractButton.IconOnly
            onClicked: {
                ActionLog.taken("RuleCondition_removeCondition")
                card.removeClicked()
            }
            Controls.ToolTip.text: text
            Controls.ToolTip.visible: hovered
        }
    }
}
