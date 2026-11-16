// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! B-335: reading a KMyMoney SQLite file stays within bounded memory: a
//! small file whose values are not stored in it is refused before any
//! large value is built. Synthetic data only.

use std::path::Path;
use std::time::Instant;

use kassensturz_import::{ImportError, KmySqlite, Limits, import};
use rusqlite::{Connection, MAIN_DB};

/// Bytes each row's computed `memo` would grow to, and how many rows.
const BYTES_PER_ROW: usize = 8 * 1024 * 1024;
const ROWS: usize = 128;

fn hostile_file(how: &str, bytes_per_row: usize) -> Vec<u8> {
    let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/kmy-sqlite/kassensturz-sample.sqlite");
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .deserialize_read_exact(
            MAIN_DB,
            &std::fs::read(golden).unwrap()[..],
            std::fs::metadata(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/golden/kmy-sqlite/kassensturz-sample.sqlite"),
            )
            .unwrap()
            .len() as usize,
            false,
        )
        .unwrap();
    let columns: Vec<(String, String)> = {
        let mut statement = connection
            .prepare("SELECT name, type FROM pragma_table_info('kmmSplits')")
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let definition: Vec<String> = columns
        .iter()
        .map(|(name, kind)| {
            if name == "memo" {
                format!(
                    "memo TEXT GENERATED ALWAYS AS (printf('%.*c', {bytes_per_row}, 'x')) {how}"
                )
            } else {
                format!("\"{name}\" {kind}")
            }
        })
        .collect();
    let copied: Vec<String> = columns
        .iter()
        .filter(|(name, _)| name != "memo")
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    let copied = copied.join(", ");
    connection
        .execute_batch(&format!(
            "ALTER TABLE kmmSplits RENAME TO old_splits;
             CREATE TABLE kmmSplits ({});",
            definition.join(", ")
        ))
        .unwrap();
    // Many copies of the first split, each with its own split number.
    for copy in 0..ROWS {
        connection
            .execute(
                &format!(
                    "INSERT INTO kmmSplits ({copied}) SELECT {} FROM old_splits LIMIT 1",
                    copied.replace("\"splitId\"", &format!("{copy} + 1000"))
                ),
                [],
            )
            .unwrap();
    }
    connection
        .execute_batch("DROP TABLE old_splits; VACUUM;")
        .unwrap();
    connection.serialize(MAIN_DB).unwrap().to_vec()
}

/// The process's peak memory in KiB, where the system reports it (Linux);
/// 0 elsewhere, so only the time is checked there.
fn peak_rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.trim().trim_end_matches(" kB").trim().parse().ok())
        .unwrap_or(0)
}

#[test]
fn a_tiny_hostile_sqlite_file_is_refused_cheaply() {
    refused_cheaply(&hostile_file("VIRTUAL", BYTES_PER_ROW));
    // A stored one is in the file, so it is kept small here; it is refused
    // all the same, as KMyMoney never writes one.
    refused_cheaply(&hostile_file("STORED", 1024));
}

fn refused_cheaply(file: &[u8]) {
    let before = peak_rss_kib();
    let started = Instant::now();
    let result = import(&KmySqlite, file, &Limits::default());
    let elapsed = started.elapsed();
    let after = peak_rss_kib();
    assert!(
        matches!(result, Err(ImportError::Malformed { .. })),
        "{:?}",
        result.err()
    );
    // A file of a few kilobytes should never cost more than a second or
    // hundreds of megabytes before it is refused.
    assert!(
        elapsed.as_secs() < 1 && after.saturating_sub(before) < 256 * 1024,
        "a {}-byte file took {elapsed:?} and {} MiB more memory",
        file.len(),
        after.saturating_sub(before) / 1024
    );
}
