// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The colors of the charts in Settings (M7a.10, D-589, Q-409, Q-410): one row
// for each of the eight places of the palette with the color, a field for its
// red, green and blue (or `#rrggbb`), a button for the system's own color
// dialog, and "Back to standard" for this color; "Back to standard for all"
// for the whole palette. A color that was changed and is hard to see on the
// background, or hard to tell from its neighbour, gets a sentence saying so
// in plain words (D-283, D-308); it can be kept anyway.
//
// Everything is text that Rust has checked (D-065): the field and the dialog
// hand over text, `ChartPaletteSettings.setColor` answers "ok", "format" or
// "part", and the screen shows the colors it is given back. The palette is
// for this computer, like how amounts look (Q-409); it is not part of a book.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: card

    // A `ChartPaletteSettings`: the colors in use, the standard ones, and
    // the warnings.
    required property var chartColors

    // Whether a change couldn't be saved for the next start.
    property bool notSaved: false
    // The place the color dialog is changing.
    property int editing: -1
    // For a place whose typed text was no color: "format" or "part".
    property var problems: ({})

    // The window's background as `#rrggbb`, which the contrast is checked
    // against, and the warnings for it. Reading `colors` makes the list
    // follow every change.
    readonly property string windowColor: Kirigami.Theme.backgroundColor.toString()
    readonly property var warnings: {
        card.chartColors.colors;
        return card.chartColors.warnings(card.windowColor);
    }

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    spacing: 0

    // The name of place `slot`, as a person would say the color.
    function placeName(slot: int): string {
        return [
            i18nContext.i18nc("@label name of a chart color", "Blue"),
            i18nContext.i18nc("@label name of a chart color", "Vermillion"),
            i18nContext.i18nc("@label name of a chart color", "Bluish green"),
            i18nContext.i18nc("@label name of a chart color", "Reddish purple"),
            i18nContext.i18nc("@label name of a chart color", "Orange"),
            i18nContext.i18nc("@label name of a chart color", "Sky blue"),
            i18nContext.i18nc("@label name of a chart color", "Yellow"),
            i18nContext.i18nc("@label name of a chart color", "Grey"),
            // The places a person can add (D-588).
            i18nContext.i18nc("@label name of a chart color", "Wine red"),
            i18nContext.i18nc("@label name of a chart color", "Green"),
            i18nContext.i18nc("@label name of a chart color", "Teal"),
            i18nContext.i18nc("@label name of a chart color", "Purple"),
            i18nContext.i18nc("@label name of a chart color", "Sand"),
            i18nContext.i18nc("@label name of a chart color", "Indigo"),
            i18nContext.i18nc("@label name of a chart color", "Rose"),
            i18nContext.i18nc("@label name of a chart color", "Dark red"),
        ][slot] ?? "";
    }

    // What place `slot` is called on screen: its standard name while it has
    // its standard color, and "Own color" once a person changed it, because
    // the standard name would then say the wrong color.
    function nameOf(slot: int): string {
        const changed = (chartColors.colors[slot] ?? "") !== (chartColors.standard[slot] ?? "");
        return changed
            ? i18nContext.i18nc("@label name of a chart color a person changed", "Own color")
            : placeName(slot);
    }

    // Opens the system's color dialog for place `slot`.
    function choose(slot: int) {
        editing = slot;
        dialog.selectedColor = chartColors.colors[slot] ?? "";
        dialog.open();
    }

    // The sentences for place `slot`: what is wrong with the text typed
    // for it, and the warnings about its color.
    function notesFor(slot: int): var {
        const notes = [];
        if (problems[slot] === "format") {
            notes.push({problem: true, text: i18nContext.i18nc("@info", "That is not a color. Write it as #rrggbb, for example #0072B2, or as three numbers for red, green and blue, for example 0, 114, 178.")});
        } else if (problems[slot] === "part") {
            notes.push({problem: true, text: i18nContext.i18nc("@info", "Red, green and blue each go from 0 to 255.")});
        }
        for (const warning of warnings) {
            if (warning.slot === slot && warning.kind === "contrast") {
                notes.push({problem: false, text: i18nContext.i18nc("@info", "This color is hard to see against the background. A darker or a lighter color is easier to read.")});
            } else if (warning.slot === slot && warning.kind === "alike") {
                notes.push({problem: false, text: i18nContext.i18nc("@info %1 is the name of a color", "This color looks very much like the next one (%1), so the two are hard to tell apart in a chart.", nameOf(warning.other))});
            } else if (warning.other === slot && warning.kind === "alike") {
                notes.push({problem: false, text: i18nContext.i18nc("@info %1 is the name of a color", "This color looks very much like the one before it (%1), so the two are hard to tell apart in a chart.", nameOf(warning.slot))});
            }
        }
        return notes;
    }

    // Gives place `slot` the color written in `entry` and keeps what went
    // wrong, so the row says it.
    function apply(slot: int, entry: string) {
        const answer = chartColors.setColor(slot, entry);
        const next = Object.assign({}, problems);
        if (answer === "format" || answer === "part") {
            next[slot] = answer;
        } else {
            delete next[slot];
        }
        problems = next;
        notSaved = answer === "not-saved";
    }

    function resetPlace(slot: int) {
        const next = Object.assign({}, problems);
        delete next[slot];
        problems = next;
        notSaved = chartColors.reset(slot) === "not-saved";
    }

    function resetAll() {
        problems = ({});
        notSaved = chartColors.resetAll() === "not-saved";
    }

    // Adds a place at the end, with its standard color.
    function addPlace() {
        notSaved = chartColors.addColor() === "not-saved";
    }

    // Removes place `slot`; the places after it move up, so what was said
    // about their typed text no longer fits.
    function removePlace(slot: int) {
        problems = ({});
        notSaved = chartColors.removeColor(slot) === "not-saved";
    }

    FormCard.FormCard {
        Layout.fillWidth: true

        Repeater {
            model: card.chartColors.colors.length

            delegate: ColumnLayout {
                id: place

                required property int index
                readonly property string current: card.chartColors.colors[index] ?? ""
                readonly property bool changed: current !== (card.chartColors.standard[index] ?? "")
                readonly property var notes: card.notesFor(index)

                Layout.fillWidth: true
                spacing: 0

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: Kirigami.Units.largeSpacing
                    spacing: Kirigami.Units.largeSpacing

                    Rectangle {
                        implicitWidth: Kirigami.Units.gridUnit * 2
                        implicitHeight: Kirigami.Units.gridUnit * 1.5
                        radius: Kirigami.Units.smallSpacing
                        color: place.current
                        border.width: 1
                        border.color: Kirigami.Theme.textColor
                        Accessible.ignored: true

                        // A swatch is what a person clicks first, so it opens
                        // the dialog like the button does (which stays for
                        // the keyboard).
                        TapHandler {
                            cursorShape: Qt.PointingHandCursor
                            onTapped: {
                                ActionLog.taken("ChartColorsCard_choose")
                                card.choose(place.index);
                            }
                        }
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        text: card.i18nContext.i18nc("@label %1 is the number of a chart color, %2 its name", "%1. %2", place.index + 1, card.nameOf(place.index))
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                    }

                    Controls.TextField {
                        id: entry

                        Layout.preferredWidth: Kirigami.Units.gridUnit * 8
                        text: place.current
                        font: Kirigami.Theme.fixedWidthFont
                        maximumLength: 40
                        inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                        Accessible.name: card.i18nContext.i18nc("@label accessible name of a field for the color of a chart; %1 is its number, %2 its name", "Color %1, %2: red, green and blue", place.index + 1, card.nameOf(place.index))
                        onEditingFinished: {
                            // Not for a text that is already the color.
                            if (text.trim() !== place.current) {
                                card.apply(place.index, text);
                            }
                            if (card.problems[place.index] === undefined) {
                                text = place.current;
                            }
                        }
                    }

                    Controls.Button {
                        text: card.i18nContext.i18nc("@action:button opens the system's dialog for choosing a color", "Choose…")
                        Accessible.name: card.i18nContext.i18nc("@action:button accessible name; %1 is the number of a chart color, %2 its name", "Choose color %1, %2", place.index + 1, card.nameOf(place.index))
                        onClicked: {
                            ActionLog.taken("ChartColorsCard_choose")
                            card.choose(place.index);
                        }
                    }

                    Controls.Button {
                        enabled: place.changed
                        text: card.i18nContext.i18nc("@action:button puts one chart color back to the one KassenSturz comes with", "Standard")
                        Accessible.name: card.i18nContext.i18nc("@action:button accessible name; %1 is the number of a chart color, %2 its name", "Back to the standard color %1, %2", place.index + 1, card.nameOf(place.index))
                        onClicked: {
                            ActionLog.taken("ChartColorsCard_reset")
                            card.resetPlace(place.index);
                        }
                    }

                    Controls.Button {
                        enabled: card.chartColors.colors.length > card.chartColors.minColors
                        text: card.i18nContext.i18nc("@action:button takes one color out of the chart colors", "Remove")
                        Accessible.name: card.i18nContext.i18nc("@action:button accessible name; %1 is the number of a chart color, %2 its name", "Remove color %1, %2", place.index + 1, card.nameOf(place.index))
                        onClicked: {
                            ActionLog.taken("ChartColorsCard_remove")
                            card.removePlace(place.index);
                        }
                    }

                    Connections {
                        target: place

                        // Keeps the field showing the color: typing in it
                        // ended its binding to the color.
                        function onCurrentChanged() {
                            entry.text = place.current;
                        }
                    }
                }

                Repeater {
                    model: place.notes

                    delegate: Controls.Label {
                        required property var modelData

                        Layout.fillWidth: true
                        Layout.leftMargin: Kirigami.Units.largeSpacing
                        Layout.rightMargin: Kirigami.Units.largeSpacing
                        Layout.bottomMargin: Kirigami.Units.smallSpacing
                        text: modelData.text
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: modelData.problem ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.neutralTextColor
                    }
                }

                FormCard.FormDelegateSeparator {
                    visible: place.index < card.chartColors.colors.length - 1
                }
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            enabled: card.chartColors.colors.length < card.chartColors.maxColors
            text: card.i18nContext.i18nc("@action:button adds one more color to the chart colors", "Add a color")
            onClicked: {
                ActionLog.taken("ChartColorsCard_add")
                card.addPlace();
            }
        }

        FormCard.FormDelegateSeparator {}

        FormCard.FormButtonDelegate {
            enabled: card.chartColors.changed
            text: card.i18nContext.i18nc("@action:button puts every chart color back to the ones KassenSturz comes with", "Back to standard for all")
            onClicked: {
                ActionLog.taken("ChartColorsCard_resetAll")
                card.resetAll();
            }
        }
    }

    FormCard.FormSectionText {
        textFormat: Text.PlainText
        text: card.notSaved
            ? card.i18nContext.i18nc("@info", "KassenSturz couldn't save this choice, so it applies only until you close KassenSturz. Check that your settings folder can be written to, then choose again.")
            : card.i18nContext.i18nc("@info", "These colors are used for every chart in KassenSturz: the first part of a chart gets color 1, the next color 2, and so on, and a chart with more parts than colors starts again at color 1. You can have from 2 to 16 colors. They change only how charts look on this computer, not your numbers. A warning means a color may be hard to see or to tell from its neighbour. You can keep it anyway. The first eight standard colors were chosen to work for people who see colors differently; the colors you add are not checked that way.")
    }

    // The system's own color dialog, where there is one.
    Dialogs.ColorDialog {
        id: dialog

        title: card.i18nContext.i18nc("@title:window", "Choose a color")
        onAccepted: {
            ActionLog.taken("ChartColorsCard_dialogAccepted")
            card.apply(card.editing, selectedColor.toString());
        }
    }
}
