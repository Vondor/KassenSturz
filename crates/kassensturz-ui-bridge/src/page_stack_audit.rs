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

    /// Inside a `Component { … }`, a binding such as `bookMove: bookMove`
    /// reads the made object's own property, not the object with that id
    /// outside: Qt reports a binding loop and the property stays null. The
    /// move page then had no book, so "Move my book" stayed grey and every
    /// folder line was empty (M6-B239). Such a binding names the id by
    /// another name.
    #[test]
    fn no_binding_in_a_component_names_itself() {
        let mut checked = 0;
        for entry in
            fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("qml")).expect("the qml folder")
        {
            let path = entry.expect("a file").path();
            if path.extension().is_none_or(|e| e != "qml") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("a readable file");
            let found = self_bindings_in_components(&source);
            assert!(
                found.is_empty(),
                "{}: {found:?} in a Component binds the property to itself (M6-B239)",
                path.display()
            );
            checked += 1;
        }
        assert!(checked > 30, "the QML files weren't found");
    }

    /// The lines (numbered from 1) inside a `Component { … }` block that bind
    /// a property to a name equal to its own.
    fn self_bindings_in_components(source: &str) -> Vec<(usize, String)> {
        let mut found = Vec::new();
        let mut depth = 0_i64;
        // The depths at which open `Component` blocks started.
        let mut components: Vec<i64> = Vec::new();
        for (index, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if !components.is_empty()
                && let Some((name, value)) = trimmed.split_once(": ")
                && !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && value == name
            {
                found.push((index + 1, trimmed.to_owned()));
            }
            if trimmed.starts_with("Component {") {
                components.push(depth);
            }
            depth += i64::try_from(line.matches('{').count()).unwrap_or(0);
            depth -= i64::try_from(line.matches('}').count()).unwrap_or(0);
            while components.last().is_some_and(|&start| depth <= start) {
                components.pop();
            }
        }
        found
    }

    #[test]
    fn a_self_binding_is_found_only_inside_a_component() {
        let source = "Item {\n    feed: feed\n    Component {\n        id: page\n\n        Page {\n            bookMove: bookMove\n            other: mover\n        }\n    }\n    after: after\n}\n";
        assert_eq!(
            self_bindings_in_components(source),
            [(7, "bookMove: bookMove".to_owned())]
        );
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

    /// Kirigami's `OverlaySheet` prints "Binding loop detected for property
    /// implicitHeight" on every opening, even around a single rectangle (KF
    /// 6.x, M6-B10), so a screen that needs a floating panel uses a
    /// `Kirigami.Dialog`.
    #[test]
    fn no_screen_uses_the_overlay_sheet() {
        for entry in
            fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("qml")).expect("the qml folder")
        {
            let path = entry.expect("a file").path();
            if path.extension().is_none_or(|e| e != "qml") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("a readable file");
            assert!(
                !source.lines().any(|line| {
                    let line = line.trim_start();
                    !line.starts_with("//") && line.contains("OverlaySheet {")
                }),
                "{}: use Kirigami.Dialog, not OverlaySheet (M6-B10)",
                path.display()
            );
        }
    }

    /// A Kirigami `FormLayout` logs "Cannot read property 'isSection' of
    /// null", "'Accessible' of null" and "Unable to assign [undefined] to
    /// int" for every row taken out of it (KF 6.x). The properties panel's
    /// rows came from a Repeater on `fields`, a new array after every change,
    /// so each change rebuilt them all (M6-B99). Its rows come from a
    /// `ListModel` that is only ever added to.
    #[test]
    fn the_properties_panel_keeps_its_rows() {
        let panel = qml("PropertiesPanel.qml");
        assert!(panel.contains("ListModel {\n        id: fieldRows\n"));
        assert!(panel.contains("model: fieldRows\n"));
        assert!(!panel.contains("model: panel.fields"));
        assert!(
            !panel.contains("fieldRows.remove") && !panel.contains("fieldRows.clear"),
            "a row taken out of the FormLayout logs errors"
        );
    }

    /// `bankUsable()` writes its own cache the first time it is asked, so a
    /// binding on it read and wrote the same property: "Binding loop
    /// detected for property bankReady" when the Overview opened (M6-B99).
    /// Pages set `bankReady` once instead.
    #[test]
    fn bank_ready_is_set_once_not_bound() {
        let main = qml("Main.qml");
        assert!(!main.contains("bankReady: root.bankUsable()"));
        assert_eq!(
            main.matches("Component.onCompleted: bankReady = root.bankUsable()")
                .count(),
            3
        );
    }

    /// The texts about the folder for copies tell the user to choose "Try
    /// again", so that button is a row of its own on the page, shown with
    /// the problem; as a trailing button of the problem's line it didn't
    /// show (M6-B241, US6 P3 VM round 1, A3).
    #[test]
    fn the_copies_folder_offers_the_try_again_it_names() {
        let settings = qml("SettingsPage.qml");
        assert!(settings.contains("choose Try again"));
        let (_, button) = settings
            .split_once("id: copiesTryAgain")
            .expect("the Try again row");
        let (button, _) = button.split_once("onClicked").expect("its action");
        assert!(button.contains("backupCopies.status.state === \"problem\""));
        assert!(button.contains("\"Try again\")"));
        assert!(
            !settings
                .contains("trailing: Controls.Button {\n                visible: backupCopies"),
            "Try again must not be a trailing button"
        );
        assert!(settings.contains("FormCard.FormButtonDelegate {\n            id: copiesTryAgain"));
    }

    /// With only the bank log on, the window title names that mode, and
    /// "Show the log" tells the user when the folder can't be opened instead
    /// of doing nothing (M6-B243, US6 P3 VM round 1, A6a).
    #[test]
    fn the_bank_log_names_itself_and_never_fails_silently() {
        let main = qml("Main.qml");
        let (_, title) = main.split_once("    title: ").expect("the title");
        let (title, _) = title.split_once("\n    width:").expect("its end");
        assert!(title.starts_with("developerMode.on\n"));
        assert!(title.contains("\"developer mode for bank connections\""));
        let (_, show) = main
            .split_once("ActionLog.taken(\"Main_showBankLog\")")
            .expect("Show the log");
        let (show, _) = show.split_once("Kirigami.Action").expect("its end");
        assert!(show.contains("!Qt.openUrlExternally(url)"));
        assert!(show.contains("showPassiveNotification("));
    }
}
