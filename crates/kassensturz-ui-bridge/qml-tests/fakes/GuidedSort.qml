// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust GuidedSort, for `tools/check-panel.sh` (#1031):
// the groups a test gives it, as guided_sort_model.rs builds them. Sorting
// takes the group out and is kept, so a test can read what the page asked.

import QtQuick

QtObject {
    property var groups: []
    property int more: 0
    // Every sort the page asked for: "key>category".
    property var asked: []
    // Set to refuse the next sort with this code.
    property string refuse: ""

    signal changed()

    function unsorted() {
        return ({ count: groups.reduce((sum, group) => sum + group.count, 0), more: more, groups: groups });
    }

    function sort(key, categoryId) {
        asked = asked.concat([key + ">" + categoryId]);
        if (refuse !== "") {
            return ({ code: refuse });
        }
        const group = groups.find(candidate => candidate.key === key);
        groups = groups.filter(candidate => candidate.key !== key);
        changed();
        return ({ sorted: group === undefined ? 0 : group.count });
    }
}
