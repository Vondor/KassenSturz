// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The folders KassenSturz may open, and taking one back (#873, D-730,
//! `docs/BOOK_FOLDER.md` §3). In the Flatpak the user gives access to a folder
//! through the folder chooser; the document portal keeps that grant until it
//! is taken back. This lists the folders and forgets one.
//!
//! Forgetting only ends the app's access. It never deletes, moves or changes a
//! file, and a folder the open book or the backup copies still use is not
//! forgotten: the page says what uses it first. The real paths are display
//! text only, shown as plain text, and kept out of logs and AI and MCP
//! results (D-730).

use std::path::Path;

use kassensturz_core::book_folder::portal_document;

/// What the document portal says about one document the app may open: its ID
/// and where it is on the computer (display text only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// The document's ID, as in `$XDG_RUNTIME_DIR/doc/<id>/`.
    pub id: String,
    /// The path on the computer, as the portal says it.
    pub host_path: String,
}

/// What still uses a folder, so it can't be forgotten yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsedBy {
    /// The open book is kept in it.
    Book,
    /// The backup copies are written to it.
    BackupCopies,
}

impl UsedBy {
    /// The word QML writes a sentence for.
    pub fn key(self) -> &'static str {
        match self {
            Self::Book => "book",
            Self::BackupCopies => "backup-copies",
        }
    }
}

/// The places in use, as the app opens them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Uses {
    /// The open book's file, if one is open.
    pub book: Option<std::path::PathBuf>,
    /// The folder chosen for backup copies, if one is.
    pub backup_copies: Option<std::path::PathBuf>,
}

/// A folder the app may open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Granted {
    /// The document's ID, to forget it by.
    pub id: String,
    /// The folder as the user knows it: its real path where the portal says
    /// it, else its name. Display text only.
    pub shown_as: String,
    /// Whether `shown_as` is the real path.
    pub shown_is_real_path: bool,
    /// What still uses it; empty if nothing does.
    pub used_by: Vec<UsedBy>,
}

/// Why a folder wasn't forgotten: a code the page writes a message for
/// (D-034). Nothing was changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The open book is kept in the folder.
    InUseByBook,
    /// The backup copies are written to the folder.
    InUseByBackupCopies,
    /// The folder isn't in the list (anymore).
    NotThere,
    /// The portal didn't take the access back.
    Failed,
}

impl Problem {
    /// The code, e.g. `granted-folders.in-use-by-book`.
    pub fn code(self) -> &'static str {
        match self {
            Self::InUseByBook => "granted-folders.in-use-by-book",
            Self::InUseByBackupCopies => "granted-folders.in-use-by-backup-copies",
            Self::NotThere => "granted-folders.not-there",
            Self::Failed => "granted-folders.failed",
        }
    }
}

/// What the document portal is asked, by the app's own platform code or by a
/// test.
pub trait Portal {
    /// What the app may open.
    fn documents(&self) -> Vec<Document>;
    /// Takes the app's access to the document `id` back. Returns whether the
    /// portal accepted it.
    fn forget(&mut self, id: &str) -> bool;
}

/// The real portal, over the platform's D-Bus.
pub struct SystemPortal;

impl Portal for SystemPortal {
    fn documents(&self) -> Vec<Document> {
        crate::platform::portal_documents()
            .into_iter()
            .map(|(id, host_path)| Document { id, host_path })
            .collect()
    }

    fn forget(&mut self, id: &str) -> bool {
        crate::platform::portal_forget(id)
    }
}

