// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The default categories of a new book (D-029, D-116): plain words for a
// household, in the user's language. They are written into the book at the
// first start and are the user's own from then on (D-067), so a change here
// reaches new books only.
//
// Each list holds the names and, alongside, the name of the category each one
// sits inside; a top-level category has an empty parent and comes before
// what sits inside it.

import QtQuick
import org.kde.ki18n

QtObject {
    id: defaults

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    // Where money comes from.
    readonly property var income: [
        [i18nContext.i18nc("@item an income category", "Salary"), ""],
        [i18nContext.i18nc("@item an income category", "Pension"), ""],
        [i18nContext.i18nc("@item an income category: child benefit, housing benefit, unemployment benefit, ...", "Benefits"), ""],
        [i18nContext.i18nc("@item an income category", "Interest"), ""],
        [i18nContext.i18nc("@item an income category", "Gifts received"), ""],
        [i18nContext.i18nc("@item an income category", "Money back"), ""],
        [i18nContext.i18nc("@item an income category", "Other income"), ""],
    ]

    // What money is spent on. The second entry names the category it sits
    // inside.
    readonly property var spending: [
        [i18nContext.i18nc("@item a spending category", "Home"), ""],
        [i18nContext.i18nc("@item a spending category inside Home", "Rent or mortgage"), i18nContext.i18nc("@item a spending category", "Home")],
        [i18nContext.i18nc("@item a spending category inside Home", "Electricity and heating"), i18nContext.i18nc("@item a spending category", "Home")],
        [i18nContext.i18nc("@item a spending category inside Home", "Water and waste"), i18nContext.i18nc("@item a spending category", "Home")],
        [i18nContext.i18nc("@item a spending category inside Home", "Internet and phone"), i18nContext.i18nc("@item a spending category", "Home")],
        [i18nContext.i18nc("@item a spending category inside Home", "Household goods"), i18nContext.i18nc("@item a spending category", "Home")],
        [i18nContext.i18nc("@item a spending category inside Home", "Repairs"), i18nContext.i18nc("@item a spending category", "Home")],
        [i18nContext.i18nc("@item a spending category", "Food"), ""],
        [i18nContext.i18nc("@item a spending category inside Food", "Groceries"), i18nContext.i18nc("@item a spending category", "Food")],
        [i18nContext.i18nc("@item a spending category inside Food", "Eating out"), i18nContext.i18nc("@item a spending category", "Food")],
        [i18nContext.i18nc("@item a spending category", "Getting around"), ""],
        [i18nContext.i18nc("@item a spending category inside Getting around", "Public transport"), i18nContext.i18nc("@item a spending category", "Getting around")],
        [i18nContext.i18nc("@item a spending category inside Getting around", "Car"), i18nContext.i18nc("@item a spending category", "Getting around")],
        [i18nContext.i18nc("@item a spending category inside Getting around", "Bicycle"), i18nContext.i18nc("@item a spending category", "Getting around")],
        [i18nContext.i18nc("@item a spending category", "Health"), ""],
        [i18nContext.i18nc("@item a spending category inside Health", "Doctor and pharmacy"), i18nContext.i18nc("@item a spending category", "Health")],
        [i18nContext.i18nc("@item a spending category inside Health", "Health insurance"), i18nContext.i18nc("@item a spending category", "Health")],
        [i18nContext.i18nc("@item a spending category", "Insurance"), ""],
        [i18nContext.i18nc("@item a spending category", "Clothes"), ""],
        [i18nContext.i18nc("@item a spending category", "Children"), ""],
        [i18nContext.i18nc("@item a spending category inside Children", "School and childcare"), i18nContext.i18nc("@item a spending category", "Children")],
        [i18nContext.i18nc("@item a spending category", "Pets"), ""],
        [i18nContext.i18nc("@item a spending category", "Free time"), ""],
        [i18nContext.i18nc("@item a spending category inside Free time", "Hobbies and sport"), i18nContext.i18nc("@item a spending category", "Free time")],
        [i18nContext.i18nc("@item a spending category inside Free time", "Holidays"), i18nContext.i18nc("@item a spending category", "Free time")],
        [i18nContext.i18nc("@item a spending category inside Free time", "Subscriptions and streaming"), i18nContext.i18nc("@item a spending category", "Free time")],
        [i18nContext.i18nc("@item a spending category inside Free time", "Books and media"), i18nContext.i18nc("@item a spending category", "Free time")],
        [i18nContext.i18nc("@item a spending category", "Education"), ""],
        [i18nContext.i18nc("@item a spending category", "Gifts and donations"), ""],
        [i18nContext.i18nc("@item a spending category", "Fees and charges"), ""],
        [i18nContext.i18nc("@item a spending category", "Other spending"), ""],
    ]

    // The lists as the Rust side takes them: names and parents side by side.
    readonly property var incomeNames: income.map(row => row[0])
    readonly property var incomeParents: income.map(row => row[1])
    readonly property var spendingNames: spending.map(row => row[0])
    readonly property var spendingParents: spending.map(row => row[1])
}
