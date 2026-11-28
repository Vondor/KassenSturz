// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: change correction (editTransaction and splitTransaction)
// check-panel-desktop: change correction desktop style

// Exercises AiChangeCorrection.qml inside AiChangeReviewPage.qml for both
// editTransaction and splitTransaction cards (US11.2, D-1016):
// - editTransaction shows only unlocked fields (here Note, with Payee hidden),
//   starts from the proposed note, saves an edited note, marks "Changed by you",
//   and supports "Back to the AI's version".
// - splitTransaction lists compatible same-currency categories (excluding USD)
//   plus "Not sorted yet" and compatible new_category choices, preserves exact
//   decimal amount strings without float conversion, swaps a part off a
//   proposed new_category so that new_category can then be left out cleanly,
//   and hides per-part name inputs when noteUnlocked is false.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 700
    height: 900
    visible: true
    property bool failed: false

    AiSwitch {
        id: ai
    }

    AiChat {
        id: chat
    }

    pageStack.initialPage: AiChangeReviewPage {
        id: page

        ai: ai
        chat: chat
        setId: "correction"
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

    Timer {
        interval: 500
        running: true
        onTriggered: {
            window.check(page.newCategories.length === 1, "one proposed new category");
            window.check(page.editedTransactions.length === 1, "one editTransaction card");
            window.check(page.splits.length === 1, "one splitTransaction card");

            const corrections = window.find(page, "AiChangeCorrection", []);
            window.check(corrections.length === 3, "three AiChangeCorrection rows, got " + corrections.length);

            const editCorr = corrections.find(c => c.kind === "editTransaction");
            const splitCorr = corrections.find(c => c.kind === "splitTransaction");
            window.check(editCorr !== undefined && splitCorr !== undefined, "both edit and split corrections present");

            // 1. editTransaction: Note unlocked, Payee locked.
            window.check(editCorr.editable, "editTransaction is editable");
            window.check(!editCorr.canEditPayee && editCorr.canEditNote, "only unlocked note field is editable");
            editCorr.start();
            window.check(editCorr.editing, "editTransaction form opens");
            window.check(editCorr.fields().note === "Order 104 · Pens", "starts from AI note");
            window.check(editCorr.fields().payee === undefined, "locked payee omitted from fields");

            // Save edited note through review.editChange and verify "Changed by you" + backToAi.
            window.check(!page.editChange("edit_transactions/0", { note: "Line1\nLine2" }), "multiline note refused");
            window.check(page.correctionResult.code === "ai-changes.edit-text", "multiline note error code");
            window.check(page.editChange("edit_transactions/0", { note: "Order 104 · Fountain pens" }), "single-line note accepted");
            window.check(page.editedTransactions[0].edited, "editTransaction marked edited");
            window.check(window.texts(page, []).includes("Changed by you"), "Changed by you shown");
            window.check(page.backToAi("edit_transactions/0"), "editTransaction backToAi succeeds");
            window.check(!page.editedTransactions[0].edited, "editTransaction restored to AI version");

            // 2. splitTransaction: compatible EUR categories only (USD filtered out).
            const splitCorr2 = window.find(page, "AiChangeCorrection", []).find(c => c.kind === "splitTransaction");
            window.check(splitCorr2.editable, "splitTransaction is editable");
            window.check(splitCorr2.categories.length === 3, "3 EUR categories (Food, Office supplies, Gadgets), got " + splitCorr2.categories.length);
            window.check(splitCorr2.categories.every(c => c.currency === "EUR"), "only EUR categories offered");

            splitCorr2.start();
            window.check(splitCorr2.editing, "splitTransaction form opens");
            const initialParts = splitCorr2.fields().parts;
            window.check(initialParts.length === 2, "two split parts in form");
            window.check(initialParts[0].amount === "-12.50" && initialParts[1].amount === "-7.49", "exact amount strings preserved");
            window.check(initialParts[0].new_category === "gadgets", "part 0 starts on new_category gadgets");
            window.check(initialParts[0].name === "Mouse" && initialParts[1].name === "Snack", "part names loaded");

            // Leaving out gadgets while part 0 still uses it is refused with edit-in-use.
            window.check(!page.leaveOut("new_categories/0"), "gadgets still in use");
            window.check(page.correctionResult.code === "ai-changes.edit-in-use", "in-use error reported");

            // Swap part 0 off "gadgets" to existing "Office supplies" and part 1 to "Not sorted yet".
            window.check(page.editChange("split_transactions/0", {
                parts: [
                    { category_id: "cat-office", amount: "-12.50", name: "USB Mouse" },
                    { amount: "-7.49", name: "Snack" }
                ]
            }), "splitTransaction edit accepted");
            window.check(page.splits[0].edited, "splitTransaction marked edited");
            window.check(page.splits[0].parts[0].category === "Office supplies", "part 0 updated to Office supplies");
            window.check(page.splits[0].parts[1].category === "", "part 1 updated to Not sorted yet");

            // Now leaving out gadgets succeeds cleanly!
            window.check(page.leaveOut("new_categories/0"), "gadgets left out cleanly after swap");
            window.check(page.newCategories.length === 0 && page.leftOut.length === 1, "gadgets moved to leftOut");

            // 3. With noteUnlocked=false, part name inputs are omitted from submitted fields.
            chat.correctionSplitNoteUnlocked = false;
            page.changes = chat.changes("correction");
            const splitCorr3 = window.find(page, "AiChangeCorrection", []).find(c => c.kind === "splitTransaction");
            splitCorr3.start();
            const lockedParts = splitCorr3.fields().parts;
            window.check(lockedParts[0].name === undefined && lockedParts[1].name === undefined, "part names omitted when noteUnlocked is false");

            console.log(window.failed ? "change-correction: FAILED" : "change-correction: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
