// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing a small file the user chose to replace (M6d.3, D-443, D-458): a
//! dashboard file. Unlike [`crate::export_file`], an existing file is
//! replaced, because the file chooser has asked the user already.

use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::ExportError;

/// How many names for the temporary file are tried before writing in place.
const TRIES: u32 = 8;

/// Writes `bytes` to `path`, replacing a file that is there.
///
/// The bytes go into a new temporary file next to it first, which is then
/// renamed over it, so a failed write leaves the old file as it was (or no
/// file). Where the folder doesn't let the app make a file of its own (the
/// file chooser's portal gives it only the file the user named, D-125), the
/// file is written in place instead: the old file is read first and, if
/// the write fails, written back, and a file this call made is removed
/// again, so there is never a half-written or emptied file (M6-B136). An
/// old file that can't be read is not touched at all.
pub fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    match temporary(path) {
        Some((temporary_path, file)) => {
            let written = write_all(file, bytes)
                .and_then(|()| fs::rename(&temporary_path, path).map_err(ExportError::from));
            if written.is_err() {
                // Nothing more can be done if even this fails.
                let _ = fs::remove_file(&temporary_path);
            }
            written
        }
        None => write_in_place(path, bytes),
    }
}

/// A new, empty file next to `path`, if the folder lets the app make one.
fn temporary(path: &Path) -> Option<(PathBuf, File)> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    (0..TRIES).find_map(|try_number| {
        let temporary_path = path.with_file_name(format!(".{name}.{try_number}.partial"));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .ok()
            .map(|file| (temporary_path, file))
    })
}

fn write_all(mut file: File, bytes: &[u8]) -> Result<(), ExportError> {
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Writes `bytes` over the file at `path`, or into a new one.
fn write_in_place(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    write_in_place_with(path, bytes, |file, bytes| file.write_all(bytes))
}

/// [`write_in_place`] with the writing step given, so a test can make it
/// fail half-way. The old content is read before anything changes: when
/// it can't be read, nothing is written. After a failed write the old
/// content is written back; the space it needs was just freed by the
/// truncation, so this works unless something else took that space.
fn write_in_place_with(
    path: &Path,
    bytes: &[u8],
    mut write: impl FnMut(&mut File, &[u8]) -> std::io::Result<()>,
) -> Result<(), ExportError> {
    let old = if path.exists() {
        Some(fs::read(path)?)
    } else {
        None
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    let written = write(&mut file, bytes)
        .and_then(|()| file.sync_all())
        .map_err(ExportError::from);
    if written.is_err() {
        match old {
            // Nothing more can be done if even this fails; the error of
            // the write is the one to report.
            Some(old) => {
                let _ = restore(&mut file, &old);
            }
            None => {
                drop(file);
                let _ = fs::remove_file(path);
            }
        }
    }
    written
}

/// Writes `old` back as the whole content of `file`.
fn restore(file: &mut File, old: &[u8]) -> std::io::Result<()> {
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(old)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use super::*;

    /// A new, empty folder for one test.
    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("kassensturz-replace-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).expect("folder made");
        folder
    }

    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .expect("listed")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_new_file_is_written_and_nothing_else_stays() {
        let folder = folder("new");
        let path = folder.join("Spending.dashboard.json");
        replace_file(&path, b"{}\n").expect("written");
        assert_eq!(fs::read(&path).expect("read"), b"{}\n");
        assert_eq!(names(&folder), ["Spending.dashboard.json"]);
    }

    #[test]
    fn an_existing_file_is_replaced_whole() {
        let folder = folder("existing");
        let path = folder.join("a.json");
        fs::write(&path, "a much longer old content than the new one").expect("written");
        // A temporary file left by a crash does not stop the next save.
        fs::write(folder.join(".a.json.0.partial"), "old").expect("written");
        replace_file(&path, b"new").expect("replaced");
        assert_eq!(fs::read(&path).expect("read"), b"new");
        assert_eq!(names(&folder), [".a.json.0.partial", "a.json"]);
    }

    #[test]
    fn a_folder_is_not_replaced_and_no_temporary_file_stays() {
        let folder = folder("directory");
        let path = folder.join("taken");
        fs::create_dir(&path).expect("folder made");
        let error = replace_file(&path, b"new").expect_err("a folder");
        assert!(matches!(error, ExportError::Io(_)), "{error:?}");
        assert!(path.is_dir());
        assert_eq!(names(&folder), ["taken"]);
    }

    #[test]
    fn a_missing_folder_says_so() {
        let folder = folder("missing");
        let path = folder.join("not-there").join("a.json");
        let error = replace_file(&path, b"new").expect_err("no folder");
        assert!(
            matches!(error, ExportError::Io(ErrorKind::NotFound)),
            "{error:?}"
        );
    }

    #[test]
    fn writing_in_place_replaces_an_existing_file_whole() {
        let folder = folder("in-place");
        let path = folder.join("a.json");
        fs::write(&path, "a much longer old content than the new one").expect("written");
        write_in_place(&path, b"new").expect("written");
        assert_eq!(fs::read(&path).expect("read"), b"new");
        let other = folder.join("b.json");
        write_in_place(&other, b"fresh").expect("written");
        assert_eq!(fs::read(&other).expect("read"), b"fresh");
        assert_eq!(names(&folder), ["a.json", "b.json"]);
    }

    /// Writes half of the bytes, then fails as a full disk would.
    fn half_then_full(file: &mut File, bytes: &[u8]) -> std::io::Result<()> {
        file.write_all(&bytes[..bytes.len() / 2])?;
        Err(std::io::Error::from(ErrorKind::StorageFull))
    }

    #[test]
    fn a_failed_write_in_place_brings_the_old_file_back() {
        // M6-B136: the old file is neither emptied nor left half-written.
        let folder = folder("in-place-failed");
        let path = folder.join("a.json");
        fs::write(&path, "the old dashboard").expect("written");
        let error = write_in_place_with(&path, b"a new and longer dashboard", half_then_full)
            .expect_err("disk full");
        assert!(
            matches!(error, ExportError::Io(ErrorKind::StorageFull)),
            "{error:?}"
        );
        assert_eq!(fs::read(&path).expect("read"), b"the old dashboard");
        // A file the call made is removed again.
        let other = folder.join("b.json");
        write_in_place_with(&other, b"fresh", half_then_full).expect_err("disk full");
        assert_eq!(names(&folder), ["a.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn an_old_file_that_cant_be_read_is_not_touched() {
        use std::os::unix::fs::PermissionsExt;
        let folder = folder("in-place-unreadable");
        let path = folder.join("a.json");
        fs::write(&path, "the old dashboard").expect("written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).expect("write only");
        if fs::read(&path).is_ok() {
            // Running as root reads anyway; nothing to check then.
            return;
        }
        assert!(write_in_place(&path, b"new").is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("readable");
        assert_eq!(fs::read(&path).expect("read"), b"the old dashboard");
    }
}
