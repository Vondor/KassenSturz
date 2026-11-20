// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object StatementImport, for
// `tools/check-panel.sh` (StartBalanceSmoke, B-411): the test sets the
// view, as statement_import_model.rs builds it.

import QtQuick

QtObject {
    property var view: ({})

    signal done(var result)

    function importNow() { return ({}); }
    function setImportAnyway(key, on) { return ({}); }
    function cancel() {}
    function transferSuggestions() { return []; }
}
