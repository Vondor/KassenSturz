// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A test that no screen shows text from a bank, a file or the user as rich
//! text (D-143, threat T-08, bug #98). Qt Quick reads a text that looks like
//! HTML as markup unless `textFormat` says otherwise, and markup can load a
//! picture from the web. The network guard (`network_guard.rs`) is the second
//! lock; this is the first: a label whose text isn't one fixed sentence says
//! `textFormat: Text.PlainText`.
//!
//! The test reads the QML files as text, the way a reviewer would. It only
//! knows the elements this app uses to show a text; Kirigami's InlineMessage,
//! PlaceholderMessage and PromptDialog have no `textFormat`, so a sentence
//! with outside text in it goes through `Plain.plain()` instead.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    /// An element being read: its type, and the properties that need a format.
    struct Element {
        kind: String,
        line: usize,
        needs: Vec<(&'static str, String)>,
        formats: Vec<String>,
    }

    /// Whether a value is more than a fixed sentence: after taking out the
    /// quoted texts and the translation calls, a name, a call or a number's
    /// source is left.
    fn is_dynamic(value: &str) -> bool {
        let mut rest = String::new();
        let mut in_string = false;
        let mut escaped = false;
        for c in value.chars() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
            } else if c == '"' {
                in_string = true;
            } else {
                rest.push(c);
            }
        }
        if rest.trim() == "{" {
            return true;
        }
        rest.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .filter(|word| !word.is_empty())
            .any(|word| {
                let last = word.rsplit('.').next().unwrap_or(word);
                !(last.starts_with("i18n")
                    || word.chars().all(|c| c.is_ascii_digit())
                    || matches!(word, "true" | "false" | "null" | "undefined"))
            })
    }

    /// `Type {`, also as `property: Type {`; the type if it is one.
    fn opening_type(line: &str) -> Option<&str> {
        let line = line.strip_suffix('{')?.trim_end();
        let line = match line.split_once(':') {
            Some((key, rest)) if key.chars().all(|c| c.is_alphanumeric() || c == '.') => {
                rest.trim()
            }
            _ => line,
        };
        let mut chars = line.chars();
        let first = chars.next()?;
        (first.is_ascii_uppercase() && chars.all(|c| c.is_alphanumeric() || c == '.' || c == '_'))
            .then_some(line)
    }

    /// What the elements in one file show without a plain-text format.
    fn unformatted(source: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut stack: Vec<Option<Element>> = Vec::new();
        for (index, line) in source.lines().enumerate() {
            let number = index + 1;
            let text = line.trim();
            if text.starts_with('}')
                && let Some(Some(element)) = stack.pop()
            {
                for (key, format) in element.needs {
                    if !element.formats.contains(&format) {
                        found.push(format!(
                            "line {}: {} {key}: needs {format}: Text.PlainText",
                            element.line, element.kind
                        ));
                    }
                }
            }
            if text.ends_with('{') {
                stack.push(opening_type(text).map(|kind| Element {
                    kind: kind.to_owned(),
                    line: number,
                    needs: Vec::new(),
                    formats: Vec::new(),
                }));
                continue;
            }
            let Some(Some(element)) = stack.last_mut() else {
                continue;
            };
            let Some((key, value)) = text.split_once(':') else {
                continue;
            };
            let key = key.trim();
            if !key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '_')
            {
                continue;
            }
            if key.ends_with("textFormat") {
                element.formats.push(key.to_owned());
                continue;
            }
            let value = value.trim();
            if value.is_empty() || !is_dynamic(value) {
                continue;
            }
            let (key, format) = match (element.kind.as_str(), key) {
                (
                    "Controls.Label" | "Label" | "Kirigami.Heading" | "FormCard.FormSectionText",
                    "text",
                ) => ("text", "textFormat"),
                ("FormCard.FormTextDelegate", "text") => ("text", "textItem.textFormat"),
                ("FormCard.FormTextDelegate" | "FormCard.FormButtonDelegate", "description") => {
                    ("description", "descriptionItem.textFormat")
                }
                _ => continue,
            };
            element.needs.push((key, format.to_owned()));
        }
        found
    }

    #[test]
    fn every_label_with_changing_text_reads_it_as_plain_text() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("qml");
        let mut problems = Vec::new();
        let mut files = 0;
        for entry in fs::read_dir(&folder).expect("the qml folder") {
            let path = entry.expect("a file").path();
            // AmountLabel sets it once for the amounts that use it.
            if path.extension().is_none_or(|e| e != "qml") || path.ends_with("AmountLabel.qml") {
                continue;
            }
            files += 1;
            let source = fs::read_to_string(&path).expect("a readable file");
            for problem in unformatted(&source) {
                problems.push(format!("{}: {problem}", path.display()));
            }
        }
        assert!(files > 30, "the QML files weren't found");
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn amount_labels_read_plain_text() {
        let source =
            fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("qml/AmountLabel.qml"))
                .expect("AmountLabel.qml");
        assert!(source.contains("textFormat: Text.PlainText"));
    }

    #[test]
    fn the_check_finds_a_label_that_forgot() {
        let source = "Controls.Label {\n    text: row.title\n}\n\
                      FormCard.FormTextDelegate {\n    text: i18n(\"A\")\n    description: page.x\n}\n\
                      Controls.Label {\n    text: page.i18nContext.i18nc(\"@info\", \"fixed\")\n}\n\
                      Controls.Label {\n    text: row.title\n    textFormat: Text.PlainText\n}\n";
        assert_eq!(
            unformatted(source),
            [
                "line 1: Controls.Label text: needs textFormat: Text.PlainText",
                "line 4: FormCard.FormTextDelegate description: needs descriptionItem.textFormat: Text.PlainText",
            ]
        );
    }

    #[test]
    fn a_message_with_outside_text_goes_through_plain() {
        // These show text from a file: a Kirigami message has no textFormat.
        for (file, needle) in [
            ("FormError.qml", "Plain.plain("),
            ("RegisterPage.qml", "Plain.plain("),
            ("RulesPage.qml", "Plain.plain("),
        ] {
            let source =
                fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("qml").join(file))
                    .expect("a QML file");
            assert!(source.contains(needle), "{file} must use {needle}");
        }
    }
}
