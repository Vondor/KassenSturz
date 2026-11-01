// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Add reports from a KMyMoney file (M6d.6b, D-450): what would happen to
// each report of the file, before anything is written
// (docs/REPORT_DESIGNER_M6D.md section 5.3). Only the reports come into the
// book; nothing else of the file does. A new report is added unless the
// user unticks it; a view that differs from its report now stays as the
// user has it unless they tick "Replace"; nothing is deleted. "Add reports"
// writes what is ticked as one entry of the change history, so one Undo
// takes it back. The lines are worded by KmyReportsSummary (D-034); every
// name comes from the file or the book and is shown as plain text.

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
    readonly property var preview: importer.preview ?? ({})
    readonly property var reports: preview.reports ?? []
    readonly property var noLongerInFile: preview.noLongerInFile ?? []
    readonly property bool loaded: preview.fileName !== undefined
    readonly property int toWrite: (preview.added ?? 0) + (preview.replaced ?? 0)

    // The reports were written: how many views were added and replaced.
    signal added(int added, int replaced)
    // The user gave up on the file, or it couldn't be read.
    signal cancelled()
    // The book changed: the other screens read it again.
    signal bookChanged()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Add reports from KMyMoney")
    header: FormError {
        id: error
    }

    actions: [
        Kirigami.Action {
            icon.name: "list-add"
            enabled: page.loaded && page.toWrite > 0
            text: page.i18nContext.i18nc("@action:button", "Add reports")
            onTriggered: {
                ActionLog.taken("KmyReportsImportPage_add")
                page.add()
            }
        },
        Kirigami.Action {
            icon.name: "dialog-cancel"
            text: page.i18nContext.i18nc("@action:button", "Cancel")
            onTriggered: {
                ActionLog.taken("KmyReportsImportPage_cancel")
                importer.cancel();
                page.cancelled();
            }
        }
    ]

    KmyReportsImport {
        id: importer

        onChanged: page.bookChanged()
    }

    function add() {
        const result = importer.add();
        error.result = result;
        if (result.code === undefined) {
            added(result.added, result.replaced);
        }
    }

    // The report lines in `states`, with their index in the file's order.
    function linesIn(states: var): var {
        const lines = [];
        for (let index = 0; index < reports.length; index++) {
            if (states.indexOf(reports[index].state) >= 0) {
                lines.push({ index: index, line: reports[index] });
            }
        }
        return lines;
    }

    // What changed about a report, or why it isn't kept, in plain words.
    function reasonsOf(line: var): string {
        const parts = line.reasons.map(reason => words.reasonText(reason, line));
        if (line.state === "deleted-earlier") {
            parts.unshift(i18nContext.i18nc("@info about a report brought in before", "you deleted its view earlier"));
        }
        return parts.join("; ");
    }

    Component.onCompleted: error.result = importer.open(file)

    ColumnLayout {
        spacing: 0

        // Words only, never shown: the lines of the done page, for the
        // reasons of each report.
        KmyReportsSummary {
            id: words

            reports: []
        }

        FormCard.FormCard {
            visible: page.loaded

            FormCard.FormTextDelegate {
                text: page.i18nContext.i18nc("@info %1 is the name of a file", "Reports in %1", page.preview.fileName ?? "")
                textItem.textFormat: Text.PlainText
                description: page.i18nContext.i18nc("@info", "Only the reports come into your book, as saved views under Statistics; its accounts, categories and transactions don't. A view you have is changed only if you tick it, and nothing is deleted. Adding them can be undone from the menu.")
                descriptionItem.wrapMode: Text.Wrap
            }

            FormCard.FormTextDelegate {
                visible: page.reports.length === 0
                text: page.i18nContext.i18nc("@info", "This file has no saved reports.")
            }
        }

        Repeater {
            model: [
                {
                    states: ["new", "deleted-earlier"],
                    title: page.i18nContext.i18nc("@title:group reports from KMyMoney", "New in this file"),
                    note: page.i18nContext.i18nc("@info", "Ticked reports are added as new saved views.")
                },
                {
                    states: ["kept-yours"],
                    title: page.i18nContext.i18nc("@title:group reports from KMyMoney", "Different from the view you have"),
                    note: page.i18nContext.i18nc("@info", "The report or its view changed since it was brought in. Your view stays as it is unless you tick it: then KMyMoney's version replaces it, under the same name and in the same place.")
                }
            ]

            delegate: ColumnLayout {
                id: choices

                required property var modelData
                readonly property var lines: page.linesIn(modelData.states)

                Layout.fillWidth: true
                spacing: 0
                visible: lines.length > 0

                FormCard.FormHeader {
                    title: choices.modelData.title
                }

                FormCard.FormCard {
                    FormCard.FormTextDelegate {
                        text: choices.modelData.note
                        textItem.wrapMode: Text.Wrap
                    }

                    Repeater {
                        model: choices.lines

                        delegate: FormCard.FormCheckDelegate {
                            required property var modelData

                            // A check box's label has no text format, so the
                            // name goes through Plain (T-08).
                            text: Plain.plain(modelData.line.state === "kept-yours"
                                ? page.i18nContext.i18nc("@option:check %1 is the name of a saved view", "Replace “%1” with KMyMoney's version", modelData.line.name)
                                : modelData.line.name)
                            description: page.reasonsOf(modelData.line)
                            descriptionItem.textFormat: Text.PlainText
                            checked: modelData.line.chosen
                            onToggled: {
                                ActionLog.taken("KmyReportsImportPage_choose")
                                importer.choose(modelData.index, checked)
                            }
                        }
                    }
                }
            }
        }

        Repeater {
            model: [
                {
                    states: ["already-here"],
                    title: page.i18nContext.i18nc("@title:group reports from KMyMoney", "Already here"),
                    note: page.i18nContext.i18nc("@info", "These views are what their reports give: nothing to do.")
                },
                {
                    states: ["not-kept", "same-id"],
                    title: page.i18nContext.i18nc("@title:group reports", "Not kept"),
                    note: page.i18nContext.i18nc("@info", "A saved view can't show these yet. A view brought in from one of them earlier stays.")
                }
            ]

            delegate: ColumnLayout {
                id: group

                required property var modelData
                readonly property var lines: page.linesIn(modelData.states)

                Layout.fillWidth: true
                spacing: 0
                visible: lines.length > 0

                FormCard.FormHeader {
                    title: group.modelData.title
                }

                FormCard.FormCard {
                    FormCard.FormTextDelegate {
                        text: group.modelData.note
                        textItem.wrapMode: Text.Wrap
                    }

                    Repeater {
                        model: group.lines

                        delegate: FormCard.FormTextDelegate {
                            required property var modelData

                            text: modelData.line.name
                            textItem.textFormat: Text.PlainText
                            description: page.reasonsOf(modelData.line)
                            descriptionItem.textFormat: Text.PlainText
                        }
                    }
                }
            }
        }

        FormCard.FormHeader {
            visible: page.noLongerInFile.length > 0
            title: page.i18nContext.i18nc("@title:group saved views", "No longer in the file")
        }

        FormCard.FormCard {
            visible: page.noLongerInFile.length > 0

            FormCard.FormTextDelegate {
                text: page.i18nContext.i18nc("@info", "These views were brought in from reports the file doesn't have any more. They stay; you can delete them under Statistics.")
                textItem.wrapMode: Text.Wrap
            }

            Repeater {
                model: page.noLongerInFile

                delegate: FormCard.FormTextDelegate {
                    required property string modelData

                    text: modelData
                    textItem.textFormat: Text.PlainText
                }
            }
        }
    }
}
