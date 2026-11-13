// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the app learns about the computer from Qt: its name, KassenSturz's
//! data folder and what kind of drive a book is on. This moves to
//! `kassensturz-platform` once that exists (`docs/ARCHITECTURE.md`).

use std::path::{Path, PathBuf};

use cxx_qt_lib::QString;
use kassensturz_core::book_folder::{self, FolderGrant, FolderRefused, Surroundings};
use kassensturz_store::Computer;

#[cxx_qt::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        /// Qt's list of strings.
        type QStringList = cxx_qt_lib::QStringList;
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

        #[cxx_name = "freeSpace"]
        fn free_space(path: &QString) -> i64;

        #[cxx_name = "portalHostPath"]
        fn portal_host_path(portal_path: &QString) -> QString;

        #[cxx_name = "dataSize"]
        fn data_size(bytes: i64) -> QString;

        #[cxx_name = "renameNoReplace"]
        fn rename_no_replace(from: &QString, to: &QString) -> i32;

        #[cxx_name = "loadKI18nQml"]
        fn load_ki18n_qml();

        #[cxx_name = "useBundledIcons"]
        fn use_bundled_icons();

        #[cxx_name = "useReadableWidgetStyle"]
        fn use_readable_widget_style();

        #[cxx_name = "followSystemColours"]
        fn follow_system_colours();

        #[cxx_name = "startInUserFolder"]
        fn start_in_user_folder();

        #[cxx_name = "useTestLocations"]
        fn use_test_locations();

        #[cxx_name = "appFilePath"]
        fn app_file_path() -> QString;

        #[cxx_name = "startDetached"]
        fn start_detached(
            program: &QString,
            arguments: &QStringList,
            environment: &QStringList,
        ) -> bool;

        #[cxx_name = "quitApp"]
        fn quit_app();
    }
}

/// The app ID (D-049), also the name of KassenSturz's data folder, which the
/// command-line tool uses too.
pub const APP_ID: &str = "io.github.vondor.KassenSturz";

/// Gives the windows the app's icon from the icon theme, where the desktop
/// file isn't what the window is matched with (an X11 window manager, or a
/// Wayland compositor that doesn't read it) (D-165).
pub fn set_window_icon() {
    ffi::set_window_icon(&QString::from(APP_ID));
}

/// On Windows, Breeze's icons from the program's own folder, because Windows
/// has no icon theme (D-555); elsewhere the desktop's theme, so nothing here.
pub fn use_bundled_icons() {
    ffi::use_bundled_icons();
}

/// On Windows, Qt's Fusion style under Plasma's QML style, because Windows'
/// own style drew a chosen radio button empty (M6-B142); the user's own
/// choice of style wins. Elsewhere nothing changes.
pub fn use_readable_widget_style() {
    ffi::use_readable_widget_style();
}

/// On Windows, colours that follow Windows' light or dark mode, also while
/// the app runs (M6-B152, D-581). Elsewhere the desktop's colour scheme
/// applies, so nothing here.
pub fn follow_system_colours() {
    ffi::follow_system_colours();
}

/// On Windows, the user's Downloads folder as the working folder when the
/// app started in the system's folder or its own, so that file dialogs don't
/// open there (M6-B147). Elsewhere nothing changes.
pub fn start_in_user_folder() {
    ffi::start_in_user_folder();
}

/// Loads KI18n's QML library here, on the main thread, before the first QML
/// file loads. Qt would load it on its QML loader thread, and KI18n's event
/// filter object, a static inside it, would then belong to that thread, so
/// that Qt warns "Cannot filter events for objects in a different thread".
pub fn load_ki18n_qml() {
    ffi::load_ki18n_qml();
}

/// Switches Qt to its own test locations (`QStandardPaths::setTestModeEnabled`),
/// apart from the user's real ones, for the test profile on Windows, where
/// Qt doesn't read the variables the profile moves (Q-399). Call it before
/// Qt starts.
pub fn use_test_locations() {
    ffi::use_test_locations();
}

