// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Opens CategoryGroupsPage.qml and CategoryGroupPage.qml on a stand-in for
// the Rust object CategoryGroups (fakes/) and checks that the pages list the
// groups and the categories, that ticking a category to hide sends the whole
// hide list, and that a group is made and changed with the members the user
// ticked, in order, of its own kind only (M6b.5c). `tools/check-panel.sh`
// runs it and fails on the first FAIL line or on any QML warning about a
// binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 640
    height: 900
    visible: true
    property bool failed: false

    CategoryGroups {
        id: fake
    }

    Component {
        id: groupPage

        CategoryGroupPage {
            groups: fake
        }
    }

    pageStack.initialPage: CategoryGroupsPage {
        id: page

        groups: fake
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function find(item, type, out) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child.toString().startsWith(type + "_") || child.toString().startsWith(type + "(")) {
                out.push(child);
            }
            find(child, type, out);
        }
        return out;
    }

    function texts(item, out) {
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (child.visible && typeof child.text === "string") {
                out.push(child.text);
            }
            texts(child, out);
        }
        return out;
    }

    // Every check box below `item`, whatever style makes them.
    function boxes(item) {
        const out = [];
        const walk = node => {
            for (let i = 0; i < node.children.length; ++i) {
                const child = node.children[i];
                if (child.toString().includes("CheckBox")) {
                    out.push(child);
                }
                walk(child);
            }
        };
        walk(item);
        return out;
    }

    // What a click does, also on a page the window doesn't show yet.
    function flip(box) {
        box.checked = !box.checked;
        box.toggled();
    }

    function fields(item) {
        const out = [];
        const walk = node => {
            for (let i = 0; i < node.children.length; ++i) {
                const child = node.children[i];
                if (child.toString().includes("TextField")) {
                    out.push(child);
                }
                walk(child);
            }
        };
        walk(item);
        return out;
    }

    Timer {
        interval: 600
        running: true
        onTriggered: {
            const all = window.texts(page, []);
            window.check(all.includes("Fixed costs"), "the group is listed");
            window.check(all.includes("Pay"), "the income group is listed too");
            // One box for every category; Food is hidden.
            const hide = window.boxes(page);
            window.check(hide.length === 4, "a box for each of the four categories, found " + hide.length);
            window.check(hide[0].checked && !hide[1].checked, "only Food is hidden");
            hide[2].click();
            window.check(fake.calls[0] === "setHidden:c-food,c-rent", "the whole hide list is sent, got " + fake.calls[0]);
            // The list is read again after a change, so the boxes are new ones.
            window.boxes(page)[0].click();
            window.check(fake.calls[1] === "setHidden:c-rent", "taking one out sends the rest, got " + fake.calls[1]);

            // A new group of money spent: only spending categories to pick.
            const adding = window.pageStack.push(groupPage);
            window.check(adding.adding, "the page makes a group");
            const picks = window.boxes(adding);
            // Three spending categories and the group of money spent.
            window.check(picks.length === 4, "three categories and a group to pick, found " + picks.length);
            window.flip(picks[2]);
            window.flip(picks[0]);
            const field = window.fields(adding)[0];
            field.text = "Household";
            adding.save();
            window.check(fake.calls[2] === "addGroup:Household:false:c-rent,c-food", "made with what was ticked, got " + fake.calls[2]);
            adding.groupName = "";
            field.text = "  ";
            adding.save();
            window.check(window.texts(adding, []).includes("Give it a name."), "an empty name says what to do");

            // Changing a group keeps its members' places and adds the new ones.
            const changing = window.pageStack.push(groupPage, {groupId: "g-fixed", groupName: "Fixed costs", income: false, memberIds: ["c-rent"]});
            const edits = window.boxes(changing);
            window.check(edits[2].checked && !edits[0].checked, "Rent is ticked, Food is not");
            window.flip(edits[1]);
            changing.save();
            window.check(fake.calls[4] === "editGroup:g-fixed:Fixed costs:c-rent,c-groceries", "changed, got " + fake.calls[4]);
            // A group of money received offers only income categories.
            const income = window.pageStack.push(groupPage, {groupId: "g-pay", groupName: "Pay", income: true, memberIds: []});
            window.check(window.boxes(income).length === 1, "one income category to pick");
            console.log(window.failed ? "groups: FAILED" : "groups: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
