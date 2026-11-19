// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A report from a plugin (#791 step 5, D-926): the plugin works the
// figures out from the accounts and months chosen here, and KassenSturz
// draws its table with its own components. Rust writes every amount and
// date (D-065); every text of the plugin is shown as plain text. The page
// says that the plugin, not KassenSturz, worked the figures out.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    // Which report: the plugin's file and the report's ID and name, and
    // the plugin's name.
    required property string file
    required property string reportId
    required property string reportName
    required property string pluginName

    // The last answer: `{ table, plugin }` or an error object.
    property var result: ({})
    readonly property var table: result.table ?? null
    readonly property real cellWidth: Kirigami.Units.gridUnit * 9

    function runReport() {
        result = reports.run(page.file, page.reportId, accountBox.currentValue ?? "", periodBox.currentValue ?? "3-months");
    }

    title: Plain.plain(table ? table.title : reportName)

    PluginReports {
        id: reports
    }

    DataExport {
        id: exporter
    }

    header: ColumnLayout {
        spacing: 0

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            position: Kirigami.InlineMessage.Position.Header
            type: Kirigami.MessageType.Information
            visible: true
            text: Plain.plain(page.i18nContext.i18nc("@info %1 is a plugin's name", "Worked out by the plugin \"%1\", not by KassenSturz. Check the figures against your accounts if something looks wrong.", page.pluginName))
        }

        FormError {
            Layout.fillWidth: true
            position: Kirigami.InlineMessage.Position.Header
            result: page.result
        }

        RowLayout {
            Layout.margins: Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                text: page.i18nContext.i18nc("@label:listbox", "Accounts:")
            }

            Controls.ComboBox {
                id: accountBox

                textRole: "text"
                valueRole: "value"
                Accessible.name: page.i18nContext.i18nc("@label:listbox", "Accounts:")
                model: [{ value: "", text: page.i18nContext.i18nc("@item:inlistbox", "All accounts") }]
                    .concat(exporter.accounts().map(account => ({ value: account.id, text: Plain.plain(account.name) })))
                onActivated: {
                    ActionLog.taken("PluginReportPage_account")
                    page.runReport()
                }
            }

            Controls.Label {
                text: page.i18nContext.i18nc("@label:listbox", "Period:")
            }

            Controls.ComboBox {
                id: periodBox

                textRole: "text"
                valueRole: "value"
                currentIndex: 1
                Accessible.name: page.i18nContext.i18nc("@label:listbox", "Period:")
                model: [
                    { value: "month", text: page.i18nContext.i18nc("@item:inlistbox", "This month") },
                    { value: "3-months", text: page.i18nContext.i18nc("@item:inlistbox", "Last 3 months") },
                    { value: "12-months", text: page.i18nContext.i18nc("@item:inlistbox", "Last 12 months") },
                    { value: "year", text: page.i18nContext.i18nc("@item:inlistbox", "This year") },
                    { value: "all", text: page.i18nContext.i18nc("@item:inlistbox", "Everything") },
                ]
                onActivated: {
                    ActionLog.taken("PluginReportPage_period")
                    page.runReport()
                }
            }

            Controls.Button {
                text: page.i18nContext.i18nc("@action:button", "Run again")
                icon.name: "view-refresh"
                onClicked: {
                    ActionLog.taken("PluginReportPage_run")
                    page.runReport()
                }
            }
        }
    }

    ListView {
        id: rows

        model: page.table ? page.table.rows : []
        Accessible.name: Plain.plain(page.table ? page.table.title : page.reportName)

        header: RowLayout {
            spacing: Kirigami.Units.largeSpacing

            Repeater {
                model: page.table ? page.table.columns : []

                Controls.Label {
                    required property var modelData

                    Layout.preferredWidth: page.cellWidth
                    horizontalAlignment: modelData.right ? Text.AlignRight : Text.AlignLeft
                    font.bold: true
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: modelData.label
                }
            }
        }

        delegate: RowLayout {
            id: row

            required property var modelData

            spacing: Kirigami.Units.largeSpacing

            Repeater {
                model: row.modelData.cells

                Controls.Label {
                    required property int index
                    required property var modelData

                    Layout.preferredWidth: page.cellWidth
                    horizontalAlignment: page.table.columns[index].right ? Text.AlignRight : Text.AlignLeft
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    color: row.modelData.negative.indexOf(index) >= 0 ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
                    text: modelData
                }
            }
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: page.table !== null && page.table.rows.length === 0
            text: page.i18nContext.i18nc("@info:placeholder", "The plugin found nothing to show for these accounts and months.")
        }
    }

    Component.onCompleted: runReport()

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }
}
