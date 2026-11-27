// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: configurable AI chat buttons

// Opens AiChatButtonsSettings.qml and AiChatPanel.qml on stand-ins (fakes/)
// and checks US11.3 (#1452, D-1013):
// - Built-in toggles hide and show built-in chat buttons immediately (while
//   still respecting permissions).
// - Custom buttons render enabled-only in display order with plain text (T-08).
// - insertOnly: false sends immediately via chat.ask; insertOnly: true
//   prefills the input box without sending.
// - Reorder, edit, toggle, delete, and the inbox preset helper work.
// - Switching AI off hides both components and clears all buttons (D-013).
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 1000
    height: 900
    visible: true
    property bool failed: false
    property var savedResults: []

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function find(item, name, seen) {
        if (item === null || item === undefined) {
            return null;
        }
        const visited = seen ?? [];
        if (visited.indexOf(item) >= 0) {
            return null;
        }
        visited.push(item);
        if (item.objectName === name) {
            return item;
        }
        const lists = [item.children, item.data];
        for (let l = 0; l < lists.length; ++l) {
            const list = lists[l];
            if (list === undefined || list === null) {
                continue;
            }
            for (let i = 0; i < list.length; ++i) {
                const found = window.find(list[i], name, visited);
                if (found !== null) {
                    return found;
                }
            }
        }
        if (item.contentItem !== undefined && item.contentItem !== null) {
            return window.find(item.contentItem, name, visited);
        }
        return null;
    }

    function findByTest(item, test) {
        if (item === null || item === undefined) {
            return null;
        }
        if (test(item)) {
            return item;
        }
        const children = item.children ?? [];
        for (let i = 0; i < children.length; ++i) {
            const found = window.findByTest(children[i], test);
            if (found !== null) {
                return found;
            }
        }
        return null;
    }

    AiSwitch {
        id: aiSwitch
    }

    AiChat {
        id: chat
    }

    AiSettings {
        id: settings

        allowed: ({
            AllowSortTransactions: true,
            AllowReadCategories: true,
            AllowReadTransactions: true,
            AllowNewRules: true,
            AllowNewViews: true
        })
        receiptFolders: ({
            inbox: "/home/sam/Inbox",
            archive: "/home/sam/Archive"
        })
    }

    pageStack.initialPage: Kirigami.Page {
        Item {
            anchors.fill: parent

            AiChatButtonsSettings {
                id: buttonsSettings

                anchors.top: parent.top
                anchors.left: parent.left
                width: 460
                ai: aiSwitch
                settings: settings
                onSaved: ok => window.savedResults = window.savedResults.concat([ok])
            }

            AiChatPanel {
                id: panel

                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: parent.right
                width: 460
                ai: aiSwitch
                chat: chat
                settings: settings
            }
        }
    }

    Timer {
        interval: 400
        running: true
        onTriggered: {
            // 1. All 4 built-in buttons start ON by default when permissions allow them.
            window.check(panel.features.length === 3, "all 3 feature buttons start shown, got " + JSON.stringify(panel.features));
            window.check(panel.chartAllowed === true, "describe chart starts shown when allowed");

            // Toggling built-in buttons off hides them in AiChatPanel immediately.
            const catSwitch = window.find(buttonsSettings, "builtinButtonSwitch_categories");
            const chartSwitch = window.find(buttonsSettings, "builtinButtonSwitch_describe_chart");
            window.check(catSwitch !== null && chartSwitch !== null, "built-in switches are found");
            if (catSwitch !== null) {
                catSwitch.checked = false;
                catSwitch.toggled();
            }
            window.check(panel.features.indexOf("categories") < 0, "categories button hidden when switched off");
            window.check(panel.features.indexOf("month") >= 0 && panel.features.indexOf("rules") >= 0, "other built-in buttons stay visible");

            if (chartSwitch !== null) {
                chartSwitch.checked = false;
                chartSwitch.toggled();
            }
            window.check(!panel.chartAllowed, "describe chart hidden when switched off");
            const describeBtn = window.find(panel, "builtinChatButton_describe_chart");
            window.check(describeBtn !== null && !describeBtn.visible, "describe chart button control is hidden");

            // Turning back on shows it immediately, still respecting permissions.
            if (chartSwitch !== null) {
                chartSwitch.checked = true;
                chartSwitch.toggled();
            }
            window.check(panel.chartAllowed, "describe chart returns when switched back on");
            settings.setAllows("AllowNewViews", false);
            window.check(!panel.chartAllowed, "describe chart still respects missing permission even when toggle is on");
            settings.setAllows("AllowNewViews", true);
            window.check(panel.chartAllowed, "describe chart returns when permission is restored");

            // 2. Custom buttons: add send-now, insert-only, and disabled buttons.
            const addedSend = settings.addCustomChatButton("<b>Weekly</b>", "Summarize my spending this week", false);
            const addedInsert = settings.addCustomChatButton("Check shop", "How much did I spend at ", true);
            const addedDisabled = settings.addCustomChatButton("Hidden button", "Never shown", false);
            window.check(addedSend.ok === true && addedInsert.ok === true && addedDisabled.ok === true, "custom buttons added");
            settings.setCustomChatButtonEnabled(addedDisabled.id, false);

            window.check(panel.customButtons.length === 2, "panel renders enabled custom buttons only, got " + panel.customButtons.length);
            window.check(panel.customButtons[0].id === addedSend.id && panel.customButtons[1].id === addedInsert.id, "enabled custom buttons keep order");

            // T-08: HTML-looking label goes through Plain.plain() on the button.
            const sendBtn = window.find(panel, "customChatButton_" + addedSend.id);
            const insertBtn = window.find(panel, "customChatButton_" + addedInsert.id);
            const hiddenBtn = window.find(panel, "customChatButton_" + addedDisabled.id);
            window.check(sendBtn !== null && insertBtn !== null, "enabled custom buttons exist in panel");
            window.check(hiddenBtn === null, "disabled custom button is not rendered in panel");
            if (sendBtn !== null) {
                window.check(sendBtn.text.indexOf("<b>") < 0, "custom button label is escaped via Plain.plain, got " + sendBtn.text);
            }

            // 3. Clicking insertOnly: false sends immediately via chat.ask().
            const questionField = window.findByTest(panel, item => item.Accessible.name === "Your question for the AI" && item.cursorPosition !== undefined);
            window.check(questionField !== null, "question field found");
            chat.asked = [];
            if (sendBtn !== null) {
                sendBtn.clicked();
            }
            window.check(chat.asked.length === 1 && chat.asked[0] === "Summarize my spending this week", "insertOnly: false sends immediately, got " + JSON.stringify(chat.asked));
            if (questionField !== null) {
                window.check(questionField.text === "", "input box stays empty after send-now custom button");
            }

            // 4. Clicking insertOnly: true prefills input without sending.
            if (insertBtn !== null) {
                insertBtn.clicked();
            }
            window.check(chat.asked.length === 1, "insertOnly: true does not call chat.ask");
            if (questionField !== null) {
                window.check(questionField.text === "How much did I spend at", "insertOnly: true prefills input box, got '" + questionField.text + "'");
                window.check(panel.suggested === true, "prefilled prompt is marked as editable suggestion");
            }

            // 5. Exercise actual delegate controls (toggle, moveUp, moveDown,
            //    delete) and verify saved(true) fires without ReferenceError.
            window.savedResults = [];
            const customSwitch = window.find(buttonsSettings, "customButtonSwitch_" + addedSend.id);
            window.check(customSwitch !== null, "custom button delegate switch found");
            if (customSwitch !== null) {
                customSwitch.checked = false;
                customSwitch.toggled();
            }
            window.check(panel.customButtons.length === 1 && panel.customButtons[0].id === addedInsert.id, "toggling custom switch off hides it in panel");
            window.check(window.savedResults.length === 1 && window.savedResults[0] === true, "toggleCustom emits saved(true), got " + JSON.stringify(window.savedResults));

            const customSwitchAgain = window.find(buttonsSettings, "customButtonSwitch_" + addedSend.id);
            window.check(customSwitchAgain !== null, "custom button delegate switch found after reload");
            if (customSwitchAgain !== null) {
                customSwitchAgain.checked = true;
                customSwitchAgain.toggled();
            }
            window.check(panel.customButtons.length === 2, "toggling custom switch back on shows it again");
            window.check(window.savedResults.length === 2 && window.savedResults[1] === true, "toggleCustom on emits saved(true)");

            const moveUpBtn = window.find(buttonsSettings, "moveUpButton_" + addedInsert.id);
            window.check(moveUpBtn !== null && moveUpBtn.enabled, "moveUp delegate button found and enabled");
            if (moveUpBtn !== null) {
                moveUpBtn.clicked();
            }
            window.check(panel.customButtons[0].id === addedInsert.id, "clicking moveUp moves button up immediately");
            window.check(window.savedResults.length === 3 && window.savedResults[2] === true, "moveUp emits saved(true)");

            const moveDownBtn = window.find(buttonsSettings, "moveDownButton_" + addedInsert.id);
            window.check(moveDownBtn !== null && moveDownBtn.enabled, "moveDown delegate button found and enabled");
            if (moveDownBtn !== null) {
                moveDownBtn.clicked();
            }
            window.check(panel.customButtons[0].id === addedSend.id, "clicking moveDown moves button back down immediately");
            window.check(window.savedResults.length === 4 && window.savedResults[3] === true, "moveDown emits saved(true)");

            const deleteBtn = window.find(buttonsSettings, "deleteButton_" + addedDisabled.id);
            window.check(deleteBtn !== null, "delete delegate button found");
            if (deleteBtn !== null) {
                deleteBtn.clicked();
            }
            window.check(buttonsSettings.customButtons.length === 2, "clicking delete removes the button, got " + buttonsSettings.customButtons.length);
            window.check(window.savedResults.length === 5 && window.savedResults[4] === true, "delete emits saved(true)");

            // Inbox preset helper.
            const presetBtn = window.find(buttonsSettings, "addInboxPresetButton");
            window.check(presetBtn !== null && presetBtn.visible, "inbox preset button is visible when inbox and archive are configured");
            if (presetBtn !== null) {
                presetBtn.clicked();
            }
            window.check(panel.customButtons.length === 3, "inbox preset button creates a ready-made enabled custom button");

            // 6. D-013: open editorDialog, populate fields, then switch AI off:
            //    dialog closes and clears, both components hide, lists clear.
            const addBtn = window.find(buttonsSettings, "addCustomButton");
            const dialog = window.find(buttonsSettings, "customButtonDialog");
            const labelField = window.find(buttonsSettings, "customButtonLabelField");
            const promptScroll = window.find(buttonsSettings, "customButtonPromptScroll");
            const promptField = window.find(buttonsSettings, "customButtonPromptField");
            window.check(addBtn !== null && dialog !== null && labelField !== null && promptScroll !== null && promptField !== null, "editor dialog and scrollable prompt field found");
            if (addBtn !== null && labelField !== null && promptField !== null) {
                addBtn.clicked();
                labelField.text = "Draft label";
                promptField.text = "Line 1\nLine 2\nLine 3";
                window.check(dialog.visible, "editor dialog opened");
            }

            aiSwitch.switchTo(false);
            if (dialog !== null && labelField !== null && promptField !== null) {
                window.check(!dialog.opened, "AI off closes open editor dialog immediately (D-013)");
                window.check(labelField.text === "" && promptField.text === "", "AI off clears editor dialog fields (D-013)");
            }
            window.check(!panel.visible && !buttonsSettings.visible, "AI off hides both chat panel and button settings");
            window.check(panel.features.length === 0 && !panel.chartAllowed && panel.customButtons.length === 0, "AI off clears all rendered buttons");
            window.check(buttonsSettings.customButtons.length === 0, "AI off clears settings custom button list");

            closeCheckTimer.start();
        }
    }

    Timer {
        id: closeCheckTimer

        interval: 350
        onTriggered: {
            const dialog = window.find(buttonsSettings, "customButtonDialog");
            window.check(dialog !== null && !dialog.visible, "editor dialog is fully hidden after close transition (D-013)");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
