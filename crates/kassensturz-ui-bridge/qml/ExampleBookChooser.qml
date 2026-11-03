// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Lets a person try KassenSturz with a made-up household (M7a.2). One row
// for each example book that comes with the app, each with a one-line
// description; the suggested start is marked. A pick always makes a new,
// clearly named book, so the user's own book is never touched and the
// bundled original stays as it is. The
// names and descriptions are written here, where KI18n finds them (D-034);
// Rust only knows the ids.
//
// This is a component, not a page: the setup wizard places it as one of its
// pages (M7a.9), and Settings places it in `ExampleBooksPage`. What follows
// a pick is up to the place: `bookMade()` says the new book is open.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard
import org.kde.ki18n
import io.github.vondor.kassensturz

ColumnLayout {
    id: chooser

    required property ExampleBooks examples

    // The example is in the open book, or in a new book that is open.
    signal bookMade(string id)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    spacing: Kirigami.Units.largeSpacing

    // The name a book is shown under, and called when it is copied.
    function bookName(id: string): string {
        switch (id) {
        case "suburban-family":
            return i18nContext.i18nc("@item the name of an example book", "Suburban Family");
        case "flower-utopia":
            return i18nContext.i18nc("@item the name of an example book", "Flower Utopia");
        case "space-colony":
            return i18nContext.i18nc("@item the name of an example book", "Space Colony");
        case "udssr-agent":
            return i18nContext.i18nc("@item the name of an example book", "USSR Government Agent");
        default:
            return id;
        }
    }

    function description(id: string): string {
        switch (id) {
        case "suburban-family":
            return i18nContext.i18nc("@info one line about an example book", "An everyday household with a checking account, savings, a credit card and cash. The easiest place to start.");
        case "flower-utopia":
            return i18nContext.i18nc("@info one line about an example book", "A small garden cooperative with a shared cash box, budgets and charts.");
        case "space-colony":
            return i18nContext.i18nc("@info one line about an example book", "Five years of life in a colony on Mars, with thousands of transactions to try reports and charts on.");
        case "udssr-agent":
            return i18nContext.i18nc("@info one line about an example book", "A made-up government agent in the 1980s, with several currencies and exchange rates.");
        default:
            return "";
        }
    }

    // Makes the copy of the book `id`.
    function choose(id: string) {
        const name = i18nContext.i18nc("@item %1 is the name of an example book; the name of the new book made from it", "Example – %1", bookName(id));
        const result = examples.make(id, name);
        error.result = result;
        if (result.code === undefined) {
            bookMade(id);
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        text: chooser.i18nContext.i18nc("@info", "These households are made up. Pick one to look around and try things out. KassenSturz makes a new book from it, so your own books are never changed.")
    }

    FormError {
        id: error

        Layout.fillWidth: true
        forNewBook: true
    }

    FormCard.FormCard {
        Layout.fillWidth: true

        Repeater {
            model: chooser.examples.books()

            delegate: FormCard.FormButtonDelegate {
                required property var modelData

                icon.name: "document-open"
                text: modelData.suggested
                    ? chooser.i18nContext.i18nc("@action:button %1 is the name of an example book", "%1 (suggested start)", chooser.bookName(modelData.id))
                    : chooser.bookName(modelData.id)
                description: chooser.description(modelData.id)
                // Written here, but shown as text, never as markup (D-143).
                descriptionItem.textFormat: Text.PlainText
                onClicked: {
                    ActionLog.taken("ExampleBookChooser_chooseBook")
                    chooser.choose(modelData.id)
                }
            }
        }
    }
}
