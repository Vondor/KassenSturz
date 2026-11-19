// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: drawer handle names

// Loads DrawerHandleName.qml (B-376) and checks that the button of a
// drawer's handle gets the drawer's words as its accessible name and its
// text: on a stand-in handle whose button reads its own name, the way
// Kirigami's handle binds its name to its tooltip, and on Kirigami's real
// handles of a menu and a sidebar, open and closed, and that the sidebar's
// handle is highlighted while it is closed (B-385). `tools/check-panel.sh`
// runs it and fails on the first FAIL line or on any QML warning about a
// binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 800
    height: 600
    visible: true
    property bool failed: false

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    globalDrawer: Kirigami.GlobalDrawer {
        modal: true
        handleVisible: true
        handleOpenToolTip: "Hide the menu"
        handleClosedToolTip: "Show the menu"
    }

    contextDrawer: Kirigami.OverlayDrawer {
        id: side

        edge: Qt.RightEdge
        modal: true
        handleVisible: true
        handleOpenToolTip: "Close the chat"
        handleClosedToolTip: "Open the chat"
    }

    // A handle like Kirigami's: the button's name is its tooltip, which is
    // empty here, and the button reads its own name back.
    Item {
        id: fakeHandle

        Controls.ToolButton {
            id: own

            readonly property string seen: Accessible.name

            Accessible.name: Controls.ToolTip.text
            Controls.ToolTip.text: ""
        }
    }

    QtObject {
        id: fakeDrawer

        readonly property Item handle: fakeHandle
        property bool drawerOpen: false
        readonly property string handleOpenToolTip: "Hide it"
        readonly property string handleClosedToolTip: "Show it"
    }

    DrawerHandleName {
        id: fakeName

        drawer: fakeDrawer
    }

    DrawerHandleName {
        id: menuName

        drawer: window.globalDrawer
    }

    DrawerHandleName {
        id: sideName

        drawer: side
        highlighted: true
    }

    Timer {
        interval: 500
        running: true
        onTriggered: {
            window.check(fakeName.button === own, "the stand-in's button is found");
            window.check(own.seen === "Show it", "its name is the drawer's words: '" + own.seen + "'");
            window.check(own.text === "Show it" && own.display === Controls.AbstractButton.IconOnly, "and its text, shown as an icon only");
            fakeDrawer.drawerOpen = true;
            window.check(own.seen === "Hide it", "the name follows the drawer: '" + own.seen + "'");

            window.check(menuName.button !== null, "the menu's handle has a button");
            window.check(sideName.button !== null, "the sidebar's handle has a button");
            if (menuName.button !== null && sideName.button !== null) {
                window.check(menuName.button.text === (window.globalDrawer.drawerOpen ? "Hide the menu" : "Show the menu"), "the menu's button: " + menuName.button.text);
                window.check(sideName.button.text === "Open the chat", "the sidebar's button: " + sideName.button.text);
                // The chat's handle stands out while closed (B-385); the menu's doesn't.
                window.check(sideName.button.highlighted === true && menuName.button.highlighted !== true, "only the sidebar's closed handle is highlighted");
            }
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
