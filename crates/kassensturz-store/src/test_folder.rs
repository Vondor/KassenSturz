// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A folder of its own for each test that needs files.

use std::path::{Path, PathBuf};

/// A new, empty folder, removed again with everything in it when dropped.
pub(crate) struct TempFolder(PathBuf);

impl TempFolder {
    pub(crate) fn new() -> Self {
        let name = format!("kassensturz-test-{}", uuid::Uuid::now_v7());
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir(&path).expect("temporary folder created");
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
