// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The test profile (`crate::test_profile`, Q-399) as a QML object: whether
//! the app runs in it, for the banner on every screen and the wizard, and
//! where its test folder is.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QUrl};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The test profile: whether it is on, and its folder (empty when off).
        #[qobject]
        #[qml_element]
        #[qproperty(bool, on, READ, CONSTANT)]
        #[qproperty(QString, folder, READ, CONSTANT)]
        #[qproperty(QString, folder_url, cxx_name = "folderUrl", READ, CONSTANT)]
        type TestProfile = super::TestProfileRust;
    }

    impl cxx_qt::Initialize for TestProfile {}
}

/// The Rust side of [`qobject::TestProfile`].
#[derive(Default)]
pub struct TestProfileRust {
    on: bool,
    folder: QString,
    folder_url: QString,
}

impl cxx_qt::Initialize for qobject::TestProfile {
    fn initialize(mut self: Pin<&mut Self>) {
        let mut model = self.as_mut().rust_mut();
        if let Some(root) = crate::test_profile::root() {
            let folder = QString::from(root.to_string_lossy().as_ref());
            model.on = true;
            model.folder_url = QUrl::from_local_file(&folder).to_qstring();
            model.folder = folder;
        }
    }
}
