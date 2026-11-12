// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The description of a custom chart in the properties panel (M6e.3, D-504,
// D-505, D-508): a field for the text, a check after a short pause in typing,
// "Open a chart description…" for a file, and the list of the parts that are
// not supported here with the choice to draw the chart without them. The
// panel decides nothing itself: `ChartDescription` (chart_spec_model.rs)
// checks the text with the parser, and only "Use this description" stores it,
// through the model, which refuses what the schema doesn't allow. Everything
// that comes from the text (a path, a name) is shown as plain text; the
// sentences are written here (D-034).

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: page

    // A `ComponentProperties`.
    required property var model
    // The text stored in the component, exactly as it was written.
    required property string stored
    // Whether the chart is drawn without the parts that are not supported.
    required property bool partial

    readonly property var result: description.view ?? ({})
    readonly property bool changed: area.text !== stored
    readonly property int count: result.count ?? 0
    readonly property var unsupported: result.unsupported ?? []
    // The file that was read last, so a file with the same text as the one
    // before still fills the field.
    property int lastOpened: 0

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // The eight places of the palette by the words a description writes
    // (M7a.10); `PaletteName::word` in Rust has the same words, in the same
    // order, and a test of the bridge compares them.
    readonly property var paletteWords: ["blue", "vermillion", "bluish-green", "reddish-purple", "orange", "sky-blue", "yellow", "grey"]

    function reasonText(reason: string): string {
        switch (reason) {
        case "data-source":
            return i18nContext.i18nc("@info a part of a chart description that is not supported", "Data comes only from your book, so data from a web address or typed in here isn't used.");
        case "expression":
            return i18nContext.i18nc("@info a part of a chart description that is not supported", "Calculations and expression texts aren't supported here.");
        case "transform":
            return i18nContext.i18nc("@info a part of a chart description that is not supported", "This kind of data step isn't supported here.");
        case "mark":
            return i18nContext.i18nc("@info a part of a chart description that is not supported", "This kind of mark isn't supported here.");
        case "composition":
            return i18nContext.i18nc("@info a part of a chart description that is not supported", "Putting several charts together in one description isn't supported here.");
        case "channel":
            return i18nContext.i18nc("@info a part of a chart description that is not supported", "This way of showing data isn't supported here, or not for this mark.");
        case "horizontal":
            return i18nContext.i18nc("@info a part of a chart description that is not supported", "Bars that run sideways aren't supported here.");
        case "option":
            return i18nContext.i18nc("@info a part of a chart description that is not supported", "This option isn't supported here.");
        case "value":
            return i18nContext.i18nc("@info a part of a chart description that is not supported", "This value isn't supported here.");
        }
        return i18nContext.i18nc("@info a part of a chart description that is not supported", "This part isn't supported here.");
    }

    spacing: Kirigami.Units.smallSpacing

    ChartDescription {
        id: description

        onViewChanged: {
            // A file was read: its text goes into the field, so the user
            // sees what was read before anything is stored.
            const opened = view.opened ?? 0;
            if (opened > page.lastOpened) {
                page.lastOpened = opened;
                area.text = view.text;
            }
        }
    }

    // The check runs a moment after the last key, not on every key.
    Timer {
        id: pause

        interval: 400
        onTriggered: description.check(area.text)
    }

    onStoredChanged: {
        area.text = stored;
        description.check(stored);
    }
    Component.onCompleted: description.check(stored)

    Dialogs.FileDialog {
        id: openDialog

        title: page.i18nContext.i18nc("@title:window", "Open a chart description")
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [
            page.i18nContext.i18nc("@item:inlistbox a file type", "Chart descriptions (*.vl.json *.json)"),
            page.i18nContext.i18nc("@item:inlistbox a file type", "All files (*)")
        ]
        onAccepted: {
            ActionLog.taken("SpecPage_chose")
            description.openFile(selectedFile)
        }
    }

    Controls.ScrollView {
        Layout.fillWidth: true
        Layout.preferredHeight: Kirigami.Units.gridUnit * 14
        Accessible.name: page.i18nContext.i18nc("@label:textbox the text that describes a custom chart", "Chart description")

        Controls.TextArea {
            id: area

            text: page.stored
            font: Kirigami.Theme.fixedWidthFont
            wrapMode: TextEdit.NoWrap
            selectByMouse: true
            persistentSelection: true
            placeholderText: page.i18nContext.i18nc("@info:placeholder", "Paste a chart description here, or open a file.")
            Accessible.name: page.i18nContext.i18nc("@label:textbox the text that describes a custom chart", "Chart description")
            onTextChanged: pause.restart()
        }
    }

    // Why the text can't be used, in words; nothing is shown while the field
    // is empty or the text is fine.
    FormError {
        Layout.fillWidth: true
        result: area.text.trim() !== "" && page.result.error !== undefined ? page.result.error : ({})
    }

    RowLayout {
        Layout.fillWidth: true

        Controls.Button {
            text: page.i18nContext.i18nc("@action:button", "Use this description")
            icon.name: "dialog-ok-apply"
            enabled: page.changed && page.result.ok === true
            onClicked: {
                ActionLog.taken("SpecPage_used")
                page.model.setText("spec", area.text)
            }
        }

        Controls.Button {
            text: page.i18nContext.i18nc("@action:button opens a file with a chart description", "Open a chart description…")
            icon.name: "document-open"
            onClicked: {
                ActionLog.taken("SpecPage_openClicked")
                openDialog.open()
            }
        }

        // Ready examples (US4.11, D-720): one goes into the field, where it
        // can be changed; nothing is kept until "Use this description".
        Controls.Button {
            id: exampleButton

            text: page.i18nContext.i18nc("@action:button opens a list of ready chart descriptions", "Insert an example")
            icon.name: "insert-text"
            onClicked: {
                ActionLog.taken("SpecPage_exampleMenu")
                exampleMenu.popup(exampleButton, 0, exampleButton.height)
            }

            Controls.Menu {
                id: exampleMenu

                Instantiator {
                    model: description.examples()

                    delegate: Controls.MenuItem {
                        required property var modelData

                        text: Plain.plain(exampleNames.name(modelData.key))
                        onTriggered: {
                            ActionLog.taken("SpecPage_exampleInserted")
                            area.text = modelData.text
                            description.check(area.text)
                        }
                    }
                    onObjectAdded: (index, object) => exampleMenu.insertItem(index, object)
                    onObjectRemoved: (index, object) => exampleMenu.removeItem(object)
                }
            }
        }
    }

    ChartExamples {
        id: exampleNames
    }

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
        opacity: 0.75
        text: page.i18nContext.i18nc("@info", "Settings › Help lists the data names a description can use, with their fields, and the examples.")
    }

    // The colors a description can use by name: the palette's places as they
    // are set in Settings (Q-410). There is no color field here, so charts
    // stay in step with each other; "Edit palette…" opens Settings. Any other
    // color can be written in the description as #rrggbb, for people who write
    // code.
    ColumnLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info", "Colors you can name in a description. They are the colors of your palette, so your charts match each other. For any other color, write it as #rrggbb.")
            textFormat: Text.PlainText
        }

        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            Repeater {
                model: page.paletteWords

                delegate: RowLayout {
                    id: word

                    required property int index
                    required property string modelData

                    spacing: Kirigami.Units.smallSpacing

                    Rectangle {
                        implicitWidth: Kirigami.Units.gridUnit
                        implicitHeight: Kirigami.Units.gridUnit
                        radius: Kirigami.Units.smallSpacing
                        color: ChartPalette.color(word.index, [])
                        border.width: 1
                        border.color: Kirigami.Theme.textColor
                        Accessible.ignored: true
                    }

                    Controls.Label {
                        text: word.modelData
                        textFormat: Text.PlainText
                        font: Kirigami.Theme.fixedWidthFont
                    }
                }
            }
        }

        Controls.Button {
            text: page.i18nContext.i18nc("@action:button opens Settings, where the colors of the charts are changed", "Edit palette…")
            icon.name: "color-management"
            onClicked: {
                ActionLog.taken("SpecPage_editPalette")
                ChartPalette.editRequested()
            }
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: page.changed && page.result.ok === true
        wrapMode: Text.Wrap
        font: Kirigami.Theme.smallFont
        opacity: 0.75
        text: page.i18nContext.i18nc("@info", "The chart still shows the earlier description. Choose “Use this description” to change it.")
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: !page.changed && page.result.ok === true && page.count === 0
        wrapMode: Text.Wrap
        font: Kirigami.Theme.smallFont
        opacity: 0.75
        text: page.i18nContext.i18nc("@info", "Everything in this description is supported here.")
    }

    // The parts that are not supported stay listed whenever the chart is
    // selected, so nothing is left out unseen (D-508).
    ColumnLayout {
        Layout.fillWidth: true
        visible: page.count > 0 && page.result.ok === true
        spacing: Kirigami.Units.smallSpacing

        Kirigami.Heading {
            Layout.fillWidth: true
            level: 4
            text: page.i18nContext.i18nc("@title the parts of a chart description that KassenSturz does not read", "Not supported here")
        }

        Repeater {
            model: page.unsupported

            delegate: ColumnLayout {
                id: part

                required property var modelData

                Layout.fillWidth: true
                spacing: 0

                Controls.Label {
                    Layout.fillWidth: true
                    text: part.modelData.path
                    textFormat: Text.PlainText
                    font: Kirigami.Theme.fixedWidthFont
                    elide: Text.ElideMiddle
                }

                Controls.Label {
                    Layout.fillWidth: true
                    text: page.reasonText(part.modelData.reason)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    opacity: 0.75
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !page.changed && !page.partial && page.result.canDraw === true
            wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info", "The chart is not drawn until you choose what to do with these parts.")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !page.changed && page.result.canDraw === false
            wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info", "Without these parts nothing is left to draw. Change the description so that it names data from your book, a mark and what the mark needs.")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !page.changed && page.partial
            wrapMode: Text.Wrap
            text: page.i18nContext.i18nc("@info", "The chart is drawn without these parts.")
        }

        Controls.Button {
            visible: !page.changed && !page.partial && page.result.canDraw === true
            text: page.i18nContext.i18nc("@action:button draw a chart without the parts of its description that are not supported", "Draw it without these")
            onClicked: {
                ActionLog.taken("SpecPage_leftOut")
                page.model.setFlag("partial", true)
            }
        }

        Controls.Button {
            visible: !page.changed && page.partial
            text: page.i18nContext.i18nc("@action:button stop leaving out the parts that are not supported", "Don't leave them out")
            onClicked: {
                ActionLog.taken("SpecPage_kept")
                page.model.setFlag("partial", false)
            }
        }
    }
}
