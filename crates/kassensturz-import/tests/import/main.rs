// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The importers against their golden files, and the interface's checks of
//! files and of what importers return.
//!
//! Each importer gets one golden test here, which runs every file in its
//! folder under `tests/golden/` (`harness.rs`).

mod files;
mod harness;

use std::fs;

use kassensturz_core::CurrencyCode;
use kassensturz_import::{Camt053, Csv, CsvSettings, Mt940, Ofx, TestFormat, currency};

#[test]
fn golden_test_format() {
    harness::check_folder(&TestFormat);
}

#[test]
fn golden_camt053() {
    harness::check_folder(&Camt053);
}

#[test]
fn golden_mt940() {
    harness::check_folder(&Mt940);
}

#[test]
fn golden_ofx() {
    harness::check_folder(&Ofx);
}

/// Each CSV file is read with the settings in its `.settings` file, for an
/// account in euros.
#[test]
fn golden_csv() {
    harness::check_folder_with("csv", &|path| {
        let settings = fs::read_to_string(harness::companion(path, "settings"))
            .expect("every CSV golden file has a .settings file");
        let euro = currency(CurrencyCode::new("EUR").expect("a valid code"))?;
        Ok(Box::new(Csv::new(
            CsvSettings::from_text(&settings)?,
            euro,
        )?))
    });
}
