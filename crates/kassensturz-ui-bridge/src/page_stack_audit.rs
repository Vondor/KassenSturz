// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that read the QML for the page stack's habits, the way
//! `plain_text_audit` does for labels (D-161). QML isn't run by the unit
//! tests, so these keep three faults of M4 Phase 2 from coming back; each
//! was seen in the running app first, with the click-through described in
//! D-161.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    fn qml(file: &str) -> String {
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("qml").join(file))
            .unwrap_or_else(|error| panic!("{file}: {error}"))
    }

    /// Kirigami's page row leaves a page it popped in a one-page window
    /// alive, parked to the right of the visible page, and it shows over
    /// the open page as soon as the window is made wider. "Done" on the
    /// bank setup then looked as if it did nothing. `Main.qml` deletes every
    /// page that leaves the stack.
    #[test]
    fn a_page_that_leaves_the_stack_is_deleted() {
        let main = qml("Main.qml");
        assert!(
            main.contains(
                "Connections {\n        target: root.pageStack\n\n        function onPageRemoved(page: Item) {\n            page.destroy("
            ),
            "Main.qml must delete a page when it leaves the stack"
        );
    }

    /// Only the pages `makePage` makes are known to be the window's to
    /// delete, so no screen puts a page on the stack by itself.
    #[test]
    fn every_page_is_made_by_make_page() {
        let mut files = 0;
        for entry in
            fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("qml")).expect("the qml folder")
        {
            let path = entry.expect("a file").path();
            if path.extension().is_none_or(|e| e != "qml") {
                continue;
            }
            files += 1;
            let source = fs::read_to_string(&path).expect("a readable file");
            for (number, line) in source.lines().enumerate() {
                let direct = ["pageStack.push(", "pageStack.replace("]
                    .iter()
                    .any(|call| line.contains(call));
                let through_make_page = line.contains("(makePage(");
                assert!(
                    !direct || through_make_page,
                    "{}:{}: use pushPage or replacePage, not the page stack's own push or replace (#77, D-161)",
                    path.display(),
                    number + 1
                );
            }
        }
        assert!(files > 30, "the QML files weren't found");
    }

    /// `changeHistory.undoChange` is empty until the history is first read,
    /// which `undefined.action` turns into a TypeError in the menu at the
    /// start of the app (Main.qml:468 and :477 in build 0e44ba1).
    #[test]
    fn the_undo_and_redo_reads_cope_with_an_empty_change() {
        let main = qml("Main.qml");
        for property in ["undoChange", "redoChange"] {
            // The reads that decide whether there is a change; the ones
            // after them, in the notice's text, run only when there is.
            assert!(
                !main.contains(&format!("{property}.action !== undefined")),
                "{property}.action !== undefined must be {property}?.action !== undefined"
            );
            assert!(main.contains(&format!("{property}?.action !== undefined")));
        }
    }

    /// The made-up bank's accounts are named in English in its code. The
    /// screens that list a bank's accounts take the name from
    /// `BankAccountNames`, which translates it by the kind of account, and
    /// show a real bank's own words as they are.
    #[test]
    fn bank_accounts_are_named_through_bank_account_names() {
        for file in ["BankSetupPage.qml", "BankFetchPage.qml"] {
            let source = qml(file);
            assert!(
                source.contains("accountNames.name(choice.modelData, page.bank.fake)"),
                "{file} must name a bank's accounts with BankAccountNames"
            );
            assert!(
                !source.contains("choice.modelData.name"),
                "{file} must not show a bank account's name as it comes"
            );
        }
        let names = qml("BankAccountNames.qml");
        for kind in ["current", "savings", "credit-card"] {
            assert!(names.contains(&format!("case \"{kind}\":")), "{kind}");
        }
    }
}
