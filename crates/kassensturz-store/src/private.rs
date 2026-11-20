// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Files and folders that hold a whole book, made readable only by the
//! user who runs KassenSturz (B-414): a new book, a backup, a copy of one
//! and the backup folder. On a computer that other people use with their
//! own accounts, their programs can't read the household's figures, also
//! where the home folder is open to everyone. The debug log and the MCP
//! request folder do the same.
//!
//! Only Linux and macOS (and the other Unix systems) have these permission
//! bits; on Windows a file in the user's own folders is the user's anyway,
//! and these functions do what the plain ones do.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Permissions of a file only its owner may read and write.
#[cfg(unix)]
pub(crate) const FILE_MODE: u32 = 0o600;

/// Permissions of a folder only its owner may open.
#[cfg(unix)]
pub(crate) const FOLDER_MODE: u32 = 0o700;

/// Creates the file at `path` for writing, failing if anything is there,
/// readable only by the user.
pub(crate) fn create_new(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, FILE_MODE);
    options.open(path)
}

/// Creates the folder at `path` and any missing folders above it, those it
/// creates readable only by the user, and makes `path` itself readable only
/// by the user if it was open to others: it is KassenSturz's own folder, so
/// the backups made before this version, which other users could read, are
/// covered too.
pub(crate) fn create_own_folder(path: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, FOLDER_MODE);
    builder.create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::symlink_metadata(path)?;
        // Never through a link: the folder it points to isn't KassenSturz's
        // to change.
        if metadata.is_dir() && metadata.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(FOLDER_MODE))?;
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::test_folder::TempFolder;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path)
            .expect("metadata read")
            .permissions()
            .mode()
            & 0o777
    }

    #[test]
    fn a_new_file_is_the_users_alone() {
        let folder = TempFolder::new();
        let path = folder.join("Household.ksz");
        drop(create_new(&path).expect("created"));
        assert_eq!(mode(&path), FILE_MODE);
        assert_eq!(
            create_new(&path).map(drop).map_err(|error| error.kind()),
            Err(io::ErrorKind::AlreadyExists)
        );
    }

    #[test]
    fn the_own_folder_is_made_and_closed_to_others() {
        let folder = TempFolder::new();
        let made = folder.join("data/backups");
        create_own_folder(&made).expect("made");
        assert_eq!(mode(&made), FOLDER_MODE);

        // A folder an older version made, open to everyone.
        let older = folder.join("older");
        std::fs::create_dir(&older).expect("made");
        std::fs::set_permissions(&older, std::fs::Permissions::from_mode(0o755)).expect("opened");
        create_own_folder(&older).expect("made");
        assert_eq!(mode(&older), FOLDER_MODE);
    }
}
