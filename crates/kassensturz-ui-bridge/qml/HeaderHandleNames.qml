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
//
// The button of the `labelled` drawer (the chat's, B-385, D-861) also shows
// those words beside its icon while the drawer is closed, and is
// highlighted, so it reads as "Ask AI" and not as a bare icon. While the
// drawer is open it is the plain icon that closes it. Kirigami's header
// button takes its tooltip from the menu's drawer whatever drawer it opens,
// so the chat's showed "Show menu" (B-401): it gets its own drawer's words,
// and a frame while closed, since a flat button's highlight barely shows.

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

    // The drawer whose button shows its words and is highlighted; null
    // for none.
    property var labelled: null

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
            if (item.drawer === names.labelled && names.labelled !== null) {
                textBinding.createObject(item, {
                    target: item,
                    value: Qt.binding(() => item.drawer.drawerOpen ? "" : item.drawer.handleClosedToolTip)
                });
                highlightBinding.createObject(item, {
                    target: item,
                    value: Qt.binding(() => !item.drawer.drawerOpen)
                });
                flatBinding.createObject(item, {
                    target: item,
                    value: Qt.binding(() => item.drawer.drawerOpen)
                });
                if (item.action !== null && item.action.tooltip !== undefined) {
                    tooltipBinding.createObject(item, {
                        target: item.action,
                        value: Qt.binding(() => names.words(item.drawer))
                    });
                }
            }
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

    Component {
        id: textBinding

        Binding {
            property: "text"
        }
    }

    Component {
        id: highlightBinding

        Binding {
            property: "highlighted"
        }
    }

    Component {
        id: flatBinding

        Binding {
            property: "flat"
        }
    }

    Component {
        id: tooltipBinding

        Binding {
            property: "tooltip"
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
