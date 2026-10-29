// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that read the QML for the page stack's habits, the way
//! `plain_text_audit` does for labels (D-161). QML isn't run by the unit
//! tests, so these keep faults seen in the running app from coming back; the
//! first three were found in M4 Phase 2 with the click-through described in
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

    /// The chat's lists must not be shown from an array that is made again
    /// for each new line. The list then deleted every message and built them
    /// all again, and the error boxes' action buttons, which Kirigami builds
    /// a moment after the box appears, were deleted before they were done:
    /// thousands of `kf.kirigami.layouts: Could not create delegate for
    /// ToolBarLayout` lines in the terminal (M5-B26). A `ListModel` keeps
    /// the messages that are already there.
    #[test]
    fn the_chat_lists_keep_their_messages_when_one_is_added() {
        let panel = qml("AiChatPanel.qml");
        for list in ["messages", "logLines"] {
            assert!(
                panel.contains(&format!("ListModel {{\n        id: {list}\n")),
                "{list} must be a ListModel"
            );
            assert!(
                !panel.contains(&format!("property var {list}")),
                "{list} must not be an array property"
            );
            assert!(
                !panel.contains(&format!("{list} = ")),
                "{list} must not be set to a new array"
            );
            assert!(
                panel.contains(&format!("model: {list}\n")),
                "the list of {list} must be shown from the ListModel"
            );
        }
    }

    /// The chat sidebar's `modal` once followed `wideScreen`, which counts
    /// the width of the menu too. When that flipped (the menu's texts
    /// change), both sidebars closed on their own while the user read the
    /// AI's changes (M5-B36). The chat decides from the window's width only.
    #[test]
    fn the_chat_sidebar_does_not_follow_the_menus_width() {
        let main = qml("Main.qml");
        let (_, chat) = main.split_once("id: chatDrawer").expect("the chat drawer");
        let (chat, _) = chat.split_once("contentItem:").expect("its content");
        assert!(
            !chat.contains("wideScreen"),
            "the chat's modal must not use wideScreen"
        );
        assert!(chat.contains("chatBesideMinWidth"));
    }

    /// Every group of account cards uses the one grid, so the groups line up
    /// the same way (M5-F3).
    #[test]
    fn the_accounts_page_lines_up_its_groups_with_one_grid() {
        let page = qml("AccountsPage.qml");
        assert!(
            !page.contains("CardsLayout"),
            "groups must not use CardsLayout"
        );
        assert_eq!(page.matches("AccountGrid {").count(), 2);
    }

    /// The book's folder on the Settings page opens in the file manager
    /// (M5-F2).
    #[test]
    fn the_settings_folder_row_opens_the_folder() {
        let settings = qml("SettingsPage.qml");
        assert!(
            settings.contains("Qt.openUrlExternally(page.session.folderUrl)"),
            "the folder row must open the folder"
        );
    }

    /// The chat sidebar's width has limits and is kept, like the menu's
    /// (#218).
    #[test]
    fn the_chat_sidebar_has_a_width_of_its_own() {
        let main = qml("Main.qml");
        let (_, chat) = main.split_once("id: chatDrawer").expect("the chat drawer");
        let (chat, _) = chat.split_once("contentItem:").expect("its content");
        assert!(
            chat.contains("width: Math.max(root.chatMinWidth"),
            "the chat must have a width limit"
        );
        assert!(main.contains("windowState.saveChatWidth(root.chatWidth)"));
    }

    /// The menu's `modal` followed `wideScreen` too, and turned into an
    /// overlay after a review of AI changes (#197). The review page is also
    /// only opened through `showChangeReview`, which never stacks a second
    /// review of the same changes (#199).
    #[test]
    fn the_menu_and_the_review_page_do_not_stack_surprises() {
        let main = qml("Main.qml");
        let (_, menu) = main.split_once("globalDrawer:").expect("the menu");
        let (menu, _) = menu.split_once("actions:").expect("its actions");
        assert!(
            !menu.contains("wideScreen"),
            "the menu's modal must not use wideScreen"
        );
        assert!(menu.contains("menuBesideMinWidth"));
        // The menu's width is its own and never its longest text's (#205).
        assert!(
            menu.contains("width: Math.max(root.menuMinWidth"),
            "the menu must have a width limit"
        );
        assert_eq!(
            main.matches("pushPage(aiChangeReviewPage").count(),
            1,
            "only showChangeReview may push the review page"
        );
    }
}
