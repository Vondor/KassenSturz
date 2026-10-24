// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the app learns about the computer from Qt: its name, KassenSturz's
//! data folder and what kind of drive a book is on. This moves to
//! `kassensturz-platform` once that exists (`docs/ARCHITECTURE.md`).

use std::path::{Path, PathBuf};

use cxx_qt_lib::QString;
use kassensturz_store::Computer;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
    }

    #[namespace = "kassensturz::platform"]
    unsafe extern "C++" {
        include!("kassensturz-ui-bridge/cpp/platform.h");

        #[cxx_name = "dataFolder"]
        fn data_folder() -> QString;

        #[cxx_name = "computerName"]
        fn computer_name() -> QString;

        #[cxx_name = "setWindowIcon"]
        fn set_window_icon(name: &QString);

        #[cxx_name = "fileSystemType"]
        fn file_system_type(path: &QString) -> QString;
    }
}

/// The app ID (D-049), also the name of KassenSturz's data folder, which the
/// command-line tool uses too.
pub const APP_ID: &str = "io.github.vondor.KassenSturz";

/// Gives the windows the app's icon from the icon theme, where the desktop
/// file isn't what the window is matched with (an X11 window manager, or a
/// Wayland compositor that doesn't read it) (D-163).
pub fn set_window_icon() {
    ffi::set_window_icon(&QString::from(APP_ID));
}

/// KassenSturz's own data folder, e.g. `~/.local/share/io.github.vondor.KassenSturz`
/// on Linux, where the app keeps its book and the backups (D-033).
pub fn app_folder() -> Option<PathBuf> {
    let folder = ffi::data_folder().to_string();
    (!folder.is_empty()).then(|| PathBuf::from(folder).join(APP_ID))
}

/// This computer, as a book's lock file and backups need it (D-038, D-080).
/// Without a data folder, backups go next to the app's book in the current
/// folder, which only a broken setup gets to.
pub fn computer() -> Computer {
    let name = ffi::computer_name().to_string();
    Computer {
        name: if name.trim().is_empty() {
            "this computer".to_owned()
        } else {
            name.trim().to_owned()
        },
        backup_folder: app_folder().unwrap_or_default().join("backups"),
    }
}

/// Whether the user can choose where a new book is kept. In the Flatpak
/// they can't: the app can't reach the user's files (D-113), and the file
/// chooser's portal would make an empty file the book can't live in
/// (`Drive::Sandboxed`), so a new book goes into the data folder. This
/// changes if the Flatpak is ever allowed the user's files (Q-071).
pub fn can_choose_book_place() -> bool {
    !in_flatpak(Path::new("/.flatpak-info"))
}

/// Whether the app runs in a Flatpak, which puts `marker` into every sandbox.
fn in_flatpak(marker: &Path) -> bool {
    cfg!(target_os = "linux") && marker.exists()
}

/// What kind of drive a book is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drive {
    /// A drive of this computer, or one Qt can't tell.
    Local,
    /// A network drive (SMB, NFS, ...), where SQLite's writes and locks
    /// aren't reliable, so the book opens with a warning (D-038).
    Network,
    /// A file a sandbox's file chooser handed over one at a time, e.g. in
    /// the Flatpak: the book's lock file and SQLite's journal can't be made
    /// next to it, so the book can't be opened from there.
    Sandboxed,
}

/// The drive `path` is on. For a file that isn't there yet, it's the drive
/// of the folder it would be in.
pub fn drive(path: &Path) -> Drive {
    let existing = if path.exists() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let file_system = ffi::file_system_type(&QString::from(existing.to_string_lossy().as_ref()));
    drive_of(&file_system.to_string())
}

/// The kind of drive with the file system called `file_system`, as Qt names
/// it from the system's list of mounted drives. On Windows, Qt names the
/// file system of the computer the network drive is on, e.g. "NTFS", so
/// network drives are recognized there once `kassensturz-platform` asks
/// Windows itself.
fn drive_of(file_system: &str) -> Drive {
    // Linux names, then macOS names. GVfs (GNOME) and KIO FUSE (Plasma)
    // reach network folders the file manager opened, such as smb:// ones.
    const NETWORK: &[&str] = &[
        "cifs",
        "smb3",
        "smbfs",
        "nfs",
        "nfs4",
        "afs",
        "ncpfs",
        "ceph",
        "glusterfs",
        "fuse.glusterfs",
        "fuse.sshfs",
        "fuse.rclone",
        "fuse.gvfsd-fuse",
        "fuse.kio-fuse",
        "davfs",
        "afpfs",
        "webdav",
    ];
    // xdg-desktop-portal's document portal, through which a Flatpak app
    // gets a file the user chose outside its sandbox.
    const SANDBOXED: &[&str] = &["fuse.portal"];
    let file_system = file_system.trim().to_lowercase();
    if NETWORK.contains(&file_system.as_str()) {
        Drive::Network
    } else if SANDBOXED.contains(&file_system.as_str()) {
        Drive::Sandboxed
    } else {
        Drive::Local
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_folder::TestFolder;

    #[test]
    fn a_flatpak_is_told_by_the_file_it_puts_into_every_sandbox() {
        let folder = TestFolder::new();
        let marker = folder.path().join(".flatpak-info");
        assert!(!in_flatpak(&marker));
        std::fs::write(&marker, b"").expect("marker");
        assert_eq!(in_flatpak(&marker), cfg!(target_os = "linux"));
    }

    #[test]
    fn tells_network_drives_and_sandbox_files_from_local_drives() {
        for file_system in [
            "cifs",
            "smb3",
            "nfs4",
            "fuse.sshfs",
            "fuse.gvfsd-fuse",
            "smbfs",
        ] {
            assert_eq!(drive_of(file_system), Drive::Network, "{file_system}");
        }
        assert_eq!(drive_of("NFS"), Drive::Network);
        assert_eq!(drive_of("fuse.portal"), Drive::Sandboxed);
        for file_system in [
            "ext4", "btrfs", "xfs", "tmpfs", "overlay", "apfs", "NTFS", "",
        ] {
            assert_eq!(drive_of(file_system), Drive::Local, "{file_system}");
        }
    }

    #[test]
    fn a_temporary_folder_is_a_local_drive() {
        let folder = TestFolder::new();
        assert_eq!(drive(folder.path()), Drive::Local);
        assert_eq!(drive(&folder.join("New book.ksz")), Drive::Local);
    }

    #[test]
    fn knows_the_computer_and_the_backup_folder() {
        let computer = computer();
        assert!(!computer.name.is_empty());
        assert!(
            computer
                .backup_folder
                .ends_with("io.github.vondor.KassenSturz/backups")
        );
    }
}
