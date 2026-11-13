// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Folders the user chose for their book, also in a sandbox (D-730,
//! `docs/BOOK_FOLDER.md`). Pure path logic, so it is tested without Qt and
//! the app and `kassensturz-mcp` read a remembered path the same way:
//!
//! - where a path lies in the document portal's mount (`$XDG_RUNTIME_DIR/doc`),
//!   through which a Flatpak app reaches what the user chose: a single file
//!   (`<id>/<file>`) can't hold a book, a file inside an exported folder
//!   (`<id>/<folder>/<file>`) can;
//! - what a chosen folder becomes ([`FolderGrant`]) or why it is refused;
//! - how a path is written into the settings file and read back, with
//!   `$XDG_RUNTIME_DIR` and `$HOME` kept as such ([`remember_entry`],
//!   [`expand_entry`]).

use std::path::{Component, Path, PathBuf};

/// Where a path lies in the document portal's mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalPlace {
    /// Not in the mount (or named with `..`, which is never trusted to be).
    Outside,
    /// The mount itself or an `<id>` folder: nothing a user chose.
    Root,
    /// `<id>/<name>`: a single file the portal handed over, or the root of an
    /// exported folder, which is the same path until one looks at what it is.
    Export,
    /// Inside an exported folder: `<id>/<folder>/<more>`.
    Inside,
}

/// Where `path` lies in the document portal's mount below `runtime_dir`.
#[must_use]
pub fn portal_place(path: &Path, runtime_dir: &Path) -> PortalPlace {
    let Ok(rest) = path.strip_prefix(runtime_dir.join("doc")) else {
        return PortalPlace::Outside;
    };
    let mut depth = 0_usize;
    for component in rest.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            _ => return PortalPlace::Outside,
        }
    }
    match depth {
        0 | 1 => PortalPlace::Root,
        2 => PortalPlace::Export,
        _ => PortalPlace::Inside,
    }
}

/// Whether a book at `path` is a single file the portal handed over, which
/// can't hold a book: its lock file and SQLite's journal can't be made next
/// to it. A file inside an exported folder is a normal book.
#[must_use]
pub fn is_single_portal_file(path: &Path, runtime_dir: &Path) -> bool {
    matches!(
        portal_place(path, runtime_dir),
        PortalPlace::Root | PortalPlace::Export
    )
}

/// A folder the user chose, as the app may use it (D-730).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderGrant {
    /// Where the app opens files: a normal path, or one in the document
    /// portal's mount.
    pub path: PathBuf,
    /// The folder as the user knows it. The real path on their computer when
    /// it is known, else the folder's name. **Display text only**, never
    /// opened; it usually holds the user's name, so it is shown as plain text
    /// and kept out of logs and AI and MCP results.
    pub shown_as: String,
    /// Whether `shown_as` is the real path, not only the folder's name.
    pub shown_is_real_path: bool,
}

/// Why a chosen folder isn't used. Each has a code the UI turns into words
/// (D-034).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderRefused {
    /// The home folder itself, `/`, or a folder that holds the home folder:
    /// the grant would reach as far as a permission for the whole home folder
    /// (option B in D-730).
    TooBroad,
    /// Not a folder the app can use: not absolute, named with `..`, or the
    /// portal's own folders.
    NotUsable,
}

impl FolderRefused {
    /// The code the UI shows words for.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::TooBroad => "folder.too-broad",
            Self::NotUsable => "folder.not-usable",
        }
    }
}

/// What the app and the computer know when a folder is chosen.
#[derive(Debug, Clone, Copy, Default)]
pub struct Surroundings<'a> {
    /// The user's home folder.
    pub home: Option<&'a Path>,
    /// `$XDG_RUNTIME_DIR`, where the document portal is mounted.
    pub runtime_dir: Option<&'a Path>,
    /// The folder's real path on the computer, from the document portal's
    /// `GetHostPaths`, where it is known.
    pub host_path: Option<&'a Path>,
}

