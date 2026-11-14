// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fuzz target: the CSV importer and the preview of its columns read any
//! bytes, in the settings of a typical German bank file, without panicking,
//! hanging or eating memory (D-787).

#![no_main]

use kassensturz_core::CurrencyCode;
use kassensturz_import::{Csv, CsvSettings, Limits, csv_preview, currency, import};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let settings = CsvSettings::from_text(
        "delimiter = \";\"\ndecimal-mark = comma\ndate-order = day-month-year\n\
         booking-date = \"Datum\"\namount = \"Betrag\"\nother-party-name = \"Name\"\n",
    )
    .expect("valid settings");
    let euro = currency(CurrencyCode::new("EUR").expect("a valid code")).expect("a currency");
    let importer = Csv::new(settings, euro).expect("an importer");
    let limits = Limits {
        max_file_bytes: 1 << 20,
        max_entries: 10_000,
        ..Limits::default()
    };
    let _ = import(&importer, data, &limits);
    let _ = csv_preview(data, None, ';', 0, 20);
});
