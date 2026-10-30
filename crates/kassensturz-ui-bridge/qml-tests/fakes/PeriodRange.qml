// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Stands in for the Rust PeriodRange in the QML checks: "a-b" is the range
// a..b, anything else is not understood.

import QtQuick

QtObject {
    function parse(input) {
        const parts = input.split("-");
        if (parts.length === 2 && parts[0] !== "" && parts[1] !== "") {
            return {word: parts[0] + ".." + parts[1]};
        }
        return {code: "input.bad-range-text"};
    }

    function describe(word) {
        const at = word.indexOf("..");
        return at < 0 ? {custom: false, text: ""} : {custom: true, text: word.slice(0, at) + "-" + word.slice(at + 2)};
    }

    function example() {
        return "01.01.2000 – 01.01.2020";
    }
}