/// Turns the folder the user chose into a [`FolderGrant`], or says why not.
///
/// # Errors
///
/// [`FolderRefused::TooBroad`] for the home folder, `/` or a folder that
/// holds the home folder; [`FolderRefused::NotUsable`] for a path that isn't
/// absolute or has `..`, and for the portal's own folders.
pub fn grant(chosen: &Path, around: Surroundings<'_>) -> Result<FolderGrant, FolderRefused> {
    if !chosen.is_absolute() || has_parent_part(chosen) {
        return Err(FolderRefused::NotUsable);
    }
    let in_portal = match around.runtime_dir.map(|dir| portal_place(chosen, dir)) {
        Some(PortalPlace::Root) => return Err(FolderRefused::NotUsable),
        Some(PortalPlace::Export | PortalPlace::Inside) => true,
        Some(PortalPlace::Outside) | None => false,
    };
    // Where the folder really is: its own path, or in the portal what the
    // portal says. If it doesn't say, the check can't be made.
    let real = if in_portal {
        around.host_path.filter(|path| path.is_absolute())
    } else {
        Some(chosen)
    };
    if let Some(real) = real
        && has_parent_part(real)
    {
        return Err(FolderRefused::NotUsable);
    }
    if let Some(real) = real
        && too_broad(real, around.home)
    {
        return Err(FolderRefused::TooBroad);
    }
    Ok(match real {
        Some(real) => FolderGrant {
            path: chosen.to_owned(),
            shown_as: real.to_string_lossy().into_owned(),
            shown_is_real_path: true,
        },
        None => FolderGrant {
            path: chosen.to_owned(),
            shown_as: chosen
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            shown_is_real_path: false,
        },
    })
}

fn has_parent_part(path: &Path) -> bool {
    path.components()
        .any(|component| component == Component::ParentDir)
}

/// Whether a grant for `folder` would reach as far as the whole home folder.
fn too_broad(folder: &Path, home: Option<&Path>) -> bool {
    // `starts_with` compares whole components and ignores a trailing `/`.
    folder.parent().is_none() || home.is_some_and(|home| home.starts_with(folder))
}

/// The settings file's text for a path in a settings entry that is written
/// with KConfig's expansion flag (`Key[$e]=`): `$XDG_RUNTIME_DIR` and `$HOME`
/// in place of those folders, every other `$` doubled. The app and the MCP
/// server read it back with [`expand_entry`].
#[must_use]
pub fn remember_entry(path: &str, home: Option<&str>, runtime_dir: Option<&str>) -> String {
    let (start, rest) = [("$XDG_RUNTIME_DIR", runtime_dir), ("$HOME", home)]
        .into_iter()
        .find_map(|(name, folder)| {
            let folder = folder.filter(|folder| folder.starts_with('/'))?;
            let rest = Path::new(path).strip_prefix(folder).ok()?;
            let rest = rest.to_str()?;
            // The path itself, or what follows the folder and a `/`.
            Some((name, rest))
        })
        .unwrap_or(("", path));
    let mut text = String::from(start);
    if !start.is_empty() && !rest.is_empty() {
        text.push('/');
    }
    for c in rest.chars() {
        if c == '$' {
            text.push('$');
        }
        text.push(c);
    }
    text
}

