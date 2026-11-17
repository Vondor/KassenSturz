// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Gives the drawer buttons in the pages' headers a name for a screen reader
// (B-376). In a wide window Kirigami doesn't show the drawers' own handles
// but a button of its own in each page's header, next to the title for the
// menu and at the right for the chat. That button has an icon and a tooltip
// but no text, so Orca read "button" and nothing else. The headers are made
// by Kirigami for each page, so they are looked for whenever the pages
// change, and each such button gets its drawer's words as its name, the
// same words its tooltip shows (DrawerHandleName.qml does it for the
// handles of a narrow window).

import QtQuick

Item {
    id: names

    visible: false
    width: 0
    height: 0

    // The window's page stack, whose changes make new headers.
    required property Item pages
    // Where the headers are looked for: the whole window, its header too.
    property Item scope: pages

    // The buttons named so far, for the smoke test.
    property var named: []

    // Kirigami makes a page's header a moment after the page, sometimes
    // later still (its loader is asynchronous): look a few times.
    property int looksLeft: 0

    function lookSoon() {
        names.looksLeft = 4;
        looker.restart();
    }

    // The drawer's words for what its button does now.
    function words(drawer) {
        return drawer ? (drawer.drawerOpen ? drawer.handleOpenToolTip : drawer.handleClosedToolTip) : "";
    }

    function look(item) {
        if (item === null || item === undefined) {
            return;
        }
        const handle = item.drawer !== undefined && item.action !== undefined && item.icon !== undefined;
        if (handle && names.named.indexOf(item) < 0) {
            names.named = names.named.concat([item]);
            nameBinding.createObject(item, {
                target: item,
                value: Qt.binding(() => names.words(item.drawer))
            });
        }
        const children = item.children ?? [];
        for (let i = 0; i < children.length; ++i) {
            look(children[i]);
        }
    }

    Component {
        id: nameBinding

        Binding {
            property: "Accessible.name"
        }
    }

    Timer {
        id: looker

        interval: 400
        onTriggered: {
            names.named = names.named.filter(button => button !== null);
            names.look(names.scope);
            names.looksLeft -= 1;
            if (names.looksLeft > 0) {
                restart();
            }
        }
    }

    Connections {
        target: names.pages

        function onCurrentItemChanged() {
            names.lookSoon();
        }

        function onDepthChanged() {
            names.lookSoon();
        }
    }

    Connections {
        target: names.pages?.layers ?? null

        function onDepthChanged() {
            names.lookSoon();
        }
    }

    Component.onCompleted: lookSoon()
}
