// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that read the QML for how secrets are typed, the way
//! `page_stack_audit` does for the page stack (D-162). QML isn't run by the
//! unit tests, so these keep a PIN, a TAN or a password from ever showing
//! as plain text while it is typed: M4 Phase 2 found the TAN dialog's field
//! showing the TAN, and the fix is one field, `SecretField.qml`, which every
//! secret goes into.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    /// The words that make a text field a secret's field when they stand in
    /// its block (its id, label, placeholder or accessible name).
    const SECRET_WORDS: [&str; 5] = ["pin", "tan", "password", "passphrase", "secret"];

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

    /// The blocks of the plain text inputs in `source`: from the line that
    /// opens one to the line that closes it (`}` at the same indentation).
    fn plain_input_blocks(source: &str) -> Vec<(usize, String)> {
        let opens = ["TextField {", "TextInput {", "TextArea {"];
        let lines: Vec<&str> = source.lines().collect();
        let mut blocks = Vec::new();
        for (start, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || !opens.iter().any(|open| trimmed.ends_with(open)) {
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

    /// A field that asks for a PIN, a TAN or a password is a `SecretField`,
    /// never a plain text field with its text showing.
    #[test]
    fn a_plain_text_field_is_never_for_a_secret() {
        let mut checked = 0;
        for (name, source) in qml_files() {
            // The one field that is allowed to be built from a text field.
            if name == "SecretField.qml" {
                continue;
            }
            for (line, block) in plain_input_blocks(&source) {
                checked += 1;
                let code: String = block
                    .lines()
                    .filter(|l| !l.trim_start().starts_with("//"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let found = words(&code);
                for secret in SECRET_WORDS {
                    assert!(
                        !found.iter().any(|w| w == secret),
                        "{name}:{line}: a plain text field that mentions \"{secret}\" shows what is typed; use SecretField (D-162)"
                    );
                }
            }
        }
        assert!(checked > 5, "no text fields were found to check");
    }

    /// The masking comes from one place, so there is one place to get wrong:
    /// no screen uses Kirigami's password field directly, and none turns the
    /// text on or sets how it is echoed.
    #[test]
    fn only_secret_field_decides_how_a_secret_shows() {
        for (name, source) in qml_files() {
            for (number, line) in source.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if name != "SecretField.qml" {
                    assert!(
                        !code.contains("PasswordField {"),
                        "{name}:{}: use SecretField, not Kirigami.PasswordField (D-162)",
                        number + 1
                    );
                }
                assert!(
                    name == "SecretField.qml" || !code.contains("echoMode"),
                    "{name}:{}: echoMode is set by SecretField's showPassword only (D-162)",
                    number + 1
                );
                if name != "SecretField.qml" {
                    assert!(
                        !code.contains("showPassword"),
                        "{name}:{}: a screen hides a secret with hide(), it never shows it (D-162)",
                        number + 1
                    );
                }
            }
        }
    }

    /// `SecretField` is masked when it is made, and its keyboard hints keep
    /// the text out of predictive input and clipboard history where Qt
    /// allows it.
    #[test]
    fn a_secret_field_is_masked_by_default() {
        let field = qml_files()
            .into_iter()
            .find(|(name, _)| name == "SecretField.qml")
            .expect("SecretField.qml")
            .1;
        assert!(
            field.contains("echoMode: field.showPassword ? TextInput.Normal : TextInput.Password"),
            "dots unless showPassword is on"
        );
        assert!(
            field.contains("property bool showPassword: false\n"),
            "masked until the user asks"
        );
        assert!(!field.contains("showPassword: true"));
        for hint in [
            "Qt.ImhSensitiveData",
            "Qt.ImhNoPredictiveText",
            "Qt.ImhNoAutoUppercase",
        ] {
            assert!(field.contains(hint), "SecretField must set {hint}");
        }
        // The eye button toggles, it doesn't switch the text on for good.
        assert!(field.contains("onTriggered: field.showPassword = !field.showPassword"));
    }

    /// The PIN and the TAN dialogs and the bank setup use it, and the dialogs
    /// hide the text again each time they open.
    #[test]
    fn the_bank_screens_use_secret_field_and_hide_it_again() {
        let dialogs = qml_files()
            .into_iter()
            .find(|(name, _)| name == "BankDialogs.qml")
            .expect("BankDialogs.qml")
            .1;
        for field in ["tanField", "pinField"] {
            assert!(
                dialogs.contains(&format!("SecretField {{\n                id: {field}")),
                "{field} must be a SecretField"
            );
            assert!(
                dialogs.contains(&format!("{field}.hide();")),
                "{field} must be hidden again when its dialog opens"
            );
        }
        let setup = qml_files()
            .into_iter()
            .find(|(name, _)| name == "BankSetupPage.qml")
            .expect("BankSetupPage.qml")
            .1;
        assert!(setup.contains("SecretField {\n                id: pinField"));
        assert!(setup.contains("pinField.hide();"));
    }
}
