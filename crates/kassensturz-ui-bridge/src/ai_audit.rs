// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A test that nothing AI-related shows while AI is off (D-013, AGENTS.md §8).
//!
//! Every AI element hides behind the one `enabled` property of `AiSwitch`
//! (`ai_switch_model.rs`), and the only AI element allowed to show while AI
//! is off is the "AI features" toggle in Settings. The test reads the QML
//! files as text, the way a reviewer would, and keeps two promises that later
//! steps can't break by accident:
//!
//! - the word "AI" in a screen's text appears only in the marked toggle block
//!   of `SettingsPage.qml`, or in a file whose name starts with `Ai`;
//! - the root element of such an `Ai*.qml` file binds `visible` to
//!   `ai.enabled`, so the whole screen is hidden, not grayed out, and the
//!   file receives the switch as `required property AiSwitch ai`.
//!
//! The toggle block itself shows only when AI is available in this build.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    const BEGIN: &str = "// ai-toggle-begin";
    const END: &str = "// ai-toggle-end";

    fn qml_files() -> Vec<(String, String)> {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("qml");
        let mut files: Vec<_> = fs::read_dir(&folder)
            .expect("qml folder")
            .map(|entry| entry.expect("entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "qml"))
            .map(|path| {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                (name, fs::read_to_string(&path).expect("qml file"))
            })
            .collect();
        files.sort();
        files
    }

    /// Whether `line` has the word "AI" in it.
    fn mentions_ai(line: &str) -> bool {
        line.split(|c: char| !c.is_alphanumeric())
            .any(|word| word == "AI")
    }

    #[test]
    fn ai_text_shows_only_in_the_toggle_or_in_hidden_ai_files() {
        let files = qml_files();
        assert!(files.iter().any(|(name, _)| name == "SettingsPage.qml"));
        for (name, text) in &files {
            if name.starts_with("Ai") {
                continue;
            }
            let mut in_toggle = false;
            for (number, line) in text.lines().enumerate() {
                let number = number + 1;
                if line.trim() == BEGIN || line.trim_start().starts_with(&format!("{BEGIN}:")) {
                    assert_eq!(name, "SettingsPage.qml", "{name}:{number}: toggle block");
                    in_toggle = true;
                } else if line.trim() == END {
                    in_toggle = false;
                } else if mentions_ai(line) && !line.trim_start().starts_with("//") {
                    assert!(
                        in_toggle,
                        "{name}:{number}: AI shown outside the toggle block: {line}"
                    );
                }
            }
            assert!(!in_toggle, "{name}: the toggle block never ends");
        }
    }

    #[test]
    fn the_toggle_block_shows_only_when_ai_is_in_this_build() {
        let settings = qml_files()
            .into_iter()
            .find(|(name, _)| name == "SettingsPage.qml")
            .expect("settings page")
            .1;
        let start = settings.find(BEGIN).expect("toggle block");
        let end = settings.find(END).expect("end of toggle block");
        let block = &settings[start..end];
        // Every element of the block that shows is hidden when AI is not in
        // the build; its two cards say so on their own line.
        let elements = block.matches("FormCard.FormCard {").count()
            + block.matches("FormCard.FormSectionText {").count();
        let hidden = block.matches("visible: page.ai.available").count();
        assert!(elements >= 1);
        assert_eq!(elements, hidden, "{block}");
        assert!(block.contains("checked: page.ai.enabled"), "{block}");
    }

    #[test]
    fn every_ai_file_hides_itself_behind_the_switch() {
        for (name, text) in qml_files() {
            if !name.starts_with("Ai") {
                continue;
            }
            assert!(
                text.contains("required property AiSwitch ai"),
                "{name}: takes the switch as `required property AiSwitch ai`"
            );
            assert!(
                text.lines()
                    .any(|line| line.trim() == "visible: ai.enabled"),
                "{name}: its root binds `visible: ai.enabled`"
            );
        }
    }

    fn qml(name: &str) -> String {
        qml_files()
            .into_iter()
            .find(|(file, _)| file == name)
            .unwrap_or_else(|| panic!("{name} is a screen"))
            .1
    }

    /// The consent dialog has a sentence for each thing the statement
    /// names, so a wider statement (a new `SentData`) can't reach the user
    /// as silence (D-204).
    #[cfg(feature = "ai")]
    #[test]
    fn the_consent_dialog_has_a_sentence_for_every_word_of_the_statement() {
        let page = qml("AiSettingsPage.qml");
        for word in crate::ai_settings::statement_words() {
            assert!(
                page.contains(&format!("case \"{word}\":")),
                "AiSettingsPage.qml has no sentence for \"{word}\""
            );
        }
    }

    /// Every `ai.`, `ai-chat.` and `ai-changes.` code has its message, in
    /// words that say what to do next.
    #[test]
    fn every_ai_error_code_has_a_message() {
        let codes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/ERROR_CODES.md");
        let codes = fs::read_to_string(codes).expect("the list of error codes");
        let messages = qml("AiErrorMessage.qml");
        let mut found = 0;
        for line in codes.lines() {
            let Some(code) = ["ai.", "ai-chat.", "ai-changes."]
                .iter()
                .find_map(|prefix| {
                    line.strip_prefix(&format!("| `{prefix}"))
                        .and_then(|rest| rest.split('`').next())
                        .map(|code| format!("{prefix}{code}"))
                })
            else {
                continue;
            };
            found += 1;
            assert!(
                messages.contains(&format!("case \"{code}\":")),
                "AiErrorMessage.qml has no message for {code}"
            );
        }
        assert!(found >= 26, "the ai codes weren't found ({found})");
    }

    /// The provider's key goes in through `SecretField` and out through
    /// nothing: the page only hands the field's text to `save` and
    /// `testConnection`, clears it, and asks whether it is empty (D-203).
    #[test]
    fn the_key_field_is_a_secret_field_and_its_text_goes_nowhere_else() {
        let page = qml("AiSettingsPage.qml");
        assert!(page.contains("SecretField {\n                id: keyField"));
        let allowed = [
            "settings.save(",
            "settings.testConnection(",
            "keyField.text = \"\"",
            "keyField.text !== \"\"",
        ];
        let mut uses = 0;
        for (number, line) in page.lines().enumerate() {
            if line.trim_start().starts_with("//") || !line.contains("keyField.text") {
                continue;
            }
            uses += 1;
            assert!(
                allowed.iter().any(|ok| line.contains(ok)),
                "AiSettingsPage.qml:{}: the key's text is used for more than saving or testing: {line}",
                number + 1
            );
        }
        assert!(uses >= 4, "the key field's uses weren't found");
    }

    /// The key typed for one question goes to `ask` or `askFeature` (the
    /// same question, worded by a button, D-222) and nowhere else (D-211,
    /// threat T-25).
    #[test]
    fn the_chats_key_field_is_a_secret_field_and_its_text_goes_only_to_ask() {
        let panel = qml("AiChatPanel.qml");
        assert!(panel.contains("SecretField {\n        id: keyField"));
        let allowed = ["chat.ask(", "chat.askFeature(", "keyField.text = \"\""];
        let mut uses = 0;
        for (number, line) in panel.lines().enumerate() {
            if line.trim_start().starts_with("//") || !line.contains("keyField.text") {
                continue;
            }
            uses += 1;
            assert!(
                allowed.iter().any(|ok| line.contains(ok)),
                "AiChatPanel.qml:{}: the key's text is used for more than asking: {line}",
                number + 1
            );
        }
        // `ask` and `askFeature`, each with the field cleared after it.
        assert_eq!(uses, 4, "the key field's uses weren't found");
    }

    /// The sidebar and its toggles show only while AI is on (D-013, D-210),
    /// and the only QML that confirms a set is the review page's button.
    #[test]
    fn the_sidebar_and_its_toggles_hide_with_the_switch_and_only_the_review_page_confirms() {
        let main = qml("Main.qml");
        let drawer = &main[main.find("contextDrawer:").expect("the sidebar")..];
        let drawer = &drawer[..drawer.find("contentItem:").expect("its content")];
        for needle in [
            "enabled: aiSwitch.enabled",
            "handleVisible: aiSwitch.enabled",
        ] {
            assert!(drawer.contains(needle), "the sidebar lacks `{needle}`");
        }
        let toggle = &main[main
            .find("text: chatPanel.toggleText")
            .expect("the menu entry")..];
        let toggle_start = main[..main.len() - toggle.len()]
            .rfind("Kirigami.Action {")
            .expect("its action");
        let action = &main[toggle_start..main.len() - toggle.len() + toggle.find("},").unwrap()];
        assert!(action.contains("visible: aiSwitch.enabled"), "{action}");
        assert!(action.contains("enabled: aiSwitch.enabled"), "{action}");
        for (name, text) in qml_files() {
            let confirms = text
                .lines()
                .filter(|line| {
                    !line.trim_start().starts_with("//") && line.contains("chat.confirm(")
                })
                .count();
            let expected = usize::from(name == "AiChangeReviewPage.qml");
            assert_eq!(confirms, expected, "{name}: calls of confirm");
        }
    }
}