/// KassenSturz's own data folder, e.g. `~/.local/share/io.github.vondor.KassenSturz`
/// on Linux, where the app keeps its book and the backups (D-033).
pub fn app_folder() -> Option<PathBuf> {
    // In the test profile, the test folder's own (Q-399).
    if let Some(folder) = crate::test_profile::data_folder() {
        return Some(folder.join(APP_ID));
    }
    let folder = ffi::data_folder().to_string();
    (!folder.is_empty()).then(|| PathBuf::from(folder).join(APP_ID))
}

/// The app's own executable file.
pub fn app_file_path() -> String {
    ffi::app_file_path().to_string()
}

/// Starts what `plan` says on its own, outliving this process (see
/// `restart`). Returns whether it started.
pub fn start_detached(plan: &crate::restart::Plan) -> bool {
    let list = |items: &[String]| {
        items
            .iter()
            .map(|item| QString::from(item.as_str()))
            .collect::<cxx_qt_lib::QStringList>()
    };
    ffi::start_detached(
        &QString::from(plan.program.as_str()),
        &list(&plan.arguments),
        &list(plan.environment.as_deref().unwrap_or_default()),
    )
}

/// Ends the event loop, so the app ends.
pub fn quit() {
    ffi::quit_app();
}

/// Whether the app runs in a Flatpak sandbox.
pub fn in_flatpak_sandbox() -> bool {
    in_flatpak(Path::new("/.flatpak-info"))
}

/// Qt's own folder for apps' data, for the test profile's path audit.
#[cfg(test)]
pub fn qt_data_folder() -> PathBuf {
    PathBuf::from(ffi::data_folder().to_string())
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

/// Whether the user can choose where a new book is kept with a save dialog,
/// which hands over one file. In the Flatpak they can't: the file chooser's
/// portal would make an empty file the book can't live in
/// (`Drive::Sandboxed`), so a new book goes into the data folder until the
/// pages ask for a folder instead (`can_choose_book_folder`, D-730, D-733).
pub fn can_choose_book_place() -> bool {
    !in_flatpak(Path::new("/.flatpak-info"))
}

/// Whether the user can choose a folder for their book: always outside a
/// sandbox, and in the Flatpak where the document portal's mount is there,
/// through which the folder chooser exports a folder (D-730).
pub fn can_choose_book_folder() -> bool {
    !in_flatpak(Path::new("/.flatpak-info"))
        || runtime_dir().is_some_and(|dir| dir.join("doc").is_dir())
}

/// The user's home folder, if the system names one.
pub fn home_dir() -> Option<PathBuf> {
    absolute_env("HOME")
}

/// `$XDG_RUNTIME_DIR`, where the document portal is mounted in `doc`.
pub fn runtime_dir() -> Option<PathBuf> {
    absolute_env("XDG_RUNTIME_DIR")
}

fn absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// What the folder chooser returned (a normal path, or in the Flatpak one in
/// the document portal's mount) as a folder the app may use, or why it
/// isn't (D-730). The chooser itself is Qt's folder dialog, shown by QML's
/// `FolderChooser`; US5.13's bill folder uses this too.
pub fn choose_folder(chosen: &Path) -> Result<FolderGrant, FolderRefused> {
    let home = home_dir();
    let runtime = runtime_dir();
    // In the portal's mount the real path is asked from the portal; if it
    // doesn't say, the folder is shown by its name.
    let host = runtime
        .as_deref()
        .filter(|runtime| chosen.starts_with(runtime.join("doc")))
        .map(|_| {
            ffi::portal_host_path(&QString::from(chosen.to_string_lossy().as_ref())).to_string()
        })
        .filter(|text| !text.is_empty())
        .map(PathBuf::from);
    book_folder::grant(
        chosen,
        Surroundings {
            home: home.as_deref(),
            runtime_dir: runtime.as_deref(),
            host_path: host.as_deref(),
        },
    )
}

/// Whether the app runs in a Flatpak, which puts `marker` into every sandbox.
fn in_flatpak(marker: &Path) -> bool {
    cfg!(target_os = "linux") && marker.exists()
}

/// The bytes the user may still write in `folder`, if the system tells.
pub fn free_space(folder: &Path) -> Option<u64> {
    let bytes = ffi::free_space(&QString::from(folder.to_string_lossy().as_ref()));
    u64::try_from(bytes).ok()
}

/// `bytes` as a size in the user's language, e.g. "1.2 MB".
pub fn data_size(bytes: u64) -> String {
    ffi::data_size(i64::try_from(bytes).unwrap_or(i64::MAX)).to_string()
}

/// Renames `from` to `to` in the same folder, failing with `AlreadyExists`
/// instead of ever replacing a file at `to` (D-731).
pub fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    let text = |path: &Path| {
        path.to_str()
            .map(QString::from)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidFilename))
    };
    match ffi::rename_no_replace(&text(from)?, &text(to)?) {
        0 => Ok(()),
        error => Err(std::io::Error::from_raw_os_error(error)),
    }
}

