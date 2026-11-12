// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Stands in for the Rust PeriodRange in the QML checks: "a-b" is the range
// a..b, anything else is not understood. Rolling words are written and
// read like the Rust side writes them (D-644, D-645).

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
        const months = /^last-([1-9][0-9]*)-(complete-)?months$/.exec(word);
        if (months !== null) {
            return {custom: false, text: "", rolling: "months", count: Number(months[1]), complete: months[2] !== undefined, day: 0, before: false};
        }
        const days = /^last-([1-9][0-9]*)-days$/.exec(word);
        if (days !== null) {
            return {custom: false, text: "", rolling: "days", count: Number(days[1]), complete: false, day: 0, before: false};
        }
        const salary = /^(this|last)-salary-month-([1-9][0-9]*)$/.exec(word);
        if (salary !== null) {
            return {custom: false, text: "", rolling: "salary", count: 0, complete: false, day: Number(salary[2]), before: salary[1] === "last"};
        }
        const at = word.indexOf("..");
        return at < 0 ? {custom: false, text: "", rolling: ""} : {custom: true, text: word.slice(0, at) + "-" + word.slice(at + 2), rolling: ""};
    }

    function months(count, complete) {
        return count < 1 || count > 120 ? "" : "last-" + count + (complete ? "-complete-months" : "-months");
    }

    function days(count) {
        return count < 1 || count > 366 ? "" : "last-" + count + "-days";
    }

    function salaryMonth(day, before) {
        return day < 1 || day > 31 ? "" : (before ? "last" : "this") + "-salary-month-" + day;
    }

    function example() {
        return "01.01.2000 – 01.01.2020";
    }
}
