// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's own folder in the Microsoft Store's package (D-856, #1178).
//!
//! Windows keeps everything a packaged app writes to `%LOCALAPPDATA%` in the
//! package's own `LocalCache` and deletes it when the app is uninstalled or
//! reset. Until D-856 the Store build kept its book and the backups there,
//! so uninstalling it took the book and every backup with it (B-382). Now
//! its own folder is `Documents\KassenSturz` (`platform::app_folder`), which
//! Windows leaves alone. What the earlier folder holds comes over in two
//! steps, both without replacing anything:
//!
//! 1. **At the start, once:** everything but the books is copied into the
//!    new folder: the backups, the bank connection's folder, the plugins.
//!    Copied, not moved, so the earlier folder stays as it was.
//! 2. **A book still in the earlier folder** is offered a move into the new
//!    one once a start, with "Not now" and "Keep it there". The move is the
//!    one of D-731: a backup first, then copy, check, switch and remove.
//!    A book is never copied, so there are never two working books.
//!
//! The logic is in `kassensturz_core::store_folder`, so the Windows CI job
//! tests it on Windows itself (B-443); this module reads the system and
//! hands the facts over.
//!
//! Outside a Windows app package, and in the test profile, nothing here
//! runs. Paths are never logged (D-730).

use std::path::{Path, PathBuf};

use crate::settings::Settings;
use kassensturz_core::store_folder::{BANK_MOVE_FAILED, GROUP, MOVE_BANK, copy_once};
pub use kassensturz_core::store_folder::{
    FOLDER_NAME, answer_bank_move, bank_move_failed, in_folder, keep_in_sync_folder,
    keep_where_it_is, move_bank_folder, offer_bank_move, offer_move, offer_sync_move,
};

/// Whether the app runs in a snap or in the Store's package: where the bank
/// connection's folder stays in the app's own data folder (B-429, B-437).
fn packaged() -> bool {
    crate::platform::in_snap_sandbox() || crate::platform::in_app_package()
}

/// Copies the earlier folder's files into the new one once, at the start of
/// the Store build. Call it after the application object exists and before
/// any book opens.
pub fn copy_at_start() {
    if crate::test_profile::on() || !crate::platform::own_folder_outside_app_data() {
        return;
    }
    let (Some(from), Some(to)) = (
        crate::platform::qt_app_folder(),
        crate::platform::app_folder(),
    ) else {
        return;
    };
    let settings = Settings::user();
    if let Err(error) = copy_once(&settings, &from, &to, packaged()) {
        // Tried again at the next start; the earlier folder is unchanged.
        eprintln!("kassensturz: couldn't copy the files of the earlier folder: {error}");
    }
    move_bank_folder_at_start(&settings);
}

/// The bank connection's folder the user agreed to move out of
/// `Documents\KassenSturz` (D-967): moved at the start, before anything
/// uses it. A move that fails leaves it where it was, and it is tried again
/// at the next start.
fn move_bank_folder_at_start(settings: &Settings) {
    if !crate::platform::in_app_package() || settings.text(GROUP, MOVE_BANK) != "true" {
        return;
    }
    let (Some(documents), Some(private)) = (
        crate::platform::store_documents_folder(),
        crate::platform::qt_app_folder(),
    ) else {
        return;
    };
    match move_bank_folder(&documents, &private) {
        Ok(()) => {
            let _ = settings.set_text(GROUP, MOVE_BANK, "done");
            let _ = settings.set_text(GROUP, BANK_MOVE_FAILED, "false");
        }
        Err(error) => {
            eprintln!("kassensturz: couldn't move the bank connection's folder: {error}");
            // The window tells the user (a GUI app has no console).
            let _ = settings.set_text(GROUP, BANK_MOVE_FAILED, "true");
        }
    }
}

/// The folder Windows deletes with the app, where the Store build kept its
/// books before D-856; `None` outside the Store build and in the test
/// profile, so nothing is ever offered there.
pub fn earlier_folder() -> Option<PathBuf> {
    if crate::test_profile::on() || !crate::platform::own_folder_outside_app_data() {
        return None;
    }
    crate::platform::qt_app_folder()
}

/// Makes the local own folder `local` ready for a book that moves out of
/// OneDrive (D-968); see `kassensturz_core::store_folder::prepare_local_folder`.
pub fn prepare_local_folder(
    documents: &Path,
    local: &Path,
) -> std::io::Result<kassensturz_core::store_folder::Copied> {
    kassensturz_core::store_folder::prepare_local_folder(documents, local, packaged())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only a Windows app package has the folder Windows deletes with the
    /// app; nothing else is ever offered a move or copies a file.
    #[cfg(not(windows))]
    #[test]
    fn outside_a_windows_package_there_is_no_earlier_folder() {
        assert!(!crate::platform::in_app_package());
        assert!(earlier_folder().is_none());
    }

    /// The settings file answers the way the core's logic reads it.
    #[test]
    fn the_settings_file_keeps_the_answers_of_the_offers() {
        let folder = crate::test_folder::TestFolder::new();
        let settings = Settings::at(&folder.join("kassensturzrc"));
        let book = folder.join("Household.ksz");
        assert!(!bank_move_failed(&settings));
        assert!(keep_where_it_is(&settings, &book));
        assert!(!offer_move(&settings, &book, folder.path()));
        assert!(answer_bank_move(&settings, true));
        assert_eq!(settings.text(GROUP, MOVE_BANK), "true");
    }
}