/// The name of the first folder in the document `id`'s mount, which is how the
/// portal shows an exported folder (`<runtime>/doc/<id>/<name>/`). `None` for
/// a document that holds a single file or nothing the app can see: only
/// folders are listed here.
fn folder_name(runtime_dir: &Path, id: &str) -> Option<String> {
    // The ID names a folder of the portal's mount: nothing that climbs out
    // of it.
    if id.is_empty() || id.contains(['/', '\\']) || id == "." || id == ".." {
        return None;
    }
    std::fs::read_dir(runtime_dir.join("doc").join(id))
        .ok()?
        .flatten()
        .find(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .and_then(|entry| entry.file_name().into_string().ok())
}

/// The folders among `documents`, sorted by how they are shown, each with what
/// still uses it.
pub fn list(documents: &[Document], runtime_dir: &Path, uses: &Uses) -> Vec<Granted> {
    let document_of = |path: &Option<std::path::PathBuf>| {
        path.as_deref()
            .and_then(|path| portal_document(path, runtime_dir))
            .map(|(id, _)| id)
    };
    let book = document_of(&uses.book);
    let backup_copies = document_of(&uses.backup_copies);
    let mut found: Vec<Granted> = documents
        .iter()
        .filter_map(|document| {
            let name = folder_name(runtime_dir, &document.id)?;
            let real = Path::new(&document.host_path).is_absolute();
            let mut used_by = Vec::new();
            if book.as_deref() == Some(document.id.as_str()) {
                used_by.push(UsedBy::Book);
            }
            if backup_copies.as_deref() == Some(document.id.as_str()) {
                used_by.push(UsedBy::BackupCopies);
            }
            Some(Granted {
                id: document.id.clone(),
                shown_as: if real {
                    document.host_path.clone()
                } else {
                    name
                },
                shown_is_real_path: real,
                used_by,
            })
        })
        .collect();
    found.sort_by(|a, b| {
        a.shown_as
            .to_lowercase()
            .cmp(&b.shown_as.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    found
}

/// Forgets the folder of the document `id`: the app's access ends, the files
/// stay. Refused, with nothing changed, for a folder that is not in the list
/// and for one the open book or the backup copies still use. After asking the
/// portal it asks again, so "forgotten" is said only if the folder is gone
/// from the list.
pub fn forget(
    id: &str,
    runtime_dir: &Path,
    uses: &Uses,
    portal: &mut impl Portal,
) -> Result<(), Problem> {
    let granted = list(&portal.documents(), runtime_dir, uses);
    let Some(folder) = granted.iter().find(|folder| folder.id == id) else {
        return Err(Problem::NotThere);
    };
    if folder.used_by.contains(&UsedBy::Book) {
        return Err(Problem::InUseByBook);
    }
    if folder.used_by.contains(&UsedBy::BackupCopies) {
        return Err(Problem::InUseByBackupCopies);
    }
    if !portal.forget(id) {
        return Err(Problem::Failed);
    }
    if list(&portal.documents(), runtime_dir, uses)
        .iter()
        .any(|folder| folder.id == id)
    {
        return Err(Problem::Failed);
    }
    Ok(())
}

/// The places in use now, from the open book and the settings.
pub fn uses_now(settings: &crate::settings::Settings) -> Uses {
    Uses {
        book: crate::session::path(),
        backup_copies: crate::backup_copies::chosen(settings).map(|chosen| chosen.path),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::test_folder::TestFolder;

    /// A portal's mount in a temporary folder: the folders `<id>/<name>`, and
    /// documents `file-id` that holds one file.
    struct Mount {
        folder: TestFolder,
    }

    impl Mount {
        fn new() -> Self {
            let mount = Self {
                folder: TestFolder::new(),
            };
            for (id, name) in [("b2", "Sync"), ("a1", "Books"), ("c3", "Copies")] {
                let folder = mount.folder.join("doc").join(id).join(name);
                fs::create_dir_all(&folder).expect("folder made");
                fs::write(folder.join("note.txt"), "keep me").expect("file written");
            }
            let single = mount.folder.join("doc").join("f9");
            fs::create_dir_all(&single).expect("folder made");
            fs::write(single.join("statement.csv"), "a,b").expect("file written");
            mount
        }

        fn runtime(&self) -> &Path {
            self.folder.path()
        }

        fn path(&self, id: &str, name: &str) -> PathBuf {
            self.folder.join("doc").join(id).join(name)
        }
    }

    /// A portal that keeps its documents in memory.
    struct Fake {
        documents: Vec<Document>,
        accepts: bool,
        forgets: bool,
        asked: Vec<String>,
    }

    impl Fake {
        fn new() -> Self {
            let document = |id: &str, host: &str| Document {
                id: id.into(),
                host_path: host.into(),
            };
            Self {
                documents: vec![
                    document("b2", "/home/example/Sync"),
                    document("a1", "/home/example/Books"),
                    document("c3", "/home/example/Backup copies"),
                    document("f9", "/home/example/statement.csv"),
                    // The portal knows it, the mount doesn't show it.
                    document("gone", "/home/example/Gone"),
                ],
                accepts: true,
                forgets: true,
                asked: Vec::new(),
            }
        }
    }

    impl Portal for Fake {
        fn documents(&self) -> Vec<Document> {
            self.documents.clone()
        }

        fn forget(&mut self, id: &str) -> bool {
            self.asked.push(id.to_owned());
            if self.forgets {
                self.documents.retain(|document| document.id != id);
            }
            self.accepts
        }
    }

    fn nothing_in_use() -> Uses {
        Uses::default()
    }

    #[test]
    fn only_folders_are_listed_sorted_by_how_they_are_shown() {
        let mount = Mount::new();
        let fake = Fake::new();
        let found = list(&fake.documents, mount.runtime(), &nothing_in_use());
        let shown: Vec<_> = found
            .iter()
            .map(|folder| (folder.id.as_str(), folder.shown_as.as_str()))
            .collect();
        // A document that holds one file, and one the mount doesn't have
        // as a folder, are left out. `f9` holds a file, so it has no folder.
        assert_eq!(
            shown,
            [
                ("c3", "/home/example/Backup copies"),
                ("a1", "/home/example/Books"),
                ("b2", "/home/example/Sync"),
            ]
        );
        assert!(found.iter().all(|folder| folder.shown_is_real_path));
        assert!(found.iter().all(|folder| folder.used_by.is_empty()));
    }

    #[test]
    fn a_folder_without_a_real_path_is_shown_by_its_name() {
        let mount = Mount::new();
        let documents = vec![Document {
            id: "a1".into(),
            host_path: String::new(),
        }];
        let found = list(&documents, mount.runtime(), &nothing_in_use());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].shown_as, "Books");
        assert!(!found[0].shown_is_real_path);
    }

    #[test]
    fn an_id_that_climbs_out_of_the_mount_is_not_a_folder() {
        let mount = Mount::new();
        let documents = ["..", ".", "a1/..", "", "../doc/a1"]
            .map(|id| Document {
                id: id.into(),
                host_path: "/home/example/x".into(),
            })
            .to_vec();
        assert!(list(&documents, mount.runtime(), &nothing_in_use()).is_empty());
    }

    #[test]
    fn the_folders_in_use_say_what_uses_them() {
        let mount = Mount::new();
        let fake = Fake::new();
        let uses = Uses {
            book: Some(mount.path("a1", "Books").join("Household.ksz")),
            backup_copies: Some(mount.path("c3", "Copies")),
        };
        let found = list(&fake.documents, mount.runtime(), &uses);
        let used = |id: &str| {
            found
                .iter()
                .find(|folder| folder.id == id)
                .map(|folder| folder.used_by.clone())
                .expect("listed")
        };
        assert_eq!(used("a1"), [UsedBy::Book]);
        assert_eq!(used("c3"), [UsedBy::BackupCopies]);
        assert!(used("b2").is_empty());
        // One folder can be both.
        let both = Uses {
            book: Some(mount.path("a1", "Books").join("Household.ksz")),
            backup_copies: Some(mount.path("a1", "Books")),
        };
        let found = list(&fake.documents, mount.runtime(), &both);
        let a1 = found
            .iter()
            .find(|folder| folder.id == "a1")
            .expect("listed");
        assert_eq!(a1.used_by, [UsedBy::Book, UsedBy::BackupCopies]);
    }

    #[test]
    fn forgetting_ends_the_access_and_leaves_every_file_alone() {
        let mount = Mount::new();
        let mut fake = Fake::new();
        forget("b2", mount.runtime(), &nothing_in_use(), &mut fake).expect("forgotten");
        assert_eq!(fake.asked, ["b2"]);
        let found = list(&fake.documents, mount.runtime(), &nothing_in_use());
        assert!(found.iter().all(|folder| folder.id != "b2"));
        assert_eq!(found.len(), 2);
        // Nothing in the folder was touched.
        assert_eq!(
            fs::read_to_string(mount.path("b2", "Sync").join("note.txt")).expect("kept"),
            "keep me"
        );
    }

    #[test]
    fn a_folder_in_use_is_refused_before_the_portal_is_asked() {
        let mount = Mount::new();
        let mut fake = Fake::new();
        let uses = Uses {
            book: Some(mount.path("a1", "Books").join("Household.ksz")),
            backup_copies: Some(mount.path("c3", "Copies")),
        };
        assert_eq!(
            forget("a1", mount.runtime(), &uses, &mut fake),
            Err(Problem::InUseByBook)
        );
        assert_eq!(
            forget("c3", mount.runtime(), &uses, &mut fake),
            Err(Problem::InUseByBackupCopies)
        );
        assert!(fake.asked.is_empty());
        assert_eq!(fake.documents.len(), 5);
    }

    #[test]
    fn a_folder_that_is_not_listed_is_refused() {
        let mount = Mount::new();
        let mut fake = Fake::new();
        for id in ["nope", "gone", "f9", ".."] {
            assert_eq!(
                forget(id, mount.runtime(), &nothing_in_use(), &mut fake),
                Err(Problem::NotThere),
                "{id}"
            );
        }
        assert!(fake.asked.is_empty());
    }

    #[test]
    fn forgetting_is_said_only_if_the_folder_is_gone_from_the_list() {
        let mount = Mount::new();
        // The portal says no.
        let mut refusing = Fake::new();
        refusing.accepts = false;
        refusing.forgets = false;
        assert_eq!(
            forget("b2", mount.runtime(), &nothing_in_use(), &mut refusing),
            Err(Problem::Failed)
        );
        // The portal says yes, and the folder is still there.
        let mut lying = Fake::new();
        lying.forgets = false;
        assert_eq!(
            forget("b2", mount.runtime(), &nothing_in_use(), &mut lying),
            Err(Problem::Failed)
        );
        assert_eq!(
            fs::read_to_string(mount.path("b2", "Sync").join("note.txt")).expect("kept"),
            "keep me"
        );
    }

    #[test]
    fn every_problem_has_its_own_code() {
        let codes = [
            Problem::InUseByBook,
            Problem::InUseByBackupCopies,
            Problem::NotThere,
            Problem::Failed,
        ]
        .map(Problem::code);
        let mut unique = codes.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), codes.len());
        assert!(
            codes
                .iter()
                .all(|code| code.starts_with("granted-folders."))
        );
    }
}
