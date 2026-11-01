// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton

import QtQuick
import io.github.vondor.kassensturz

// Every handler of an action (a click, a menu entry, a switch, a choice, a
// confirmation) says first that it ran, `ActionLog.taken("RulesPage_addRule")`:
// in the developer mode (D-227) the debug log then shows whether a click
// reached its handler, so a button that "does nothing" can be told from a
// handler that fails (M6c.D, D-419). The name is one of the fixed names of
// `kassensturz_core::vocabulary::ACTIONS`; `action_log_audit` checks that
// every handler has one. Outside the developer mode the call does nothing.
QtObject {
    id: log

    readonly property DeveloperMode mode: DeveloperMode {}

    function taken(name: string): void {
        mode.actionTaken(name);
    }
}
