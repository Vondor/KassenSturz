// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Gives the button of a drawer's handle its words as its name, so a screen
// reader says what it does (B-376). Kirigami's handle names the button only
// through its tooltip, and Orca read it as a button with no name at all, so
// the name is set on the button itself, with the tooltip's own words
// (`handleOpenToolTip` and `handleClosedToolTip`, which Main.qml gives in
// the user's language).

import QtQuick
import QtQuick.Controls as Controls

QtObject {
    id: name

    // The drawer, a Kirigami.OverlayDrawer or one of its kinds.
    required property var drawer

    // The handle's button; null where Kirigami builds the handle otherwise.
    readonly property var button: {
        const first = name.drawer?.handle?.children?.[0];
        return first !== undefined && first.display !== undefined ? first : null;
    }
    // What the button does now.
    readonly property string words: name.drawer
        ? (name.drawer.drawerOpen ? name.drawer.handleOpenToolTip : name.drawer.handleClosedToolTip)
        : ""

    // A document can't fill a default property it declares itself, so each
    // binding has a property of its own.
    property Binding nameBinding: Binding {
        target: name.button
        property: "Accessible.name"
        value: name.words
    }

    property Binding textBinding: Binding {
        target: name.button
        property: "text"
        value: name.words
    }

    property Binding displayBinding: Binding {
        target: name.button
        property: "display"
        value: Controls.AbstractButton.IconOnly
    }
}
