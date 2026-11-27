// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-computer configuration for built-in and custom AI chat quick-prompt
//! buttons (US11.3, #1452, D-1013).
//!
//! Stored in `kassensturzrc` under `[AI]`, never in a book (D-892), and
//! completely hidden and unloaded while AI is off (D-013). This module is
//! compiled only in a build with AI (D-039).

use std::collections::HashSet;

use kassensturz_core::ai_switch::{
    BuiltinChatButton, CUSTOM_CHAT_BUTTONS_KEY, CustomChatButton, ENABLED_KEY, GROUP,
    MAX_CUSTOM_CHAT_BUTTONS, ON, is_valid_chat_button_id, sanitize_chat_button_label,
    sanitize_chat_button_prompt, validate_chat_button_label, validate_chat_button_prompt,
};
use serde_json::{Value, json};

use crate::settings::Settings;

/// Outcome of adding or updating a custom chat button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SaveButtonResult {
    /// Saved; carries the button's stable identifier.
    Saved(String),
    /// Refused or failed to persist, with a fixed reason word for QML.
    Failed(&'static str),
}

/// Whether `settings` currently has AI switched on (D-013).
fn ai_enabled_in(settings: &Settings) -> bool {
    settings.text(GROUP, ENABLED_KEY) == ON
}

/// Whether the built-in chat button `word` (`categories`, `month`, `rules`,
/// `describe_chart`) is enabled in `settings` (D-1013).
///
/// Always `false` while AI is off (D-013) or for an unknown word. When AI is
/// on and the key is absent (legacy `kassensturzrc`), returns `true`.
pub(crate) fn builtin_chat_button_in(settings: &Settings, word: &str) -> bool {
    if !ai_enabled_in(settings) {
        return false;
    }
    let Some(button) = BuiltinChatButton::from_word(word) else {
        return false;
    };
    match settings.text(GROUP, button.key()).as_str() {
        "" => button.on_by_default(),
        value => value != "false",
    }
}

/// Shows or hides the built-in chat button `word` in `settings`. Returns
/// `false` while AI is off, for an unknown word, or if writing fails.
pub(crate) fn set_builtin_chat_button_in(settings: &Settings, word: &str, shown: bool) -> bool {
    if !ai_enabled_in(settings) {
        return false;
    }
    let Some(button) = BuiltinChatButton::from_word(word) else {
        return false;
    };
    settings.set_text(GROUP, button.key(), if shown { ON } else { "false" })
}

