// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Adding a dashboard from a file (M6d.4, D-441, D-442): what the file holds
// and what this book has of it, before anything is added
// (docs/REPORT_DESIGNER_M6D.md section 4.3). Only "Add dashboard" writes,
// as one entry of the change history. Every name comes from the file or
// the book and is shown as plain text (T-08, T-21); the sentences are
// written here (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    // The file the user chose.
    required property url file
    readonly property var summary: importer.summary ?? ({})
    readonly property var found: summary.found ?? ({})
    readonly property var missing: summary.missing ?? []
    readonly property var showsMore: summary.showsMore ?? []
    readonly property var formulaNames: summary.formulaNames ?? []
    readonly property var unknown: summary.unknown ?? []
    readonly property bool loaded: summary.name !== undefined

    // The dashboard was added; the page shows it next.
    signal added(string id)
    // The user gave up on the file, or it couldn't be read.
    signal cancelled()
    // The book changed: the other screens read it again.
    signal bookChanged()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Add a dashboard from a file")
    header: FormError {
        id: error
    }

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            enabled: page.loaded && nameField.text.trim() !== ""
            text: page.i18nContext.i18nc("@action:button", "Add dashboard")
            onTriggered: {
                ActionLog.taken("DashboardFilePage_add")
                page.add()
            }
        },
        Kirigami.Action {
            icon.name: "dialog-cancel"
            text: page.i18nContext.i18nc("@action:button", "Cancel")
            onTriggered: {
                ActionLog.taken("DashboardFilePage_cancel")
                importer.cancel();
                page.cancelled();
            }
        }
    ]

    DashboardFileImport {
        id: importer

        onChanged: page.bookChanged()
    }

    GridNames {
        id: names
    }

    // A refusal of the dashboard inside the file is about the file, not
    // about a view the user made, so it gets the file's words.
    function fileResult(result: var): var {
        if (result.code === "report.document-invalid") {
            return { code: "report.dashboard-file-invalid", path: result.path ?? "" };
        }
        if (result.code === "report.unsupported-document-version") {
            return { code: "report.unsupported-dashboard-file-version", version: result.version };
        }
        return result;
    }

    function add() {
        error.result = importer.add(nameField.text);
        if (error.result.code === undefined) {
            added(error.result.id);
        }
    }

    // A widget as the summary names it: its title, or its kind.
    function widgetName(widget: var): string {
        return widget.title !== ""
            ? i18nContext.i18nc("@item %1 is the title of a widget", "\"%1\"", widget.title)
            : names.typeName(widget.kind);
    }

    function widgetList(widgets: var): string {
        return widgets.map(widget => widgetName(widget)).join(", ");
    }

    function missingLine(entry: var): string {
        let what;
        if (entry.kind === "account") {
            what = i18nContext.i18nc("@info %1 is the name of an account", "Account %1", entry.name);
        } else if (entry.kind === "group") {
            what = i18nContext.i18nc("@info %1 is the name of a group of categories", "Group %1", entry.name);
        } else {
            what = i18nContext.i18nc("@info %1 is a category with the categories it sits inside", "Category %1", entry.name);
        }
        const users = [];
        if (entry.filterBar) {
            users.push(i18nContext.i18nc("@info the bar above the widgets that narrows them by period, accounts and categories", "the filter bar"));
        }
        if (entry.widgets.length > 0) {
            users.push(widgetList(entry.widgets));
        }
        const reason = entry.several
            ? i18nContext.i18nc("@info", "matches more than one in this book, so none was taken")
            : i18nContext.i18nc("@info", "is not in this book");
        return users.length === 0
            ? i18nContext.i18nc("@info %1 names what is missing, %2 why", "%1 %2.", what, reason)
            : i18nContext.i18nc("@info %1 names what is missing, %2 why, %3 the widgets that used it", "%1 %2. Used by %3; left out there.", what, reason, users.join(", "));
    }

    function showsMoreLine(entry: var): string {
        const n = entry.named;
        if (entry.widget === undefined || entry.widget === null) {
            return entry.list === "accounts"
                ? i18nContext.i18ncp("@info", "The filter bar will count all accounts, because its account is not in this book.", "The filter bar will count all accounts, because none of its %1 accounts are in this book.", n)
                : i18nContext.i18ncp("@info", "The filter bar will count all categories, because its category is not in this book.", "The filter bar will count all categories, because none of its %1 categories are in this book.", n);
        }
        const widget = widgetName(entry.widget);
        switch (entry.list) {
        case "accounts":
            return i18nContext.i18ncp("@info %2 names a widget", "%2 will show all accounts, because its account is not in this book.", "%2 will show all accounts, because none of its %1 accounts are in this book.", n, widget);
        case "groups":
            return i18nContext.i18ncp("@info %2 names a widget", "%2 will show all groups, because its group is not in this book.", "%2 will show all groups, because none of its %1 groups are in this book.", n, widget);
        case "series":
            return i18nContext.i18ncp("@info %2 names a widget", "%2 will show what its data shows by itself, because its category is not in this book.", "%2 will show what its data shows by itself, because none of its %1 categories are in this book.", n, widget);
        }
        return i18nContext.i18ncp("@info %2 names a widget", "%2 will show all categories, because its category is not in this book.", "%2 will show all categories, because none of its %1 categories are in this book.", n, widget);
    }

    Component.onCompleted: {
        error.result = fileResult(importer.open(file));
        if (error.result.code === undefined) {
            nameField.text = summary.name;
        }
    }

    ColumnLayout {
        spacing: 0
        visible: page.loaded

        FormCard.FormCard {
            FormCard.FormTextFieldDelegate {
                id: nameField

                label: page.i18nContext.i18nc("@label:textbox", "Name of the new dashboard")
                maximumLength: 200
            }

            FormCard.FormDelegateSeparator {}

            // A file name could contain anything: plain text.
            FormCard.FormTextDelegate {
                text: page.i18nContext.i18ncp("@info %2 is a file name", "%2 holds a dashboard with %1 widget.", "%2 holds a dashboard with %1 widgets.", page.summary.widgets ?? 0, page.summary.fileName ?? "")
                textItem.textFormat: Text.PlainText
                textItem.wrapMode: Text.Wrap
            }
        }

        FormCard.FormHeader {
            title: page.i18nContext.i18nc("@title:group", "Uses from this book")
        }

        FormCard.FormCard {
            Repeater {
                model: [
                    { list: page.found.accounts ?? [], label: page.i18nContext.i18nc("@label names of accounts follow", "Accounts") },
                    { list: page.found.categories ?? [], label: page.i18nContext.i18nc("@label names of categories follow", "Categories") },
                    { list: page.found.groups ?? [], label: page.i18nContext.i18nc("@label names of groups of categories follow", "Groups") }
                ]

                delegate: FormCard.FormTextDelegate {
                    required property var modelData

                    visible: modelData.list.length > 0
                    text: modelData.label
                    textItem.textFormat: Text.PlainText
                    description: modelData.list.join("; ")
                    descriptionItem.textFormat: Text.PlainText
                    descriptionItem.wrapMode: Text.Wrap
                }
            }

            FormCard.FormTextDelegate {
                visible: (page.found.accounts ?? []).length + (page.found.categories ?? []).length + (page.found.groups ?? []).length === 0
                text: page.i18nContext.i18nc("@info", "Nothing: this dashboard names no accounts, categories or groups.")
                textItem.wrapMode: Text.Wrap
            }
        }

        FormCard.FormHeader {
            visible: page.missing.length + page.showsMore.length + page.formulaNames.length + page.unknown.length > 0
            title: page.i18nContext.i18nc("@title:group", "Not in this book")
        }

        FormCard.FormCard {
            visible: page.missing.length + page.showsMore.length + page.formulaNames.length + page.unknown.length > 0

            Repeater {
                model: page.missing

                delegate: FormCard.FormTextDelegate {
                    required property var modelData

                    icon.name: "dialog-information"
                    text: page.missingLine(modelData)
                    textItem.textFormat: Text.PlainText
                    textItem.wrapMode: Text.Wrap
                }
            }

            Repeater {
                model: page.showsMore

                delegate: FormCard.FormTextDelegate {
                    required property var modelData

                    icon.name: "dialog-warning"
                    text: page.showsMoreLine(modelData)
                    textItem.textFormat: Text.PlainText
                    textItem.wrapMode: Text.Wrap
                }
            }

            Repeater {
                model: page.formulaNames

                delegate: FormCard.FormTextDelegate {
                    required property var modelData

                    icon.name: "dialog-information"
                    text: page.i18nContext.i18nc("@info %1 is a name in a formula, %2 names a widget", "%2 uses %1 in its formula, which is not a category or group in this book. The widget will say so.", modelData.name, page.widgetName(modelData.widget))
                    textItem.textFormat: Text.PlainText
                    textItem.wrapMode: Text.Wrap
                }
            }

            Repeater {
                model: page.unknown

                delegate: FormCard.FormTextDelegate {
                    required property var modelData

                    icon.name: "dialog-information"
                    text: page.i18nContext.i18nc("@info %1 names a widget", "%1 is a kind of widget this version of KassenSturz doesn't know. It is kept and shown as an empty frame.", page.widgetName(modelData))
                    textItem.textFormat: Text.PlainText
                    textItem.wrapMode: Text.Wrap
                }
            }
        }
    }
}
