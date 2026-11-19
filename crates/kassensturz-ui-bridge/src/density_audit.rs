// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that read the QML for what a UI density plugin can hide (D-927),
//! the way `secret_field_audit` reads it for secrets. A layout plugin's
//! `hide_hints` hides the grey explanations of `HintText.qml` and nothing
//! else, so these keep a warning, an error or a question before a change
//! from ever becoming a `HintText`.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    /// Words that make a text a warning or about data, which always stays.
    const WARNING_WORDS: [&str; 16] = [
        "undo",
        "delete",
        "deleted",
        "lost",
        "lose",
        "remove",
        "removed",
        "overwrite",
        "overwrites",
        "warning",
        "careful",
        "trust",
        "backup",
        "copy",
        "couldn",
        "balance",
    ];

    fn qml_files() -> Vec<(String, String)> {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("qml");
        let mut files = Vec::new();
        for entry in fs::read_dir(&folder).expect("the qml folder") {
            let path = entry.expect("a file").path();
            if path.extension().is_none_or(|e| e != "qml") {
                continue;
            }
            let name = path
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .into_owned();
            let source = fs::read_to_string(&path).expect("a readable file");
            files.push((name, source));
        }
        assert!(files.len() > 30, "the QML files weren't found");
        files
    }

    /// The `HintText` blocks of `source`, with their first line's number.
    fn hint_blocks(source: &str) -> Vec<(usize, String)> {
        let lines: Vec<&str> = source.lines().collect();
        let mut blocks = Vec::new();
        for (start, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("HintText {") {
                continue;
            }
            let indent = line.len() - trimmed.len();
            let end = lines[start + 1..]
                .iter()
                .position(|l| {
                    l.len() - l.trim_start().len() == indent && l.trim_start().starts_with('}')
                })
                .map_or(lines.len(), |offset| start + 1 + offset);
            blocks.push((start + 1, lines[start..end].join("\n")));
        }
        blocks
    }

    fn words(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect()
    }

    /// Only `HintText` reads `Look.hideHints`, so the switch can't reach a
    /// warning, an error message or a dialog.
    #[test]
    fn only_a_hint_text_can_be_hidden() {
        for (name, source) in qml_files() {
            if name == "HintText.qml" {
                assert!(source.contains("visible: !Look.hideHints"));
                continue;
            }
            assert!(
                !source.contains("Look.hideHints"),
                "{name}: only HintText.qml may read Look.hideHints"
            );
        }
    }

    /// A `HintText` is an explanation the page works without: never in a
    /// dialog, never shown only sometimes (as a problem is), never coloured,
    /// and never with words of a warning or about the user's data.
    #[test]
    fn a_hint_text_is_never_a_warning() {
        let mut hints = 0;
        for (name, source) in qml_files() {
            for (line, block) in hint_blocks(&source) {
                hints += 1;
                assert!(
                    !name.contains("Dialog"),
                    "{name}:{line}: a dialog's text is never a HintText"
                );
                for key in ["visible:", "color:", "type:"] {
                    assert!(
                        !block.contains(key),
                        "{name}:{line}: a HintText sets no {key}"
                    );
                }
                let found: Vec<String> = words(&block)
                    .into_iter()
                    .filter(|word| WARNING_WORDS.contains(&word.as_str()))
                    .collect();
                assert!(
                    found.is_empty(),
                    "{name}:{line}: a HintText with {found:?} reads like a warning; use FormCard.FormSectionText"
                );
            }
        }
        assert!(hints >= 3, "the HintTexts weren't found");
    }

    /// The audit itself finds a warning word in a hint.
    #[test]
    fn the_audit_finds_a_warning_in_a_hint() {
        let source = "Item {\n    HintText {\n        text: \"Undo takes it back.\"\n    }\n}\n";
        let blocks = hint_blocks(source);
        assert_eq!(blocks.len(), 1);
        assert!(
            words(&blocks[0].1)
                .iter()
                .any(|word| WARNING_WORDS.contains(&word.as_str()))
        );
    }
}