/// Parses custom chat buttons from the compact JSON string stored in
/// `kassensturzrc`. Malformed JSON, non-array values, invalid entries, NUL
/// bytes, and entries past [`MAX_CUSTOM_CHAT_BUTTONS`] degrade safely
/// without panicking.
pub(crate) fn parse_custom_buttons(raw: &str) -> Vec<CustomChatButton> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(trimmed) else {
        return Vec::new();
    };
    let mut buttons = Vec::new();
    let mut seen_ids = HashSet::new();
    for item in items {
        if buttons.len() >= MAX_CUSTOM_CHAT_BUTTONS {
            break;
        }
        let Some(object) = item.as_object() else {
            continue;
        };
        let Some(label) = object
            .get("label")
            .and_then(Value::as_str)
            .and_then(sanitize_chat_button_label)
        else {
            continue;
        };
        let Some(prompt) = object
            .get("prompt")
            .and_then(Value::as_str)
            .and_then(sanitize_chat_button_prompt)
        else {
            continue;
        };
        let enabled = object
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let insert_only = object
            .get("insertOnly")
            .or_else(|| object.get("insert_only"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let id = match object
            .get("id")
            .and_then(Value::as_str)
            .filter(|candidate| {
                is_valid_chat_button_id(candidate) && !seen_ids.contains(*candidate)
            }) {
            Some(existing) => existing.to_owned(),
            None => next_id(&seen_ids),
        };
        seen_ids.insert(id.clone());
        buttons.push(CustomChatButton {
            id,
            label,
            prompt,
            enabled,
            insert_only,
        });
    }
    buttons
}

/// Serializes custom chat buttons as compact JSON for `kassensturzrc`.
/// An empty list serializes to an empty string.
pub(crate) fn serialize_custom_buttons(buttons: &[CustomChatButton]) -> String {
    if buttons.is_empty() {
        return String::new();
    }
    let array: Vec<Value> = buttons
        .iter()
        .take(MAX_CUSTOM_CHAT_BUTTONS)
        .map(|button| {
            json!({
                "id": button.id,
                "label": button.label,
                "prompt": button.prompt,
                "enabled": button.enabled,
                "insertOnly": button.insert_only,
            })
        })
        .collect();
    Value::Array(array).to_string()
}

/// Synthesizes a short, stable, unique ASCII button ID not in `used`.
fn next_id(used: &HashSet<String>) -> String {
    let mut counter: usize = 1;
    loop {
        let candidate = format!("btn-{counter}");
        if !used.contains(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// Reads the user's custom chat buttons from `settings`. Returns an empty
/// list immediately without reading the key while AI is off (D-013).
pub(crate) fn custom_chat_buttons_in(settings: &Settings) -> Vec<CustomChatButton> {
    if !ai_enabled_in(settings) {
        return Vec::new();
    }
    parse_custom_buttons(&settings.text(GROUP, CUSTOM_CHAT_BUTTONS_KEY))
}

/// Persists `buttons` into `settings` if AI is on.
fn write_custom_buttons(settings: &Settings, buttons: &[CustomChatButton]) -> bool {
    if !ai_enabled_in(settings) {
        return false;
    }
    settings.set_text(
        GROUP,
        CUSTOM_CHAT_BUTTONS_KEY,
        &serialize_custom_buttons(buttons),
    )
}

/// Adds a new custom chat button (enabled by default).
pub(crate) fn add_custom_chat_button_in(
    settings: &Settings,
    label: &str,
    prompt: &str,
    insert_only: bool,
) -> SaveButtonResult {
    if !ai_enabled_in(settings) {
        return SaveButtonResult::Failed("ai-off");
    }
    let Some(label) = validate_chat_button_label(label) else {
        return SaveButtonResult::Failed("invalid-label");
    };
    let Some(prompt) = validate_chat_button_prompt(prompt) else {
        return SaveButtonResult::Failed("invalid-prompt");
    };
    let mut buttons = custom_chat_buttons_in(settings);
    if buttons.len() >= MAX_CUSTOM_CHAT_BUTTONS {
        return SaveButtonResult::Failed("limit");
    }
    let used: HashSet<String> = buttons.iter().map(|b| b.id.clone()).collect();
    let id = next_id(&used);
    buttons.push(CustomChatButton {
        id: id.clone(),
        label,
        prompt,
        enabled: true,
        insert_only,
    });
    if write_custom_buttons(settings, &buttons) {
        SaveButtonResult::Saved(id)
    } else {
        SaveButtonResult::Failed("not-written")
    }
}

/// Updates an existing custom chat button identified by `id`.
pub(crate) fn update_custom_chat_button_in(
    settings: &Settings,
    id: &str,
    label: &str,
    prompt: &str,
    enabled: bool,
    insert_only: bool,
) -> SaveButtonResult {
    if !ai_enabled_in(settings) {
        return SaveButtonResult::Failed("ai-off");
    }
    let Some(label) = validate_chat_button_label(label) else {
        return SaveButtonResult::Failed("invalid-label");
    };
    let Some(prompt) = validate_chat_button_prompt(prompt) else {
        return SaveButtonResult::Failed("invalid-prompt");
    };
    let mut buttons = custom_chat_buttons_in(settings);
    let Some(target) = buttons.iter_mut().find(|button| button.id == id) else {
        return SaveButtonResult::Failed("not-found");
    };
    target.label = label;
    target.prompt = prompt;
    target.enabled = enabled;
    target.insert_only = insert_only;
    if write_custom_buttons(settings, &buttons) {
        SaveButtonResult::Saved(id.to_owned())
    } else {
        SaveButtonResult::Failed("not-written")
    }
}

/// Toggles whether custom chat button `id` is shown in the chat panel.
pub(crate) fn set_custom_chat_button_enabled_in(
    settings: &Settings,
    id: &str,
    enabled: bool,
) -> bool {
    if !ai_enabled_in(settings) {
        return false;
    }
    let mut buttons = custom_chat_buttons_in(settings);
    let Some(target) = buttons.iter_mut().find(|button| button.id == id) else {
        return false;
    };
    target.enabled = enabled;
    write_custom_buttons(settings, &buttons)
}

/// Reorders custom chat button `id` by `delta` (`-1` up, `+1` down).
pub(crate) fn move_custom_chat_button_in(settings: &Settings, id: &str, delta: i32) -> bool {
    if !ai_enabled_in(settings) || (delta != -1 && delta != 1) {
        return false;
    }
    let mut buttons = custom_chat_buttons_in(settings);
    let Some(index) = buttons.iter().position(|button| button.id == id) else {
        return false;
    };
    let target = if delta < 0 {
        let Some(prev) = index.checked_sub(1) else {
            return false;
        };
        prev
    } else {
        let next = index + 1;
        if next >= buttons.len() {
            return false;
        }
        next
    };
    buttons.swap(index, target);
    write_custom_buttons(settings, &buttons)
}

/// Removes custom chat button `id`.
pub(crate) fn remove_custom_chat_button_in(settings: &Settings, id: &str) -> bool {
    if !ai_enabled_in(settings) {
        return false;
    }
    let mut buttons = custom_chat_buttons_in(settings);
    let before = buttons.len();
    buttons.retain(|button| button.id != id);
    buttons.len() < before && write_custom_buttons(settings, &buttons)
}

#[cfg(test)]
mod tests {
    use kassensturz_core::ai_switch::{
        BuiltinChatButton, CUSTOM_CHAT_BUTTONS_KEY, ENABLED_KEY, GROUP,
        MAX_CHAT_BUTTON_LABEL_CHARS, MAX_CHAT_BUTTON_PROMPT_CHARS, MAX_CUSTOM_CHAT_BUTTONS, ON,
        builtin_chat_button_in as read_builtin_from_file,
    };

    use super::*;
    use crate::test_folder::TestFolder;

    fn settings_with_ai_on() -> (TestFolder, std::path::PathBuf, Settings) {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert!(settings.set_text(GROUP, ENABLED_KEY, ON));
        (folder, file, settings)
    }

    #[test]
    fn builtin_buttons_default_on_for_legacy_files_and_round_trip() {
        let (_folder, file, settings) = settings_with_ai_on();
        for button in BuiltinChatButton::ALL {
            assert!(
                builtin_chat_button_in(&settings, button.word()),
                "{button:?} starts on"
            );
        }
        assert!(!builtin_chat_button_in(&settings, "unknown"));
        assert!(!set_builtin_chat_button_in(&settings, "unknown", false));

        assert!(set_builtin_chat_button_in(&settings, "categories", false));
        assert!(set_builtin_chat_button_in(
            &settings,
            "describe_chart",
            false
        ));
        let fresh = Settings::at(&file);
        assert!(!builtin_chat_button_in(&fresh, "categories"));
        assert!(builtin_chat_button_in(&fresh, "month"));
        assert!(builtin_chat_button_in(&fresh, "rules"));
        assert!(!builtin_chat_button_in(&fresh, "describe_chart"));

        // Non-Qt reader sees the same state.
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(!read_builtin_from_file(
            &written,
            BuiltinChatButton::Categories
        ));
        assert!(read_builtin_from_file(&written, BuiltinChatButton::Month));
        assert!(!read_builtin_from_file(
            &written,
            BuiltinChatButton::DescribeChart
        ));

        // Turning back on works too.
        assert!(set_builtin_chat_button_in(&fresh, "categories", true));
        assert!(builtin_chat_button_in(&Settings::at(&file), "categories"));
    }

    #[test]
    fn custom_buttons_round_trip_with_unicode_quotes_newlines_and_markup() {
        let (_folder, file, settings) = settings_with_ai_on();
        let label = "Übersicht: \"Belege\" & <b>Test</b> \\ / = [AI]";
        let prompt = "Check 5 bills from inbox.\nMatch to transactions & keep notes.\nBackslash: \\ \"quoted\"";

        let SaveButtonResult::Saved(id) =
            add_custom_chat_button_in(&settings, label, prompt, false)
        else {
            panic!("expected save");
        };
        let loaded = custom_chat_buttons_in(&Settings::at(&file));
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, id);
        assert_eq!(loaded[0].label, label);
        assert_eq!(loaded[0].prompt, prompt);
        assert!(loaded[0].enabled);
        assert!(!loaded[0].insert_only);
    }

    #[test]
    fn custom_buttons_crud_toggle_and_reorder() {
        let (_folder, file, settings) = settings_with_ai_on();
        let SaveButtonResult::Saved(first) =
            add_custom_chat_button_in(&settings, "First", "Prompt 1", false)
        else {
            panic!("first");
        };
        let SaveButtonResult::Saved(second) =
            add_custom_chat_button_in(&settings, "Second", "Prompt 2", true)
        else {
            panic!("second");
        };
        let SaveButtonResult::Saved(third) =
            add_custom_chat_button_in(&settings, "Third", "Prompt 3", false)
        else {
            panic!("third");
        };
        assert_ne!(first, second);
        assert_ne!(second, third);

        // Reorder: move first down, cannot move past ends.
        assert!(!move_custom_chat_button_in(&settings, &first, -1));
        assert!(!move_custom_chat_button_in(&settings, &third, 1));
        assert!(!move_custom_chat_button_in(&settings, &first, 0));
        assert!(!move_custom_chat_button_in(&settings, &first, 2));
        assert!(move_custom_chat_button_in(&settings, &first, 1));
        let ids: Vec<String> = custom_chat_buttons_in(&settings)
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(ids, vec![second.clone(), first.clone(), third.clone()]);

        // Move third up.
        assert!(move_custom_chat_button_in(&settings, &third, -1));
        let ids: Vec<String> = custom_chat_buttons_in(&settings)
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(ids, vec![second.clone(), third.clone(), first.clone()]);

        // Toggle enabled off and back on.
        assert!(set_custom_chat_button_enabled_in(&settings, &second, false));
        let list = custom_chat_buttons_in(&Settings::at(&file));
        assert!(!list[0].enabled);
        assert!(list[0].insert_only);

        // Update label, prompt, enabled, and insert_only.
        assert_eq!(
            update_custom_chat_button_in(
                &settings,
                &second,
                "Second edited",
                "Updated prompt",
                true,
                false
            ),
            SaveButtonResult::Saved(second.clone())
        );
        let list = custom_chat_buttons_in(&Settings::at(&file));
        assert_eq!(list[0].label, "Second edited");
        assert_eq!(list[0].prompt, "Updated prompt");
        assert!(list[0].enabled);
        assert!(!list[0].insert_only);

        // Delete one, then all.
        assert!(remove_custom_chat_button_in(&settings, &third));
        assert!(!remove_custom_chat_button_in(&settings, "missing-id"));
        let ids: Vec<String> = custom_chat_buttons_in(&settings)
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(ids, vec![second.clone(), first.clone()]);
        assert!(remove_custom_chat_button_in(&settings, &second));
        assert!(remove_custom_chat_button_in(&settings, &first));
        assert!(custom_chat_buttons_in(&Settings::at(&file)).is_empty());
    }

    #[test]
    fn custom_buttons_enforce_count_and_length_limits_and_reject_control_chars() {
        let (_folder, _file, settings) = settings_with_ai_on();
        assert_eq!(
            add_custom_chat_button_in(&settings, "  ", "Valid prompt", false),
            SaveButtonResult::Failed("invalid-label")
        );
        assert_eq!(
            add_custom_chat_button_in(&settings, "Two\nLines", "Valid prompt", false),
            SaveButtonResult::Failed("invalid-label")
        );
        assert_eq!(
            add_custom_chat_button_in(&settings, "Has\0Nul", "Valid prompt", false),
            SaveButtonResult::Failed("invalid-label")
        );
        assert_eq!(
            add_custom_chat_button_in(
                &settings,
                &"a".repeat(MAX_CHAT_BUTTON_LABEL_CHARS + 1),
                "Valid prompt",
                false
            ),
            SaveButtonResult::Failed("invalid-label")
        );
        assert_eq!(
            add_custom_chat_button_in(&settings, "Valid", "   ", false),
            SaveButtonResult::Failed("invalid-prompt")
        );
        assert_eq!(
            add_custom_chat_button_in(&settings, "Valid", "bad\0nul", false),
            SaveButtonResult::Failed("invalid-prompt")
        );
        assert_eq!(
            add_custom_chat_button_in(
                &settings,
                "Valid",
                &"x".repeat(MAX_CHAT_BUTTON_PROMPT_CHARS + 1),
                false
            ),
            SaveButtonResult::Failed("invalid-prompt")
        );

        // Fill up to the cap (20).
        for index in 0..MAX_CUSTOM_CHAT_BUTTONS {
            assert!(matches!(
                add_custom_chat_button_in(
                    &settings,
                    &format!("Button {}", index + 1),
                    &format!("Prompt {}", index + 1),
                    false
                ),
                SaveButtonResult::Saved(_)
            ));
        }
        assert_eq!(
            add_custom_chat_button_in(&settings, "One too many", "Prompt 21", false),
            SaveButtonResult::Failed("limit")
        );
        assert_eq!(
            custom_chat_buttons_in(&settings).len(),
            MAX_CUSTOM_CHAT_BUTTONS
        );
    }

    #[test]
    fn corrupt_or_malformed_json_degrades_safely_without_panic() {
        for corrupt in [
            "",
            "not json at all",
            "{}",
            "42",
            "\"string\"",
            "[null, 123, \"str\", {}]",
            "[{\"label\":\"\",\"prompt\":\"ok\"}]",
            "[{\"label\":\"ok\",\"prompt\":\"\"}]",
            "[{\"label\":\"has\\u0000nul\",\"prompt\":\"ok\"}]",
        ] {
            assert!(parse_custom_buttons(corrupt).is_empty(), "{corrupt}");
        }

        // Mixed valid and invalid entries, duplicate IDs, missing fields, and >20 items.
        let mut items: Vec<Value> = vec![
            json!({"id": "dup", "label": "Keep 1", "prompt": "Ask 1"}),
            json!({"id": "dup", "label": "Keep 2\nline", "prompt": "Ask 2", "enabled": false, "insertOnly": true}),
            json!({"id": "bad id!", "label": "Keep 3", "prompt": "Ask 3", "insert_only": true}),
            json!({"label": "drop\0nul", "prompt": "Ask 4"}),
            json!(123),
        ];
        for i in 0..30 {
            items.push(json!({"label": format!("Extra {i}"), "prompt": format!("Prompt {i}")}));
        }
        let parsed = parse_custom_buttons(&Value::Array(items).to_string());
        assert_eq!(parsed.len(), MAX_CUSTOM_CHAT_BUTTONS);
        assert_eq!(parsed[0].id, "dup");
        assert!(parsed[0].enabled);
        assert!(!parsed[0].insert_only);
        assert_ne!(parsed[1].id, "dup");
        assert_eq!(parsed[1].label, "Keep 2 line");
        assert!(!parsed[1].enabled);
        assert!(parsed[1].insert_only);
        assert!(is_valid_chat_button_id(&parsed[2].id));
        assert!(parsed[2].insert_only);
    }

    #[test]
    fn d013_ai_off_hides_and_blocks_all_button_reads_and_writes() {
        let (_folder, file, settings) = settings_with_ai_on();
        assert!(set_builtin_chat_button_in(&settings, "month", false));
        assert!(matches!(
            add_custom_chat_button_in(&settings, "Quick", "My prompt", false),
            SaveButtonResult::Saved(_)
        ));
        assert_eq!(custom_chat_buttons_in(&settings).len(), 1);

        // Turn AI off: nothing is loaded or mutated while AI is off.
        assert!(settings.set_text(GROUP, ENABLED_KEY, "false"));
        for button in BuiltinChatButton::ALL {
            assert!(!builtin_chat_button_in(&settings, button.word()));
            assert!(!set_builtin_chat_button_in(&settings, button.word(), true));
        }
        assert!(custom_chat_buttons_in(&settings).is_empty());
        assert_eq!(
            add_custom_chat_button_in(&settings, "Another", "Prompt", false),
            SaveButtonResult::Failed("ai-off")
        );
        assert_eq!(
            update_custom_chat_button_in(&settings, "btn-1", "X", "Y", true, false),
            SaveButtonResult::Failed("ai-off")
        );
        assert!(!set_custom_chat_button_enabled_in(
            &settings, "btn-1", false
        ));
        assert!(!move_custom_chat_button_in(&settings, "btn-1", 1));
        assert!(!remove_custom_chat_button_in(&settings, "btn-1"));

        // Turning AI back on restores the user's saved choices intact.
        assert!(settings.set_text(GROUP, ENABLED_KEY, ON));
        let fresh = Settings::at(&file);
        assert!(!builtin_chat_button_in(&fresh, "month"));
        assert!(builtin_chat_button_in(&fresh, "categories"));
        let restored = custom_chat_buttons_in(&fresh);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].label, "Quick");
        // Verify storage is in kassensturzrc under [AI].
        let raw = std::fs::read_to_string(&file).unwrap();
        assert!(raw.contains("[AI]"), "{raw}");
        assert!(raw.contains(CUSTOM_CHAT_BUTTONS_KEY), "{raw}");
    }
}
