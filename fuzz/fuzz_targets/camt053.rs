// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fuzz target: the camt053 importer reads any bytes without panicking,
//! hanging or eating memory; it returns an error or a result that passed
//! the importer's own checks (D-787). Small limits keep each run quick.

#![no_main]

use kassensturz_import::{Camt053, Limits, import};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = Limits {
        max_file_bytes: 1 << 20,
        max_entries: 10_000,
        ..Limits::default()
    };
    let importer = Camt053;
    // `recognize` must not fail on any start of a file either.
    let _ = kassensturz_import::Importer::recognize(&importer, &data[..data.len().min(4096)]);
    let _ = import(&importer, data, &limits);
});
