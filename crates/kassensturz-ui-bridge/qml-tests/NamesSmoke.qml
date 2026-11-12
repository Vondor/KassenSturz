// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: grid names

// Checks GridNames.qml (#780): the screen-reader name of a tile says where
// it sits with the width and height in their plural forms, and the title of
// the window that shows a whole tile (D-736) is only what the tile shows,
// never its place in the layout.
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 400
    height: 200
    visible: true
    property bool failed: false

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    GridNames {
        id: names
    }

    Timer {
        interval: 200
        running: true
        onTriggered: {
            const small = {"type": "notice.unsorted", "col": 0, "row": 26, "cols": 1, "rows": 1};
            const large = {"type": "notice.unsorted", "col": 0, "row": 26, "cols": 12, "rows": 2};
            const titled = {"type": "chart.bar", "col": 2, "row": 3, "cols": 6, "rows": 4,
                "subject": {"fields": [{"key": "title", "text": "Groceries"}]}};

            window.check(names.name(small) === "Transactions without a category, column 1, row 27, 1 column wide, 1 row high", "one column and one row are singular, got " + names.name(small));
            window.check(names.name(large) === "Transactions without a category, column 1, row 27, 12 columns wide, 2 rows high", "more columns and rows are plural, got " + names.name(large));
            window.check(names.name(titled) === "Bars: Groceries, column 3, row 4, 6 columns wide, 4 rows high", "a title the user gave is part of the name, got " + names.name(titled));

            window.check(names.windowTitle(small) === "Transactions without a category", "the whole tile's window says what it shows, got " + names.windowTitle(small));
            window.check(names.windowTitle(titled) === "Groceries", "the whole tile's window shows the title the user gave, got " + names.windowTitle(titled));
            window.check(names.windowTitle({"type": "table.income_sources", "col": 0, "row": 21, "cols": 12, "rows": 4}) === "Where your money came from", "a tile without a title says what it is");

            console.log(window.failed ? "grid names: FAILED" : "grid names: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
