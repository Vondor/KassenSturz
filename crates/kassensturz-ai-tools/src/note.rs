// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A line for the user's "Notes for the AI" (US7, #808, threat T-64).
//!
//! The notes are text the user writes in the AI settings; the chat puts
//! them into every request as the user's own notes. A model may only
//! *suggest* a line to add: the call checks the line and hands it back to
//! the front end, which shows it word for word and saves it only when the
//! user confirms. Nothing here reads the book or the settings, and nothing
//! is written; the model's words are never changed to fit, so what the user
//! reads is exactly what would be saved. The MCP server does not offer this
//! function (Q-025).

use kassensturz_core::ai_switch::MAX_NOTES_CHARS;
use serde_json::{Value, json};

use crate::error::ToolError;
use crate::figures::Figures;
use crate::functions::{Function, Output};

/// The longest line a model may suggest, in characters.
pub const MAX_NOTE_LINE_CHARS: usize = 200;

/// The function that suggests a line for the user's notes.
pub const SUGGEST_NOTE: Function = Function {
    name: "suggest_note",
    description: "Suggests one line for the user's \"Notes for the AI\", only when the user asks \
        you to remember something for later chats, e.g. \"salary comes on the 28th\". Write one \
        short plain sentence in the user's own words, about how they use KassenSturz or their \
        book: never a figure you calculated, an ID, a password, a PIN or a key. Nothing is \
        saved: the user reads the line word for word and decides whether to add it. Never say \
        the note was saved; say that you suggested it.",
    parameters: r#"{"type":"object","properties":{"line":{"type":"string","maxLength":200,"description":"One line of plain text, without line breaks."}},"required":["line"],"additionalProperties":false}"#,
};

/// The arguments of the call, by their fixed names: keywords of
/// `ai-tools.bad-argument`.
#[cfg(test)]
pub(crate) const FIELDS: &[&str] = &["line"];

/// A line the model suggested for the user's notes. It is only a
/// suggestion until the user confirms it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSuggestion {
    /// The line, exactly as the model wrote it, without line breaks.
    pub line: String,
}

/// Checks a model's call of [`SUGGEST_NOTE`] with `arguments`, against the
/// notes the user has now, and gives the suggestion and the result the model
/// gets. A line that is empty, longer than [`MAX_NOTE_LINE_CHARS`], holds a
/// line break, a control or direction character, is already among the notes,
/// or would not fit into the notes is refused: the line is never changed.
pub fn suggest_note(
    arguments: &Value,
    current_notes: &str,
) -> Result<(NoteSuggestion, Output), ToolError> {
    let bad = ToolError::BadArgument { argument: "line" };
    let Value::Object(values) = arguments else {
        return Err(ToolError::BadArgument {
            argument: "arguments",
        });
    };
    if values.keys().any(|key| key != "line") {
        return Err(ToolError::BadArgument {
            argument: "arguments",
        });
    }
    let Some(Value::String(line)) = values.get("line") else {
        return Err(bad);
    };
    let line = line.trim();
    if line.is_empty()
        || line.chars().count() > MAX_NOTE_LINE_CHARS
        || line.chars().any(|c| {
            c.is_control()
                || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}' | '\u{2028}' | '\u{2029}')
        })
    {
        return Err(bad);
    }
    if current_notes
        .lines()
        .any(|existing| existing.trim() == line)
        || current_notes.chars().count() + 1 + line.chars().count() > MAX_NOTES_CHARS
    {
        return Err(bad);
    }
    let suggestion = NoteSuggestion {
        line: line.to_owned(),
    };
    let output = Output::new(
        json!({
            "suggested": true,
            "message": "KassenSturz will show this line to the user when you have finished \
                        your answer. It is saved only if the user confirms it. Nothing was \
                        changed.",
        }),
        Figures::new(),
    );
    Ok((suggestion, output))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(line: &str, current: &str) -> Result<NoteSuggestion, ToolError> {
        suggest_note(&json!({ "line": line }), current).map(|(suggestion, _)| suggestion)
    }

    #[test]
    fn the_schema_names_only_the_fixed_argument() {
        let schema: Value = serde_json::from_str(SUGGEST_NOTE.parameters).expect("JSON");
        assert_eq!(schema["additionalProperties"], false);
        let mut keys: Vec<&str> = schema["properties"]
            .as_object()
            .expect("properties")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, FIELDS);
        assert_eq!(
            schema["properties"]["line"]["maxLength"],
            MAX_NOTE_LINE_CHARS
        );
    }

    #[test]
    fn a_plain_line_comes_back_word_for_word() {
        let line = "Corner Market is groceries, Ümlaut ok";
        assert_eq!(call(line, "").unwrap().line, line);
        // Only the spaces around it go.
        assert_eq!(
            call("  Salary comes on the 28th ", "").unwrap().line,
            "Salary comes on the 28th"
        );
    }

    #[test]
    fn a_line_that_would_have_to_be_changed_is_refused() {
        for line in [
            "",
            "   ",
            "two\nlines",
            "tab\there",
            "bell\u{7}",
            "hidden\u{202e}text",
            "zero\u{200b}width",
            &"x".repeat(MAX_NOTE_LINE_CHARS + 1),
        ] {
            assert!(
                matches!(
                    call(line, ""),
                    Err(ToolError::BadArgument { argument: "line" })
                ),
                "{line:?}"
            );
        }
        assert!(call(&"x".repeat(MAX_NOTE_LINE_CHARS), "").is_ok());
    }

    #[test]
    fn a_line_already_there_or_one_too_many_is_refused() {
        assert!(
            call(
                "Salary comes on the 28th",
                "Hello\nSalary comes on the 28th\nBye"
            )
            .is_err()
        );
        let full = "y".repeat(MAX_NOTES_CHARS - 10);
        assert!(call("a line that is too long to fit", &full).is_err());
        assert!(call("short", &full).is_ok());
    }

    #[test]
    fn other_arguments_and_kinds_are_refused() {
        for arguments in [
            json!({}),
            json!({"line": 5}),
            json!({"line": null}),
            json!({"line": "ok", "extra": "x"}),
            json!(["ok"]),
        ] {
            assert!(suggest_note(&arguments, "").is_err(), "{arguments}");
        }
    }

    #[test]
    fn it_is_not_one_of_the_read_functions() {
        // The MCP server offers FUNCTIONS and nothing else (Q-025).
        assert!(crate::FUNCTIONS.iter().all(|f| f.name != SUGGEST_NOTE.name));
    }
}
