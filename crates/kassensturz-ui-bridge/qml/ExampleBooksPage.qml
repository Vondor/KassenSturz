// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// "Try an example book…" in Settings (M7a.2): the chooser on a page of its
// own. The setup wizard (M7a.9) places `ExampleBookChooser` as one of its
// pages instead.

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.ki18n
import io.github.vondor.kassensturz

Kirigami.ScrollablePage {
    id: page

    required property ExampleBooks examples

    // The example is in the open book, or in a new book that is open.
    signal bookMade(string id)

    readonly property KI18nContext i18nContext: KI18nContext {
        translationDomain: "kassensturz"
    }

    title: i18nContext.i18nc("@title", "Try an example book")

    ExampleBookChooser {
        examples: page.examples
        onBookMade: id => page.bookMade(id)
    }
}
