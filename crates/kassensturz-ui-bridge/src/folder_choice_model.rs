// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The QML object behind choosing a folder the app may keep files in
//! (D-730): what Qt's folder dialog returns becomes a folder grant or a
//! refusal with a code. `FolderChooser.qml` shows the dialog; moving the
//! book (D-731), the backup folder (#806) and the bill folder (US5.13) all
//! ask for their folder through it. The folder's real path is shown as plain
//! text only, and never reaches a log or an AI or MCP result.

use cxx_qt_lib::{QUrl, QVariant};
use kassensturz_core::book_folder::{FolderGrant, FolderRefused};

use crate::book_session::local_path;
use crate::platform;
use crate::variant::{map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// Turns a chosen folder into one the app may use. `available` says
        /// whether this computer offers choosing a folder at all (D-730).
        #[qobject]
        #[qml_element]
        #[qproperty(bool, available, READ, CONSTANT)]
        type FolderChoice = super::FolderChoiceRust;
    }

    extern "RustQt" {
        /// The folder the dialog returned as an object: `ok`, and then `path`,
        /// `shownAs` and `shownIsRealPath`, or else `code` for the reason it
        /// isn't used.
        #[qinvokable]
        fn grant(self: &FolderChoice, folder: &QUrl) -> QVariant;
    }

    impl cxx_qt::Initialize for FolderChoice {}
}

/// The Rust side of [`qobject::FolderChoice`].
#[derive(Default)]
pub struct FolderChoiceRust {
    available: bool,
}

impl cxx_qt::Initialize for qobject::FolderChoice {
    fn initialize(self: core::pin::Pin<&mut Self>) {
        use cxx_qt::CxxQtType;
        let mut this = self;
        this.as_mut().rust_mut().available = platform::can_choose_book_folder();
    }
}

impl qobject::FolderChoice {
    fn grant(&self, folder: &QUrl) -> QVariant {
        match local_path(folder) {
            Some(path) => grant_object(&platform::choose_folder(&path)),
            None => grant_object(&Err(FolderRefused::NotUsable)),
        }
    }
}

/// The result as QML reads it.
fn grant_object(result: &Result<FolderGrant, FolderRefused>) -> QVariant {
    match result {
        Ok(grant) => map([
            ("ok", QVariant::from(&true)),
            ("path", text(&grant.path.to_string_lossy())),
            ("shownAs", text(&grant.shown_as)),
            ("shownIsRealPath", QVariant::from(&grant.shown_is_real_path)),
        ]),
        Err(refused) => map([
            ("ok", QVariant::from(&false)),
            ("code", text(refused.code())),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::variant::read_back::{flag_of, keys, text_of};

    #[test]
    fn a_granted_folder_reads_back_as_qml_reads_it() {
        let grant = FolderGrant {
            path: PathBuf::from("/data/Books"),
            shown_as: "Books".to_owned(),
            shown_is_real_path: false,
        };
        let object = grant_object(&Ok(grant));
        assert_eq!(keys(&object), ["ok", "path", "shownAs", "shownIsRealPath"]);
        assert!(flag_of(&object, "ok"));
        assert_eq!(text_of(&object, "path"), "/data/Books");
        assert_eq!(text_of(&object, "shownAs"), "Books");
        assert!(!flag_of(&object, "shownIsRealPath"));
    }

    #[test]
    fn a_refused_folder_reads_back_with_its_code() {
        let object = grant_object(&Err(FolderRefused::TooBroad));
        assert_eq!(keys(&object), ["code", "ok"]);
        assert!(!flag_of(&object, "ok"));
        assert_eq!(text_of(&object, "code"), "folder.too-broad");
    }
}
