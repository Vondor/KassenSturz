// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object ComponentProperties, for
// `tools/check-panel.sh` (M6a.7): the same `view` shape and calls, with
// just enough behaviour to click through the Statistics page.

import QtQuick

QtObject {
    id: component

    property var view: ({})
    property string title: ""

    function shows(type) {
        return type === "kpi.card" ? "card" : type === "progress.bar" ? "progress" : type === "table.rows" ? "table" : "chart";
    }

    // The columns of a chart or table, as the real object keeps them (D-339).
    property string columnsJson: "[]"

    function build(type, dataset) {
        component.type = type;
        component.dataset = dataset;
        const hasColumns = type.startsWith("chart.") || type === "table.rows";
        const made = {
            type: type, dataset: dataset, datasets: [dataset],
            json: JSON.stringify({type: type, dataset: dataset, params: hasColumns ? {columns: JSON.parse(columnsJson)} : {}, props: {title: title}}),
            fields: [{key: "title", place: "props", kind: "text", text: title, flag: false, word: "", words: []}],
            preview: {shows: shows(type), chart: type.replace("chart.", "").replace("_", "-"), grouping: "each", compare: "previous_period", measure: "income", columns: columnsJson === "[]" ? "" : columnsJson}
        };
        if (hasColumns) {
            made.columns = columnsJson;
        }
        view = made;
    }

    property string type: ""
    property string dataset: ""

    function open(type, dataset) { title = ""; columnsJson = "[]"; build(type, dataset); }
    function load(json) { const c = JSON.parse(json); title = c.props.title; columnsJson = JSON.stringify(c.params?.columns ?? []); build(c.type, c.dataset); }
    function setType(type) { build(type, component.dataset); }
    function setDataset(dataset) { build(component.type, dataset); }
    function setText(key, value) { title = value; build(component.type, component.dataset); }
    function setColumns(json) { columnsJson = json; build(component.type, component.dataset); }
    function setFlag(key, value) { }
    function setWord(key, value) { }
    function setFormula(key, value) { }
}
