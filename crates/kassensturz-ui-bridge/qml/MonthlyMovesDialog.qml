// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The monthly moves between the user's own accounts that KassenSturz joins by
// itself (US6.3, D-749): what each one is, and a button to stop it. Removing
// one is a change like any other, so Undo brings it back. Every text and
// amount comes from Rust (D-065).
//
//     monthlyMoves.show();

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.Dialog {
    id: dialog

    // Any register will do: the moves belong to the book.
    required property AccountRegister register
    // What register.monthlyMoves() returned.
    property var moves: []

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Opens and reads the moves; it opens even if reading them fails, so
    // the button never does nothing (M6-B242).
    function show() {
        open();
        load();
    }

    function load() {
        const found = register.monthlyMoves();
        // An error comes back as an object without a length.
        if (found?.length === undefined) {
            error.result = found;
            moves = [];
        } else {
            error.result = ({});
            moves = found;
        }
    }

    title: i18nContext.i18nc("@title:window", "Monthly moves")
    standardButtons: Kirigami.Dialog.Close
    preferredWidth: Kirigami.Units.gridUnit * 28

    ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        FormError {
            id: error

            Layout.fillWidth: true
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: dialog.moves.length > 0
            text: dialog.i18nContext.i18nc("@info", "KassenSturz joins these moves by itself when your accounts' statements bring both sides, and lists what it joined after every import. Moves already joined stay joined. If you remove one, KassenSturz asks about that move again, one by one.")
            wrapMode: Text.Wrap
        }

        // No dead ends (AGENTS.md §6): say how a move gets here.
        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            visible: dialog.moves.length === 0 && error.result.code === undefined
            text: dialog.i18nContext.i18nc("@info", "No monthly moves yet")
            explanation: dialog.i18nContext.i18nc("@info", "When you join two entries that are one move between your accounts, KassenSturz can offer to join it by itself every month. It only does so if you tick the box there.")
        }

        Repeater {
            model: dialog.moves

            delegate: RowLayout {
                id: row

                required property var modelData

                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0

                    Controls.Label {
                        textFormat: Text.PlainText
                        Layout.fillWidth: true
                        text: dialog.i18nContext.i18nc("@info %1 and %2 are names of accounts, %3 an amount, %4 a day of the month", "“%1” → “%2”, %3, around day %4 of the month", row.modelData.from, row.modelData.to, row.modelData.amount, row.modelData.day)
                        wrapMode: Text.Wrap
                        font.bold: true
                    }

                    Controls.Label {
                        textFormat: Text.PlainText
                        Layout.fillWidth: true
                        visible: (row.modelData.lastJoined ?? "") !== ""
                        text: dialog.i18nContext.i18nc("@info %1 is a date", "Last joined %1", row.modelData.lastJoined ?? "")
                        wrapMode: Text.Wrap
                        font: Kirigami.Theme.smallFont
                    }

                    Controls.Label {
                        textFormat: Text.PlainText
                        Layout.fillWidth: true
                        visible: (row.modelData.party ?? "") !== ""
                        text: dialog.i18nContext.i18nc("@info %1 is the name the paying account shows for the money", "Only entries that say “%1”", row.modelData.party ?? "")
                        wrapMode: Text.Wrap
                        font: Kirigami.Theme.smallFont
                    }
                }

                Controls.Button {
                    icon.name: "edit-delete"
                    text: dialog.i18nContext.i18nc("@action:button", "Remove")
                    Accessible.description: dialog.i18nContext.i18nc("@info %1 and %2 are names of accounts", "Stop joining the move from “%1” to “%2” by itself", row.modelData.from, row.modelData.to)
                    onClicked: {
                        ActionLog.taken("MonthlyMovesDialog_remove")
                        const result = dialog.register.removeMove(row.modelData.id);
                        if (result.code !== undefined) {
                            error.result = result;
                        } else {
                            // Later, not here: reading again destroys this
                            // row, button and all, while its click is still
                            // being handled. That is the likely cause of the
                            // button that once did nothing afterwards
                            // (M6-B242).
                            Qt.callLater(dialog.load);
                        }
                    }
                }
            }
        }
    }
}
