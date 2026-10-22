// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// What was changed in the book, newest first (M3.8, D-032, D-066, D-086).
// It says which change Undo would take back and which Redo would make again,
// with what they touched, so two similar changes can be told apart (#51).
// The list can be as long as the user keeps the history for; the newest
// changes are shown.

import QtQuick
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
    header: FormError {
        result: page.failure
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

    FormCard.FormCard {
        visible: page.history.total > 0

        Repeater {
            model: page.history.changes

            delegate: ColumnLayout {
                id: row

                required property int index
                required property var modelData

                Layout.fillWidth: true
                spacing: 0

                FormCard.FormDelegateSeparator {
                    visible: row.index > 0
                }

                FormCard.FormTextDelegate {
                    text: names.describe(row.modelData)
                    description: row.modelData.undone
                        ? page.i18nContext.i18nc("@info %1 is a date and time", "%1 · taken back; Redo can make it again", row.modelData.madeAt)
                        : row.modelData.madeAt
                    // A file's name could contain anything.
                    textItem.textFormat: Text.PlainText
                    descriptionItem.textFormat: Text.PlainText
                    textItem.opacity: row.modelData.undone ? 0.6 : 1
                }
            }
        }
    }

    FormCard.FormSectionText {
        visible: page.history.total === 0
        text: page.i18nContext.i18nc("@info", "Nothing has been changed yet, or the history was cleaned up. What you change from now on shows up here, and you can undo it.")
    }

    FormCard.FormSectionText {
        visible: page.history.total > page.history.shown
        text: page.i18nContext.i18nc("@info %1 is how many changes there are, %2 how many are shown", "The newest %2 of %1 changes are shown. Undo goes back through all of them, newest first.", page.history.total, page.history.shown)
    }

    FormCard.FormSectionText {
        text: page.i18nContext.i18nc("@info", "Undo takes back the newest change first, and an older one only after the newer ones. How long the history is kept is set in Settings.")
    }
}
