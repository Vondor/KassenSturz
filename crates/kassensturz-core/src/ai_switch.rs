// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether AI is switched on (D-013, D-168), read without Qt.
//!
//! The one "AI features" toggle lives in the app's settings file next to the
//! other settings that belong to this computer, never in a book (D-168). The
//! app writes it with KConfig; programs without Qt, such as
//! `kassensturz-mcp`, read it here. Anything but an explicit `Enabled=true`
//! in the `[AI]` group, including a missing, unreadable or odd file, means
//! off: AI never starts on a guess (AGENTS.md §8).
//!
//! This module exists only in a build with AI (feature `ai`, D-039).

use std::path::{Path, PathBuf};

/// The settings group of the AI settings.
pub const GROUP: &str = "AI";

/// The key of the toggle in [`GROUP`].
pub const ENABLED_KEY: &str = "Enabled";

/// The value that switches AI on; every other value is off.
pub const ON: &str = "true";

/// The key of the second toggle in [`GROUP`]: whether the user's own AI
/// tools may prepare changes through `kassensturz-mcp` (D-035, D-219). It
/// counts only while AI itself is on.
pub const WRITE_TOOLS_KEY: &str = "McpWriteTools";

/// The settings file's name in the user's settings folder.
const FILE: &str = "kassensturzrc";

/// Whether the settings file's `text` switches AI on.
pub fn enabled_in(text: &str) -> bool {
    value_in(text, GROUP, ENABLED_KEY) == Some(ON)
}

/// Whether the settings file's `text` switches AI and the MCP write tools
/// on. The write tools are off whenever AI is (D-013).
pub fn write_tools_in(text: &str) -> bool {
    enabled_in(text) && value_in(text, GROUP, WRITE_TOOLS_KEY) == Some(ON)
}

/// The value of `key` in `group` of the settings file's `text`, with the
/// spaces around it removed, as KConfig wrote it: the last entry of a key
/// wins, as in KConfig. KConfig's escapes (`\s`, `\\` and so on) are left
/// as they are, for the caller to refuse or read.
pub fn value_in<'t>(text: &'t str, group: &str, key: &str) -> Option<&'t str> {
    let mut in_group = false;
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_group = name == group;
        } else if in_group
            && let Some((name, value)) = line.split_once('=')
            && name.trim_end() == key
        {
            found = Some(value.trim());
        }
    }
    found
}

/// Whether the settings file at `path` switches AI on. A file that can't be
/// read is off.
pub fn enabled_at(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| enabled_in(&text))
}

/// Where KConfig keeps the app's settings file, where this build knows it.
/// `None` elsewhere, which counts as off until the Windows and macOS
/// packaging steps (M7) say where their settings go.
pub fn settings_file() -> Option<PathBuf> {
    if cfg!(not(target_os = "linux")) {
        return None;
    }
    let folder = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(folder) if !folder.is_empty() => PathBuf::from(folder),
        _ => {
            PathBuf::from(std::env::var_os("HOME").filter(|home| !home.is_empty())?).join(".config")
        }
    };
    Some(folder.join(FILE))
}

/// Whether AI is switched on in this user's settings.
pub fn enabled() -> bool {
    settings_file().is_some_and(|path| enabled_at(&path))
}

/// Whether AI and the MCP write tools are both switched on in this user's
/// settings. A file that can't be read is off.
pub fn write_tools_enabled() -> bool {
    settings_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .is_some_and(|text| write_tools_in(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_explicit_true_in_the_ai_group_is_on() {
        assert!(!enabled_in(""));
        assert!(!enabled_in("[AI]\n"));
        assert!(enabled_in("[AI]\nEnabled=true\n"));
        assert!(enabled_in("[Books]\nLastBook=/a\n\n[AI]\nEnabled = true\n"));
        assert!(!enabled_in("[AI]\nEnabled=false\n"));
        assert!(!enabled_in("[AI]\nEnabled=1\n"));
        assert!(!enabled_in("[AI]\nEnabled=True\n"));
        // The key in another group, or another key, doesn't count.
        assert!(!enabled_in("[Other]\nEnabled=true\n"));
        assert!(!enabled_in("[AI]\nEnabledX=true\n"));
        // The last entry wins.
        assert!(!enabled_in("[AI]\nEnabled=true\nEnabled=false\n"));
        assert!(enabled_in("[AI]\nEnabled=false\nEnabled=true\n"));
    }

    #[test]
    fn values_are_read_from_their_group() {
        let text = "[AI]\nModel = a\n[Other]\nModel=b\n[AI]\nModel=c\nEmpty=\n";
        assert_eq!(value_in(text, "AI", "Model"), Some("c"));
        assert_eq!(value_in(text, "Other", "Model"), Some("b"));
        assert_eq!(value_in(text, "AI", "Empty"), Some(""));
        assert_eq!(value_in(text, "AI", "Missing"), None);
        assert_eq!(value_in(text, "None", "Model"), None);
    }

    #[test]
    fn the_write_tools_are_on_only_with_ai_on() {
        assert!(!write_tools_in(""));
        assert!(!write_tools_in("[AI]\nMcpWriteTools=true\n"));
        assert!(!write_tools_in("[AI]\nEnabled=true\n"));
        assert!(!write_tools_in("[AI]\nEnabled=false\nMcpWriteTools=true\n"));
        assert!(!write_tools_in("[AI]\nEnabled=true\nMcpWriteTools=1\n"));
        assert!(write_tools_in("[AI]\nEnabled=true\nMcpWriteTools=true\n"));
        assert!(!write_tools_in(
            "[AI]\nEnabled=true\n[Other]\nMcpWriteTools=true\n"
        ));
    }

    #[test]
    fn a_missing_file_is_off() {
        assert!(!enabled_at(Path::new("/nonexistent/kassensturzrc")));
    }
}
