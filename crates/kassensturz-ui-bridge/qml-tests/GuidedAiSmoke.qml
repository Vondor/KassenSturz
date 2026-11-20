// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// check-panel: guided setup, step 7 (a local AI or not) and its Help entry

// Opens step 7 of the guided setup (#1031, docs/GUIDED_SETUP.md) on a
// stand-in for AiSwitch (fakes/) and checks:
//
// - "No, or I don't know" is what holds while AI is off, and Skip, Continue
//   and "End the setup" never touch the switch (D-911);
// - "Yes" turns the switch on and asks for the provider page; "No" after
//   that turns it off again;
// - a choice that can't be saved says so and opens nothing;
// - the guided setup's Help lists the step only in a build that has AI
//   (D-039).
//
// `tools/check-panel.sh` runs it and fails on the first FAIL line or on any
// QML warning about a binding or a type.

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.vondor.kassensturz

Kirigami.ApplicationWindow {
    id: window

    width: 900
    height: 900
    visible: true
    property bool failed: false
    property var seen: []

    AiSwitch {
        id: off

        enabled: false
    }

    AiSwitch {
        id: locked

        enabled: false
        saves: false
    }

    AiSwitch {
        id: absent

        enabled: false
        available: false
    }

    Component {
        id: aiPage

        GuidedAiPage {
            onProviderClicked: window.seen.push("provider")
            onNextClicked: window.seen.push("next")
            onSkipClicked: window.seen.push("skip")
            onEndClicked: window.seen.push("end")
        }
    }

    Component {
        id: helpPage

        GuidedHelpPage {
            onStepClicked: name => window.seen.push("step:" + name)
        }
    }

    function check(ok, what) {
        if (!ok) {
            console.log("FAIL: " + what);
            failed = true;
        }
    }

    function find(item, test, out) {
        if (!item) {
            return out;
        }
        for (let i = 0; i < item.children.length; ++i) {
            const child = item.children[i];
            if (test(child)) {
                out.push(child);
            }
            find(child, test, out);
        }
        return out;
    }

    function parts(page, test) {
        return find(page, test, []);
    }

    function buttons(page, text) {
        return parts(page, child => typeof child.clicked === "function" && child.down !== undefined && (child.text === text || child.Accessible.name === text));
    }

    function choice(page, heading) {
        return parts(page, child => child.heading === heading && child.badge !== undefined)[0];
    }

    function took(what) {
        return seen.indexOf(what) >= 0;
    }

    property var step
    property var stuck
    property var help
    property var helpWithout

    Component.onCompleted: {
        step = pageStack.push(aiPage, { ai: off, step: 7, count: 7 });
        stuck = pageStack.push(aiPage, { ai: locked, step: 0, count: 0 });
        help = pageStack.push(helpPage, { ai: off });
        helpWithout = pageStack.push(helpPage, { ai: absent });
    }

    Timer {
        interval: 1000
        running: true
        onTriggered: {
            const step = window.step;
            const no = window.choice(step, "No, or I don't know");
            const yes = window.choice(step, "Yes, I have an AI on this computer");
            window.check(no !== undefined && yes !== undefined, "step 7 offers No and Yes");
            window.check(no.selected && !yes.selected, "No holds while AI is off");
            window.check(no.badge === "This is how it stays if you do nothing" && yes.badge === "", "No says it stays so, got “" + no.badge + "”");
            window.check(window.parts(step, child => child.text === "Step 7 of 7").length === 1, "step 7 says Step 7 of 7");

            // Skip, Continue, End the setup and No leave the switch alone.
            window.buttons(step, "Skip")[0].clicked();
            window.buttons(step, "Continue")[0].clicked();
            window.buttons(step, "End the setup")[0].clicked();
            no.clicked();
            window.check(["skip", "next", "end"].every(what => window.took(what)), "step 7 has Skip, Continue and End the setup, got " + window.seen);
            window.check(off.asked.length === 0 && off.enabled === false, "Skip, Continue, End and No never touch the switch, got " + off.asked);
            window.check(!window.took("provider"), "nothing opens before Yes");

            // Yes turns it on and asks for the provider page.
            yes.clicked();
            window.check(off.asked.join(",") === "true" && off.enabled === true, "Yes turns the switch on, got " + off.asked);
            window.check(window.took("provider"), "Yes asks for the provider page");
            window.check(yes.selected && !no.selected && yes.badge === "Turned on" && no.badge === "", "Yes holds once AI is on");

            // No takes it back.
            no.clicked();
            window.check(off.asked.join(",") === "true,false" && off.enabled === false, "No turns the switch off again, got " + off.asked);
            window.check(no.selected && !yes.selected, "No holds again");

            // A choice that can't be saved says so and opens nothing.
            window.seen = [];
            const stuck = window.stuck;
            window.choice(stuck, "Yes, I have an AI on this computer").clicked();
            window.check(locked.enabled === false && !window.took("provider"), "a choice that isn't saved opens nothing");
            window.check(window.parts(stuck, child => child.text !== undefined && child.visible && String(child.text).indexOf("couldn't save this choice") >= 0).length >= 1, "a choice that isn't saved says so");
            window.check(window.buttons(stuck, "Skip")[0].visible === false && window.buttons(stuck, "Done").length === 1, "opened from Help, step 7 has Done and nothing to skip");

            // Help lists the step only where the build has AI.
            const entry = "Do you have an AI on this computer?";
            const shown = window.buttons(window.help, entry);
            window.check(shown.length === 1 && shown[0].visible, "Help lists the step where AI is in the build");
            shown[0].clicked();
            window.check(window.took("step:ai"), "Help asks for the step");
            const hidden = window.buttons(window.helpWithout, entry);
            window.check(hidden.length === 1 && hidden[0].visible === false, "Help hides the step in a build without AI");

            console.log(window.failed ? "guided setup, step 7: FAILED" : "guided setup, step 7: ok");
            Qt.exit(window.failed ? 1 : 0);
        }
    }
}