/// What kind of drive a book is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drive {
    /// A drive of this computer, or one Qt can't tell.
    Local,
    /// A network drive (SMB, NFS, ...), where SQLite's writes and locks
    /// aren't reliable, so the book opens with a warning (D-038).
    Network,
    /// A single file a sandbox's file chooser handed over, e.g. in the
    /// Flatpak: the book's lock file and SQLite's journal can't be made next
    /// to it, so the book can't be opened from there. A file inside a folder
    /// the user exported is a normal book (D-730).
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
    let kind = drive_of(&file_system.to_string());
    match runtime_dir() {
        Some(runtime) => narrowed(kind, path, &runtime),
        None => kind,
    }
}

/// `kind`, a file system's drive, with the document portal's mount told
/// apart: the sandbox sees `fuse.portal` for everything the user exported, but
/// only a single file can't hold a book. A file in an exported folder is as
/// local as the system lets us tell (D-730; a network drive behind the portal
/// is D-732's check).
fn narrowed(kind: Drive, path: &Path, runtime_dir: &Path) -> Drive {
    // Anything the file system calls the portal stays a single file unless the
    // path plainly lies inside an exported folder.
    if kind == Drive::Sandboxed
        && book_folder::portal_place(path, runtime_dir) == book_folder::PortalPlace::Inside
    {
        Drive::Local
    } else {
        kind
    }
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
    fn only_a_single_portal_file_is_sandboxed() {
        let run = Path::new("/run/user/1000");
        let single = Path::new("/run/user/1000/doc/ab12/Household.ksz");
        let inside = Path::new("/run/user/1000/doc/ab12/Books/Household.ksz");
        assert_eq!(narrowed(Drive::Sandboxed, single, run), Drive::Sandboxed);
        assert_eq!(narrowed(Drive::Sandboxed, inside, run), Drive::Local);
        // The file system's other kinds are not changed.
        assert_eq!(narrowed(Drive::Network, inside, run), Drive::Network);
        assert_eq!(narrowed(Drive::Local, single, run), Drive::Local);
        // A path that only looks like one, with a `..`, stays sandboxed.
        let tricky = Path::new("/run/user/1000/doc/ab12/../x/y/Household.ksz");
        assert_eq!(narrowed(Drive::Sandboxed, tricky, run), Drive::Sandboxed);
    }

    #[test]
    fn a_chosen_folder_becomes_a_grant_or_is_refused() {
        let folder = TestFolder::new();
        let grant = choose_folder(folder.path()).expect("granted");
        assert_eq!(grant.path, folder.path());
        assert_eq!(grant.shown_as, folder.path().to_string_lossy());
        assert!(grant.shown_is_real_path);
        // The home folder and `/` reach as far as a home permission would.
        if let Some(home) = home_dir() {
            assert_eq!(choose_folder(&home), Err(FolderRefused::TooBroad));
        }
        assert_eq!(choose_folder(Path::new("/")), Err(FolderRefused::TooBroad));
        assert_eq!(
            choose_folder(Path::new("relative")),
            Err(FolderRefused::NotUsable)
        );
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