/// A path from [`remember_entry`], or as KConfig's `writePathEntry` writes
/// it: `$XDG_RUNTIME_DIR` or `$HOME` at the start become `runtime_dir` and
/// `home`, `$$` is a `$`. Anything else with a `$` (another variable, a
/// command) is refused rather than guessed at.
///
/// `None` when the value isn't written that way, or names a folder that
/// isn't known here.
#[must_use]
pub fn expand_entry(value: &str, home: Option<&str>, runtime_dir: Option<&str>) -> Option<String> {
    let mut start = "";
    let mut rest = value;
    for (name, folder) in [("$XDG_RUNTIME_DIR", runtime_dir), ("$HOME", home)] {
        if let Some(after) = value.strip_prefix(name)
            && (after.is_empty() || after.starts_with('/'))
        {
            start = folder.filter(|folder| !folder.is_empty())?;
            rest = after;
            break;
        }
    }
    let mut path = String::from(start);
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        if c == '$' {
            // Only a doubled `$` is a `$`; anything else asks KConfig for a
            // variable or a command, which this never does.
            if chars.next() != Some('$') {
                return None;
            }
        }
        path.push(c);
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = "/run/user/1000";

    fn place(path: &str) -> PortalPlace {
        portal_place(Path::new(path), Path::new(RUN))
    }

    #[cfg(unix)]
    fn around<'a>(home: &'a Path, runtime: &'a Path, host: Option<&'a Path>) -> Surroundings<'a> {
        Surroundings {
            home: Some(home),
            runtime_dir: Some(runtime),
            host_path: host,
        }
    }

    #[test]
    fn tells_a_single_file_from_a_file_in_an_exported_folder() {
        assert_eq!(place("/home/ada/Books/Household.ksz"), PortalPlace::Outside);
        assert_eq!(place("/run/user/1000/doc"), PortalPlace::Root);
        assert_eq!(place("/run/user/1000/doc/ab12"), PortalPlace::Root);
        assert_eq!(
            place("/run/user/1000/doc/ab12/Household.ksz"),
            PortalPlace::Export
        );
        assert_eq!(place("/run/user/1000/doc/ab12/Books"), PortalPlace::Export);
        assert_eq!(
            place("/run/user/1000/doc/ab12/Books/Household.ksz"),
            PortalPlace::Inside
        );
        assert_eq!(
            place("/run/user/1000/doc/ab12/Books/sub/Household.ksz"),
            PortalPlace::Inside
        );
        assert_eq!(place("/run/user/1000/docs/ab12/x/y"), PortalPlace::Outside);
        assert_eq!(
            place("/run/user/1000/doc/ab12/../x/y/z"),
            PortalPlace::Outside
        );
        let run = Path::new(RUN);
        assert!(is_single_portal_file(
            Path::new("/run/user/1000/doc/ab12/Household.ksz"),
            run
        ));
        assert!(!is_single_portal_file(
            Path::new("/run/user/1000/doc/ab12/Books/Household.ksz"),
            run
        ));
        assert!(!is_single_portal_file(
            Path::new("/home/ada/Household.ksz"),
            run
        ));
    }

    // Unix paths: on Windows "/home/ada" isn't absolute, and the portal is Linux only.
    #[cfg(unix)]
    #[test]
    fn a_normal_folder_is_granted_as_it_is() {
        let grant = grant(
            Path::new("/home/ada/Books"),
            around(Path::new("/home/ada"), Path::new(RUN), None),
        )
        .expect("granted");
        assert_eq!(grant.path, Path::new("/home/ada/Books"));
        assert_eq!(grant.shown_as, "/home/ada/Books");
        assert!(grant.shown_is_real_path);
        // A second drive, outside the home folder, is fine too.
        assert!(
            super::grant(
                Path::new("/mnt/disk2/Books"),
                around(Path::new("/home/ada"), Path::new(RUN), None)
            )
            .is_ok()
        );
    }

    // Unix paths: on Windows "/home/ada" isn't absolute, and the portal is Linux only.
    #[cfg(unix)]
    #[test]
    fn a_folder_as_wide_as_the_home_folder_is_refused() {
        let home = Path::new("/home/ada");
        for chosen in ["/home/ada", "/home/ada/", "/home", "/"] {
            assert_eq!(
                grant(Path::new(chosen), around(home, Path::new(RUN), None)),
                Err(FolderRefused::TooBroad),
                "{chosen}"
            );
        }
        // Another user's folder is not above this home folder.
        assert!(grant(Path::new("/home/bob"), around(home, Path::new(RUN), None)).is_ok());
        // A folder only named like the home folder isn't it.
        assert!(grant(Path::new("/home/ada2"), around(home, Path::new(RUN), None)).is_ok());
    }

    // Unix paths: on Windows "/home/ada" isn't absolute, and the portal is Linux only.
    #[cfg(unix)]
    #[test]
    fn a_portal_folder_is_judged_by_its_real_path() {
        let home = Path::new("/home/ada");
        let run = Path::new(RUN);
        let chosen = Path::new("/run/user/1000/doc/ab12/ada");
        assert_eq!(
            grant(chosen, around(home, run, Some(Path::new("/home/ada")))),
            Err(FolderRefused::TooBroad)
        );
        let granted = grant(
            chosen,
            around(home, run, Some(Path::new("/home/ada/Sync/Books"))),
        )
        .expect("granted");
        assert_eq!(granted.path, chosen);
        assert_eq!(granted.shown_as, "/home/ada/Sync/Books");
        assert!(granted.shown_is_real_path);
    }

    // Unix paths: on Windows "/home/ada" isn't absolute, and the portal is Linux only.
    #[cfg(unix)]
    #[test]
    fn a_portal_folder_without_a_real_path_is_shown_by_its_name() {
        let granted = grant(
            Path::new("/run/user/1000/doc/ab12/Books"),
            around(Path::new("/home/ada"), Path::new(RUN), None),
        )
        .expect("granted");
        assert_eq!(granted.shown_as, "Books");
        assert!(!granted.shown_is_real_path);
    }

    // Unix paths: on Windows "/home/ada" isn't absolute, and the portal is Linux only.
    #[cfg(unix)]
    #[test]
    fn paths_that_are_no_folder_for_the_book_are_refused() {
        let home = Path::new("/home/ada");
        let run = Path::new(RUN);
        for chosen in [
            "Books",
            "/home/ada/../bob/Books",
            "/run/user/1000/doc",
            "/run/user/1000/doc/ab12",
        ] {
            assert_eq!(
                grant(Path::new(chosen), around(home, run, None)),
                Err(FolderRefused::NotUsable),
                "{chosen}"
            );
        }
        assert_eq!(
            grant(
                Path::new("/run/user/1000/doc/ab12/Books"),
                around(home, run, Some(Path::new("/home/ada/../../x")))
            ),
            Err(FolderRefused::NotUsable)
        );
        assert_eq!(FolderRefused::TooBroad.code(), "folder.too-broad");
        assert_eq!(FolderRefused::NotUsable.code(), "folder.not-usable");
    }

    #[test]
    fn a_remembered_path_keeps_the_runtime_and_home_folders_as_variables() {
        let remember = |path| remember_entry(path, Some("/home/ada"), Some(RUN));
        assert_eq!(
            remember("/run/user/1000/doc/ab12/Books/Household.ksz"),
            "$XDG_RUNTIME_DIR/doc/ab12/Books/Household.ksz"
        );
        assert_eq!(remember("/home/ada/Books/a.ksz"), "$HOME/Books/a.ksz");
        assert_eq!(remember("/home/ada"), "$HOME");
        assert_eq!(remember("/home/adam/a.ksz"), "/home/adam/a.ksz");
        assert_eq!(remember("/data/$5 a.ksz"), "/data/$$5 a.ksz");
        assert_eq!(
            remember_entry("/home/ada/a.ksz", None, None),
            "/home/ada/a.ksz"
        );
    }

    #[test]
    fn a_remembered_path_reads_back_as_it_was_written() {
        let home = Some("/home/ada");
        let run = Some(RUN);
        for path in [
            "/run/user/1000/doc/ab12/Books/Household.ksz",
            "/home/ada/Books/Household.ksz",
            "/data/$HOME/$$ \\ a.ksz",
            "/run/user/1000",
            "/home/ada",
        ] {
            let text = remember_entry(path, home, run);
            assert_eq!(
                expand_entry(&text, home, run).as_deref(),
                Some(path),
                "{text}"
            );
        }
    }

    #[test]
    fn only_the_two_folder_variables_and_doubled_dollars_are_read() {
        let read = |text| expand_entry(text, Some("/home/ada"), Some(RUN));
        assert_eq!(read("$HOME/a.ksz").as_deref(), Some("/home/ada/a.ksz"));
        assert_eq!(
            read("$XDG_RUNTIME_DIR/doc/x/a.ksz").as_deref(),
            Some("/run/user/1000/doc/x/a.ksz")
        );
        assert_eq!(read("/data/$$HOME.ksz").as_deref(), Some("/data/$HOME.ksz"));
        for refused in [
            "$HOMEWORK/a.ksz",
            "$XDG_RUNTIME_DIRX/a.ksz",
            "$XDG_DATA_HOME/a.ksz",
            "$(cmd)/a.ksz",
            "/data/$X/a.ksz",
            "/data/trailing$",
        ] {
            assert_eq!(read(refused), None, "{refused}");
        }
        // A variable this computer doesn't know is no path.
        assert_eq!(expand_entry("$XDG_RUNTIME_DIR/a", None, None), None);
        assert_eq!(expand_entry("$HOME/a", Some(""), None), None);
    }
}
