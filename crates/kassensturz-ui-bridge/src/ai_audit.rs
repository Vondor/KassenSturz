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
}
