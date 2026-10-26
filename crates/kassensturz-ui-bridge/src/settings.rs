// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The app's own settings, kept with KConfig the KDE way (D-112): what
//! belongs to this computer rather than to a book, such as the book opened
//! last (D-033) and the number format (D-002). A book's own settings stay in
//! the book.

use std::path::{Path, PathBuf};

use cxx_qt_lib::QString;
use kassensturz_platform::SecretStorage;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
    }

    #[namespace = "kassensturz::settings"]
    unsafe extern "C++" {
        include!("kassensturz-ui-bridge/cpp/settings.h");

        #[cxx_name = "readEntry"]
        fn read_entry(file: &QString, group: &QString, key: &QString) -> QString;

        #[cxx_name = "writeEntry"]
        fn write_entry(file: &QString, group: &QString, key: &QString, value: &QString) -> bool;

        #[cxx_name = "readPathEntry"]
        fn read_path_entry(file: &QString, group: &QString, key: &QString) -> QString;

        #[cxx_name = "writePathEntry"]
        fn write_path_entry(file: &QString, group: &QString, key: &QString, path: &QString)
        -> bool;
    }
}

/// The settings file in the user's settings folder, e.g.
/// `~/.config/kassensturzrc` on Linux.
const USER_FILE: &str = "kassensturzrc";

/// The group of the settings about books.
const BOOKS: &str = "Books";

/// The book opened last, which the app opens at the next start.
const LAST_BOOK: &str = "LastBook";

/// The group of the settings about bank connections.
const BANK: &str = "Bank";

/// Where saved PINs go (D-163).
const PIN_STORAGE: &str = "PinStorage";

/// How long after the last fetch the overview warns (D-180).
const FETCH_GAP: &str = "FetchGapDays";

/// The group and key of the main window's size and place.
const WINDOW: &str = "Window";
const WINDOW_GEOMETRY: &str = "Geometry";

/// The app's settings in one settings file.
pub struct Settings {
    file: QString,
}

impl Settings {
    /// The user's own settings.
    pub fn user() -> Self {
        Self {
            file: QString::from(USER_FILE),
        }
    }

    /// The settings in the file at `path`, e.g. for a test.
    #[cfg(test)]
    pub fn at(path: &Path) -> Self {
        Self {
            file: QString::from(path.to_string_lossy().as_ref()),
        }
    }

    /// The book opened last, if one was.
    pub fn last_book(&self) -> Option<PathBuf> {
        let path =
            ffi::read_path_entry(&self.file, &QString::from(BOOKS), &QString::from(LAST_BOOK))
                .to_string();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    /// Remembers `path` as the book opened last. Returns whether that was
    /// saved. A path that isn't valid Unicode isn't, since KConfig keeps
    /// text: the app then opens its own book at the next start.
    pub fn set_last_book(&self, path: &Path) -> bool {
        let Some(path) = path.to_str() else {
            return false;
        };
        ffi::write_path_entry(
            &self.file,
            &QString::from(BOOKS),
            &QString::from(LAST_BOOK),
            &QString::from(path),
        )
    }

    /// Where the user chose to keep saved PINs (D-163); the system
    /// keychain when nothing, or nothing this build offers here, is kept.
    pub fn pin_storage(&self) -> SecretStorage {
        SecretStorage::from_key(&self.text(BANK, PIN_STORAGE))
    }

    /// Keeps where saved PINs go. Returns whether that was saved.
    pub fn set_pin_storage(&self, storage: SecretStorage) -> bool {
        self.set_text(BANK, PIN_STORAGE, storage.key())
    }

    /// The word kept for the fetch gap warning's limit (D-180): a number of
    /// days or "off"; empty when nothing is kept, which `fetch_gap::limit_of`
    /// reads as the default.
    pub fn fetch_gap_choice(&self) -> String {
        self.text(BANK, FETCH_GAP)
    }

    /// Keeps the fetch gap warning's limit. Returns whether it was saved.
    pub fn set_fetch_gap_choice(&self, choice: &str) -> bool {
        self.set_text(BANK, FETCH_GAP, choice)
    }

    /// The window's size and place as `window_state` keeps them; empty when
    /// nothing is kept.
    pub fn window_geometry(&self) -> String {
        self.text(WINDOW, WINDOW_GEOMETRY)
    }

    /// Keeps the window's size and place. Returns whether it was saved.
    pub fn set_window_geometry(&self, geometry: &str) -> bool {
        self.set_text(WINDOW, WINDOW_GEOMETRY, geometry)
    }

    /// The text kept under `key` in `group`; empty if there is none.
    pub fn text(&self, group: &str, key: &str) -> String {
        ffi::read_entry(&self.file, &QString::from(group), &QString::from(key)).to_string()
    }

    /// Keeps `value` under `key` in `group`. Returns whether it was saved.
    pub fn set_text(&self, group: &str, key: &str, value: &str) -> bool {
        ffi::write_entry(
            &self.file,
            &QString::from(group),
            &QString::from(key),
            &QString::from(value),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_folder::TestFolder;

    #[test]
    fn keeps_the_last_book_and_text_in_the_file() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert_eq!(settings.last_book(), None);
        assert_eq!(settings.text("Display", "DecimalPlaces"), "");

        let book = folder.join("Household.ksz");
        assert!(settings.set_last_book(&book));
        assert!(settings.set_text("Display", "DecimalPlaces", "0"));

        // Read back through a new handle, and as KConfig wrote the file.
        let again = Settings::at(&file);
        assert_eq!(again.last_book(), Some(book.clone()));
        assert_eq!(again.text("Display", "DecimalPlaces"), "0");
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(written.contains("[Books]"), "{written}");
        assert!(written.contains("[Display]\nDecimalPlaces=0"), "{written}");
    }

    #[test]
    fn keeps_where_saved_pins_go() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert_eq!(settings.pin_storage(), SecretStorage::System);
        assert!(settings.set_pin_storage(SecretStorage::Nowhere));
        assert_eq!(Settings::at(&file).pin_storage(), SecretStorage::Nowhere);
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(written.contains("[Bank]\nPinStorage=nowhere"), "{written}");
        // A word another build wrote gives the default.
        assert!(settings.set_text("Bank", "PinStorage", "elsewhere"));
        assert_eq!(settings.pin_storage(), SecretStorage::System);
    }

    #[test]
    fn keeps_the_fetch_gap_limit() {
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        assert_eq!(settings.fetch_gap_choice(), "");
        assert!(settings.set_fetch_gap_choice("off"));
        assert_eq!(Settings::at(&file).fetch_gap_choice(), "off");
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(written.contains("[Bank]\nFetchGapDays=off"), "{written}");
    }

    #[test]
    fn keeps_a_book_in_the_home_folder_relative_to_it() {
        let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) else {
            return;
        };
        let folder = TestFolder::new();
        let file = folder.join("kassensturzrc");
        let settings = Settings::at(&file);
        let book = PathBuf::from(&home).join("Books/Household.ksz");
        assert!(settings.set_last_book(&book));
        let written = std::fs::read_to_string(&file).expect("settings file written");
        assert!(
            written.contains("LastBook[$e]=$HOME/Books/Household.ksz"),
            "{written}"
        );
        assert_eq!(settings.last_book(), Some(book));
    }
}
