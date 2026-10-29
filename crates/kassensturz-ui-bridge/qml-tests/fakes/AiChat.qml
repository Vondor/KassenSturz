// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object AiChat, for `tools/check-panel.sh`
// (M6a.9): one prepared set, with a new chart as the bridge writes it
// (`ChangeView::NewView`), and the calls the review page makes.

import QtQuick

QtObject {
    property var confirmed: []
    property var discarded: []

    function changes(setId) {
        return [{
            kind: "newView",
            title: "Where the money goes",
            period: "this-year",
            component: JSON.stringify({type: "chart.pie", dataset: "spending_by_category", params: {grouping: "category"}, props: {title: "Where the money goes"}})
        }];
    }

    function confirm(setId) {
        confirmed = confirmed.concat([setId]);
        return {ok: true};
    }

    function discard(setId) { discarded = discarded.concat([setId]); }
}
