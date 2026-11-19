// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: header handle names

// Loads HeaderHandleNames.qml (B-376) in a wide window, where Kirigami
// shows the drawers' buttons in the page's header instead of their handles,
// and checks that it finds the menu's and the sidebar's button there, and
// still after a page is pushed, so that each gets its drawer's words as its
// name, and that the sidebar's button shows those words, highlighted
// (B-385).
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 1280
    height: 720
    visible: true
    property bool failed: false
    property int step: 0

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    pageStack.columnView.columnResizeMode: Kirigami.ColumnView.SingleColumn
    pageStack.initialPage: Kirigami.Page {
        title: "Accounts"
    }

    globalDrawer: Kirigami.GlobalDrawer {
        modal: false
        drawerOpen: true
        handleVisible: true
        handleOpenToolTip: "Hide the menu"
        handleClosedToolTip: "Show the menu"
    }

    contextDrawer: Kirigami.OverlayDrawer {
        id: side

        edge: Qt.RightEdge
        modal: false
        drawerOpen: false
        handleVisible: true
        handleOpenToolTip: "Close the chat"
        handleClosedToolTip: "Open the chat"
        handleClosedIcon.name: "dialog-messages"
    }

    HeaderHandleNames {
        id: names

        pages: window.pageStack
        scope: window.contentItem.parent ?? window.contentItem
        labelled: side
    }

    Component {
        id: second

        Kirigami.Page {
            title: "Planned payments"
        }
    }

    Timer {
        interval: 2500
        repeat: true
        running: true
        onTriggered: {
            const drawers = names.named.map(button => button.drawer);
            switch (window.step) {
            case 0:
                window.check(drawers.indexOf(window.globalDrawer) >= 0, "the menu's button in the header is found (" + names.named.length + " found)");
                window.check(drawers.indexOf(side) >= 0, "the sidebar's button in the header is found");
                window.check(names.words(window.globalDrawer) === "Hide the menu" && names.words(side) === "Open the chat", "the words follow the drawers");
                const chat = names.named.filter(button => button.drawer === side);
                const menu = names.named.filter(button => button.drawer === window.globalDrawer);
                // The chat's button shows its words beside the icon and stands
                // out (B-385, D-861); the menu's stays an icon.
                window.check(chat.length > 0 && chat.every(button => button.text === "Open the chat" && button.highlighted === true), "the sidebar's button shows its words, highlighted");
                // Its tooltip is its own drawer's words, not the menu's, and
                // it has a frame while closed (B-401).
                window.check(chat.every(button => button.action.tooltip === "Open the chat"), "the sidebar's tooltip is its own: " + chat.map(button => button.action.tooltip));
                window.check(chat.every(button => button.flat === false), "the sidebar's button has a frame while closed");
                window.check(chat.every(button => button.icon.name === "dialog-messages"), "the sidebar's button shows the speech bubble: " + chat.map(button => button.icon.name));
                window.check(menu.every(button => button.text === "" && button.highlighted !== true), "the menu's button stays a plain icon");
                window.step = names.named.length;
                window.pageStack.push(second);
                break;
            default:
                const now = names.named.map(button => button.drawer);
                window.check(names.named.length >= window.step && now.indexOf(window.globalDrawer) >= 0, "after a push the buttons are still named: " + names.named.length);
                Qt.exit(window.failed ? 1 : 0);
            }
            if (window.step === 0) {
                window.step = 1;
            }
        }
    }
}
