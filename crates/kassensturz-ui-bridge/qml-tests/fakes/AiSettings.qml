// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// A stand-in for the Rust object AiSettings, for `tools/check-panel.sh`
// (#631): the AI may change transactions, and amount, date and payee are
// locked, as at the first start (D-252).

import QtQuick

QtObject {
    property var allowed: ({ AllowSortTransactions: true })
    property var lockedKeys: ({ LockAmount: true, LockDate: true, LockPayee: true })
    // Every call of setLocked, as [key, value].
    property var lockCalls: []

    function allows(key) { return allowed[key] === true; }
    function setAllows(key, value) {
        const next = Object.assign({}, allowed);
        next[key] = value;
        allowed = next;
        allowsChanged();
        return true;
    }
    // As AiSettings says a saved permission (B-402).
    signal allowsChanged()
    // For AiSettingsPage and the chat panel (B-386, B-396): a provider on
    // this computer is saved with a model. ModelChoiceSmoke sets its own
    // form without one.
    property bool testing: false
    property bool fake: false
    property var form: ({ address: "http://localhost:1234/v1", model: "m", keyUse: "none", place: "this", consent: "not-needed" })
    signal testFinished(var result)
    function load() { return form; }
    // An address with "192.168." is on another computer at home, as
    // AiSetupSmoke needs (B-391); every other one is on this computer.
    function check(address, allowUnencrypted) {
        if (address.indexOf("192.168.") < 0) {
            return ({ place: "this", network: "this", unencrypted: false });
        }
        const plain = address.indexOf("http://") === 0;
        if (plain && !allowUnencrypted) {
            return ({ code: "ai.plain-http" });
        }
        return ({ place: "other", network: "home", unencrypted: plain });
    }
    function withScheme(address) { return address.trim(); }
    // Whether save and giveConsent change `form` as the real object's
    // load() would then read it (AiSetupSmoke, B-408).
    property bool keepsSaved: false
    function save(address, model, keyUse, key, allowUnencrypted) {
        calls = calls.concat(["save " + allowUnencrypted]);
        const remote = check(address, true).place === "other";
        if (keepsSaved) {
            const sameAddress = address === form.address;
            form = Object.assign({}, form, { address: address, model: model, keyUse: keyUse, allowUnencrypted: allowUnencrypted,
                place: remote ? "other" : "this",
                consent: remote ? (sameAddress && form.consent === "given" ? "given" : "missing") : "notNeeded" });
        }
        return ({ saved: true, consentNeeded: remote && form.consent !== "given" });
    }
    // How often the connection was tested, for ModelChoiceSmoke (B-407).
    property int testCalls: 0
    function testConnection(key) { testCalls += 1; calls = calls.concat(["test"]); }
    // A request as long as the real one, so a box that shows it can be
    // measured (B-431): taller than the ten lines of text the box once had.
    function capabilityRequest() {
        return JSON.stringify({ model: "chat-small", stream: false, messages: [{ role: "system", content: "Answer only by calling the function." }, { role: "user", content: "This is a made-up test question to see whether a model calls a made-up function." }], tools: [{ type: "function", function: { name: "test_function", description: "A function that only exists for this test.", parameters: { type: "object", properties: { word: { type: "string", description: "Any word." } }, required: ["word"] } } }], tool_choice: "auto" }, null, 2);
    }
    function statement() { return []; }
    function forgetKey() { return ({}); }
    // Every call of save and giveConsent, in order, for AiSetupSmoke.
    property var calls: []
    function giveConsent() {
        calls = calls.concat(["consent"]);
        if (keepsSaved) {
            form = Object.assign({}, form, { consent: "given", consentDay: "19.11.2026" });
        }
        return true;
    }
    function withdrawConsent() { return ({}); }
    // The choice not to be shown each request, for AiSetupSmoke (D-935).
    property var reviewOffCalls: []
    function setReviewOff(off) {
        reviewOffCalls = reviewOffCalls.concat([off]);
        form = Object.assign({}, form, { reviewOff: off });
        return true;
    }
    function sample(leaveOut) { return ({}); }
    function notes() { return ""; }
    function notesLimit() { return 2000; }
    function setNotes(text) { return true; }
    function notice(word) { return true; }
    function setNotice(word, value) { return true; }
    function slowAnswer() { return "warn"; }
    function setSlowAnswer(value) { return true; }
    function writeTools() { return false; }
    function setWriteTools(value) { return true; }
    function writeToolsProblem() { return ""; }
    function billFolder() { return ""; }
    function setBillFolder(folder) { return ""; }
    function removeBillFolder() { return true; }
    property var receiptFolders: ({ inbox: "", archive: "" })
    function receiptFolder(which) { return receiptFolders[which] ?? ""; }
    function setReceiptFolder(which, folder) { return ""; }
    function removeReceiptFolder(which) { return true; }
    function serviceKinds() { return []; }
    function serviceKind() { return ""; }
    function setServiceKind(id) { return true; }

    // Configurable built-in and custom chat buttons (US11.3, D-1013).
    property var builtinButtons: ({ categories: true, month: true, rules: true, describe_chart: true })
    property var customButtonsList: []
    property int nextButtonId: 1
    signal chatButtonsChanged()

    function builtinChatButton(word) { return builtinButtons[word] !== false; }
    function setBuiltinChatButton(word, shown) {
        const next = Object.assign({}, builtinButtons);
        next[word] = shown;
        builtinButtons = next;
        chatButtonsChanged();
        return true;
    }
    function customChatButtons() { return customButtonsList.slice(); }
    function chatButtonLimits() { return ({ maxCount: 20, maxLabelChars: 60, maxPromptChars: 2000 }); }
    function addCustomChatButton(label, prompt, insertOnly) {
        const cleanLabel = label.trim();
        const cleanPrompt = prompt.trim();
        if (cleanLabel === "" || cleanLabel.indexOf("\n") >= 0) {
            return ({ ok: false, reason: "invalid-label" });
        }
        if (cleanPrompt === "") {
            return ({ ok: false, reason: "invalid-prompt" });
        }
        if (customButtonsList.length >= 20) {
            return ({ ok: false, reason: "limit" });
        }
        const id = "btn-" + nextButtonId;
        nextButtonId += 1;
        customButtonsList = customButtonsList.concat([{
            id: id,
            label: cleanLabel,
            prompt: cleanPrompt,
            enabled: true,
            insertOnly: insertOnly === true
        }]);
        chatButtonsChanged();
        return ({ ok: true, id: id });
    }
    function updateCustomChatButton(id, label, prompt, enabled, insertOnly) {
        const cleanLabel = label.trim();
        const cleanPrompt = prompt.trim();
        if (cleanLabel === "" || cleanLabel.indexOf("\n") >= 0) {
            return ({ ok: false, reason: "invalid-label" });
        }
        if (cleanPrompt === "") {
            return ({ ok: false, reason: "invalid-prompt" });
        }
        const next = customButtonsList.slice();
        for (let i = 0; i < next.length; ++i) {
            if (next[i].id === id) {
                next[i] = { id: id, label: cleanLabel, prompt: cleanPrompt, enabled: enabled === true, insertOnly: insertOnly === true };
                customButtonsList = next;
                chatButtonsChanged();
                return ({ ok: true, id: id });
            }
        }
        return ({ ok: false, reason: "not-found" });
    }
    function setCustomChatButtonEnabled(id, enabled) {
        const next = customButtonsList.slice();
        for (let i = 0; i < next.length; ++i) {
            if (next[i].id === id) {
                next[i] = Object.assign({}, next[i], { enabled: enabled === true });
                customButtonsList = next;
                chatButtonsChanged();
                return true;
            }
        }
        return false;
    }
    function moveCustomChatButton(id, delta) {
        const next = customButtonsList.slice();
        for (let i = 0; i < next.length; ++i) {
            if (next[i].id === id) {
                const target = i + delta;
                if (target < 0 || target >= next.length) {
                    return false;
                }
                const moved = next[i];
                next[i] = next[target];
                next[target] = moved;
                customButtonsList = next;
                chatButtonsChanged();
                return true;
            }
        }
        return false;
    }
    function removeCustomChatButton(id) {
        const next = customButtonsList.filter(item => item.id !== id);
        if (next.length === customButtonsList.length) {
            return false;
        }
        customButtonsList = next;
        chatButtonsChanged();
        return true;
    }

    function locked(key) { return lockedKeys[key] === true; }
    function setLocked(key, value) {
        const next = Object.assign({}, lockedKeys);
        next[key] = value;
        lockedKeys = next;
        lockCalls = lockCalls.concat([[key, value]]);
        return true;
    }
}
