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

/// The settings file's name in the user's settings folder.
const FILE: &str = "kassensturzrc";

/// Whether the settings file's `text` switches AI on.
pub fn enabled_in(text: &str) -> bool {
    let mut in_group = false;
    let mut on = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_group = group == GROUP;
        } else if in_group
            && let Some((key, value)) = line.split_once('=')
            && key.trim_end() == ENABLED_KEY
        {
            // The last entry of a key wins, as in KConfig.
            on = value.trim() == ON;
        }
    }
    on
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
    fn a_missing_file_is_off() {
        assert!(!enabled_at(Path::new("/nonexistent/kassensturzrc")));
    }
}
