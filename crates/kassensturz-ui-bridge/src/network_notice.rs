// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The yellow note that a book is on a network drive (D-038) is shown until
//! the user says "Don't show again" for that book (B-321, D-802). The
//! books they said it for are kept in the app's settings, not in the book:
//! nothing about the book's layout changes. A book is told apart by where
//! it is, so a moved or copied book gets the note again. Only the note is
//! hidden: nothing else about the book, its backups or its warnings at open
//! changes.

use std::path::Path;

use crate::settings::Settings;

/// The settings group that remembers the books the note is hidden for.
const GROUP: &str = "NetworkDriveNote";
/// The key, with one book's place per line.
const KEY: &str = "Hidden";
/// How many books are remembered; the oldest are forgotten first.
const REMEMBERED: usize = 200;

/// How a book is remembered: its place on one line.
fn key_of(book: &Path) -> String {
    book.to_string_lossy()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// The books the user hid the note for.
fn hidden(settings: &Settings) -> Vec<String> {
    settings
        .text(GROUP, KEY)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Whether the note is shown for the book at `book`: not if the user hid it.
pub fn shown_for(settings: &Settings, book: &Path) -> bool {
    !hidden(settings).contains(&key_of(book))
}

/// Remembers that the note is hidden for the book at `book`. Returns whether
/// it was saved.
pub fn hide_for(settings: &Settings, book: &Path) -> bool {
    let mut all = hidden(settings);
    let key = key_of(book);
    if !all.contains(&key) {
        all.push(key);
    }
    let start = all.len().saturating_sub(REMEMBERED);
    settings.set_text(GROUP, KEY, &all[start..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_folder::TestFolder;

    #[test]
    fn the_note_is_shown_until_hidden_for_that_book_only() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        let one = Path::new("/mnt/share/One.ksz");
        let other = Path::new("/mnt/share/Other.ksz");
        assert!(shown_for(&settings, one));
        assert!(hide_for(&settings, one));
        assert!(hide_for(&settings, one));
        // Kept in the file: a new start still has it hidden.
        let again = Settings::at(&file);
        assert!(!shown_for(&again, one));
        assert!(shown_for(&again, other));
        // A moved or copied book is another place, so it gets the note.
        assert!(shown_for(&again, Path::new("/mnt/share/Copy of One.ksz")));
        assert_eq!(hidden(&again).len(), 1);
    }

    #[test]
    fn a_settings_file_without_the_key_loads_and_shows_the_note() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        // A file as an earlier build wrote it: other groups, no such key.
        std::fs::write(&file, "[RuleSuggestions]\nDismissed=a\n").expect("file written");
        let settings = Settings::at(&file);
        assert!(shown_for(&settings, Path::new("/mnt/share/One.ksz")));
        assert!(hide_for(&settings, Path::new("/mnt/share/One.ksz")));
        // The other group is still there after the new key is added.
        assert_eq!(settings.text("RuleSuggestions", "Dismissed"), "a");
    }

    #[test]
    fn the_oldest_books_are_forgotten_first() {
        let folder = TestFolder::new();
        let settings = Settings::at(&folder.join("kassensturzrc"));
        for number in 0..REMEMBERED + 3 {
            assert!(hide_for(
                &settings,
                Path::new(&format!("/mnt/{number}.ksz"))
            ));
        }
        assert!(shown_for(&settings, Path::new("/mnt/0.ksz")));
        assert!(!shown_for(
            &settings,
            Path::new(&format!("/mnt/{}.ksz", REMEMBERED + 2))
        ));
    }
}
