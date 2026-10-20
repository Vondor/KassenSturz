// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The importers against their golden files, and the interface's checks of
//! files and of what importers return.
//!
//! Each importer gets one golden test here, which runs every file in its
//! folder under `tests/golden/` (`harness.rs`).

mod files;
mod harness;

use kassensturz_import::{Camt053, TestFormat};

#[test]
fn golden_test_format() {
    harness::check_folder(&TestFormat);
}

#[test]
fn golden_camt053() {
    harness::check_folder(&Camt053);
}
