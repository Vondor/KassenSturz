// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Exporter interface every format implements (D-007, D-069).

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, ErrorKind, Write};
use std::path::Path;

use crate::data::ExportData;
use crate::error::ExportError;

/// Permissions of a new export file on Unix: readable and writable only by
/// its owner, matching books and backups (B-414, T-71).
#[cfg(unix)]
const EXPORT_FILE_MODE: u32 = 0o600;

/// Creates the file at `path` for writing, failing if anything is there,
/// readable only by the user on Unix (B-414, T-71).
fn create_new(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, EXPORT_FILE_MODE);
    options.open(path)
}

/// Writes one file format, e.g. CSV.
///
/// Built-in exporters implement it directly; the WASM plugin API mirrors it
/// after 1.0, so a plugin exporter works the same way (D-069, D-071). An
/// exporter only writes: it gets the data and a place to write to, and never
/// sees a book. Settings a format needs, such as CSV's separator, are the
/// exporter value's own, given when it is created, so the interface stays the
/// same for every format.
///
/// Front ends call [`export`] or [`export_file`] rather than
/// [`Exporter::write`], so the data is checked and no half-written file is
/// left behind whatever the exporter does.
pub trait Exporter {
    /// What the exporter writes.
    fn info(&self) -> ExporterInfo;

    /// Writes `data` to `out`, in the order it has, exactly: an amount is
    /// never rounded, and nothing is left out without being said (D-029).
    /// It checks `data` itself ([`ExportData::check`]) as the first thing,
    /// since it can be called directly.
    fn write(&self, data: &ExportData, out: &mut dyn Write) -> Result<(), ExportError>;
}

/// What an exporter writes, for choosing it: in the file dialog, on the
/// command line and in settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExporterInfo {
    /// A stable ID: lowercase ASCII letters, digits and hyphens, e.g.
    /// `csv`. It names the exporter on the command line and in saved
    /// settings, and never changes. The UI shows the format's name, which it
    /// keeps itself (D-034).
    pub id: &'static str,
    /// What it writes.
    pub kind: ExportKind,
    /// The file name extension the format has, lowercase and without the
    /// dot, e.g. `csv`.
    pub extension: &'static str,
}

/// What an exporter writes, known before anything is gathered from the book,
/// so the front end collects the right data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExportKind {
    /// The transactions of accounts ([`ExportData::Transactions`]).
    Transactions,
    /// A whole book ([`ExportData::Book`]).
    Book,
}

/// Writes `data` with `exporter` and returns the bytes of the file.
pub fn export(exporter: &dyn Exporter, data: &ExportData) -> Result<Vec<u8>, ExportError> {
    data.check()?;
    let mut bytes = Vec::new();
    exporter.write(data, &mut bytes)?;
    Ok(bytes)
}

/// Writes `data` with `exporter` to a new file at `path`.
///
/// On Unix, a newly created or pre-created empty file is readable and
/// writable only by its owner (`0o600`, aligning with B-414 / T-71). Never
/// overwrites: if something is
/// at `path`, it fails with [`ExportError::AlreadyExists`], naming the file,
/// and leaves it alone. The one exception is an empty file, which holds
/// nothing to lose: a sandbox's file chooser (xdg-desktop-portal, in the
/// Flatpak) makes the file the user named before the app can write to it,
/// and it is written to in place, since the app may not be allowed to remove
/// it (D-125). If writing fails, the file this call began is removed again,
/// or emptied again if it was there already, so there is either a complete
/// file or none.
pub fn export_file(
    exporter: &dyn Exporter,
    data: &ExportData,
    path: &Path,
) -> Result<(), ExportError> {
    data.check()?;
    let (file, was_there) = match create_new(path) {
        Ok(file) => (file, false),
        // Windows says "permission denied", not "already exists", when the
        // name is a folder.
        Err(error)
            if error.kind() == ErrorKind::AlreadyExists
                || (error.kind() == ErrorKind::PermissionDenied && path.exists()) =>
        {
            let empty = fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() == 0);
            let file = empty
                .then(|| OpenOptions::new().write(true).open(path).ok())
                .flatten()
                .ok_or_else(|| ExportError::AlreadyExists {
                    file: path.display().to_string(),
                })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(EXPORT_FILE_MODE))?;
            }
            (file, true)
        }
        Err(error) => return Err(error.into()),
    };
    let mut out = BufWriter::new(file);
    let written = exporter
        .write(data, &mut out)
        .and_then(|()| out.flush().map_err(ExportError::from))
        .and_then(|()| out.get_ref().sync_all().map_err(ExportError::from));
    if written.is_err() {
        // Nothing more can be done if even this fails.
        if was_there {
            // The buffer is thrown away, not flushed: dropping the writer
            // would write what is left in it after the file was emptied.
            let (file, _) = out.into_parts();
            let _ = file.set_len(0);
        } else {
            drop(out);
            let _ = fs::remove_file(path);
        }
    }
    written
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use kassensturz_core::RoundingMode;

    use super::*;
    use crate::{BookData, BookExport, CsvExport};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "kassensturz-export-mode-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("test folder");
            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path)
            .expect("metadata read")
            .permissions()
            .mode()
            & 0o777
    }

    fn empty_book() -> BookData {
        BookData {
            rounding_mode: RoundingMode::HalfAwayFromZero,
            history_keep_days: None,
            currencies: Vec::new(),
            accounts: Vec::new(),
            rates: Vec::new(),
            rules: Vec::new(),
            transactions: Vec::new(),
            rule_marks: Vec::new(),
            tags_by_rule: Vec::new(),
            tags_taken_off: Vec::new(),
            institutions: Vec::new(),
            payees: Vec::new(),
            tags: Vec::new(),
            bank_links: Vec::new(),
            views: Vec::new(),
            groups: Vec::new(),
            hidden_categories: Vec::new(),
            budgets: Vec::new(),
            schedules: Vec::new(),
            recurring_moves: Vec::new(),
        }
    }

    fn create_empty_with_mode(path: &Path, file_mode: u32) {
        File::create(path).expect("empty file created");
        fs::set_permissions(path, fs::Permissions::from_mode(file_mode)).expect("set mode");
        assert_eq!(mode(path), file_mode);
    }

    #[test]
    fn exported_csv_file_has_owner_only_permissions() {
        let dir = TempDir::new("csv");
        let path = dir.join("transactions.csv");
        export_file(
            &CsvExport::default(),
            &ExportData::Transactions(Vec::new()),
            &path,
        )
        .expect("csv exported");
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn exported_book_json_file_has_owner_only_permissions() {
        let dir = TempDir::new("book");
        let path = dir.join("book.json");
        export_file(&BookExport, &ExportData::Book(empty_book()), &path).expect("book exported");
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn precreated_empty_csv_file_gets_owner_only_permissions() {
        let dir = TempDir::new("precreated-csv");
        let path = dir.join("transactions.csv");
        create_empty_with_mode(&path, 0o644);
        export_file(
            &CsvExport::default(),
            &ExportData::Transactions(Vec::new()),
            &path,
        )
        .expect("csv exported");
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn precreated_empty_book_json_file_gets_owner_only_permissions() {
        let dir = TempDir::new("precreated-book");
        let path = dir.join("book.json");
        create_empty_with_mode(&path, 0o644);
        export_file(&BookExport, &ExportData::Book(empty_book()), &path).expect("book exported");
        assert_eq!(mode(&path), 0o600);
    }
}
