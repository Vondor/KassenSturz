// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Attack scenario B6 of the outside security review: hostile import files.
//! Each file is built here from made-up data (AGENTS.md §3.1 rule 2) and
//! read through [`import`] with the default [`Limits`], as the app reads
//! it. The pass condition is the review's: each file is refused with an
//! error, or read within the limits, never hanging or eating memory.
//! Hiding characters are kept as the file delivered them (the original is
//! never changed) and stripped where an AI reads text, which
//! `kassensturz-ai-tools` tests. That Undo takes a whole import back is
//! tested in `kassensturz-pipeline`.

use std::io::Write as _;
use std::time::{Duration, Instant};

use kassensturz_core::CurrencyCode;
use kassensturz_import::{
    Camt053, Csv, CsvSettings, ImportError, Imported, Importer, Kmy, Limits, Mt940, Ofx, currency,
    import,
};

/// Longer than any of these files may take, even in a debug build.
const PATIENCE: Duration = Duration::from_secs(60);

/// Imports `input` with `importer` and the default limits, and checks that
/// it took less than [`PATIENCE`].
fn timed_import(importer: &dyn Importer, input: &[u8]) -> Result<Imported, ImportError> {
    let started = Instant::now();
    let result = import(importer, input, &Limits::default());
    assert!(
        started.elapsed() < PATIENCE,
        "the import took {:?}",
        started.elapsed()
    );
    result
}

fn camt(entries: &str, prolog: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
{prolog}<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.02">
  <BkToCstmrStmt>
    <Stmt>
      <Acct><Id><IBAN>DE89370400440532013000</IBAN></Id><Ccy>EUR</Ccy></Acct>
      {entries}
    </Stmt>
  </BkToCstmrStmt>
</Document>
"#
    )
}

const ENTRY: &str = r#"<Ntry><Amt Ccy="EUR">10.00</Amt><CdtDbtInd>CRDT</CdtDbtInd><Sts>BOOK</Sts><BookgDt><Dt>2026-10-01</Dt></BookgDt><AddtlNtryInf>&lol9;</AddtlNtryInf></Ntry>"#;

#[test]
fn b6_camt_with_a_billion_laughs_is_refused() {
    let mut prolog = String::from("<!DOCTYPE Document [\n<!ENTITY lol \"lol\">\n");
    prolog.push_str("<!ENTITY lol1 \"&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;\">\n");
    for level in 2..=9 {
        let previous = format!("&lol{};", level - 1);
        prolog.push_str(&format!(
            "<!ENTITY lol{level} \"{}\">\n",
            previous.repeat(10)
        ));
    }
    prolog.push_str("]>\n");
    let file = camt(ENTRY, &prolog);
    let result = timed_import(&Camt053, file.as_bytes());
    assert!(
        matches!(result, Err(ImportError::Malformed { .. })),
        "{result:?}"
    );
}

#[test]
fn b6_camt_with_an_external_entity_is_refused() {
    let prolog = "<!DOCTYPE Document [<!ENTITY lol9 SYSTEM \"file:///etc/passwd\">]>\n";
    let file = camt(ENTRY, prolog);
    let result = timed_import(&Camt053, file.as_bytes());
    assert!(result.is_err(), "{result:?}");
}

/// A `.kmy` file: gzip that unpacks to `unpacked` bytes of the same letter.
fn gzip_bomb(unpacked: usize) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    let chunk = vec![b' '; 1024 * 1024];
    let mut written = 0;
    encoder
        .write_all(b"<?xml version=\"1.0\"?><KMYMONEY-FILE>")
        .expect("written");
    while written < unpacked {
        encoder.write_all(&chunk).expect("written");
        written += chunk.len();
    }
    encoder.finish().expect("packed")
}

#[test]
fn b6_a_kmy_gzip_bomb_stops_at_the_file_size_limit() {
    let limits = Limits::default();
    let unpacked = usize::try_from(limits.max_file_bytes).expect("fits") * 2;
    let bomb = gzip_bomb(unpacked);
    assert!(bomb.len() < 4 * 1024 * 1024, "the bomb is small packed");
    let result = timed_import(&Kmy, &bomb);
    assert!(
        matches!(result, Err(ImportError::FileTooLarge { .. })),
        "{result:?}"
    );
}

#[test]
fn b6_an_mt940_with_more_entries_than_the_limit_is_refused() {
    let limits = Limits::default();
    let mut file = String::from(":20:X\n:25:DE89370400440532013000\n:60F:C261001EUR0,00\n");
    for _ in 0..=limits.max_entries {
        file.push_str(":61:2610021002C1,00NMSCNONREF\n");
    }
    file.push_str(":62F:C261002EUR0,00\n-\n");
    let result = timed_import(&Mt940, file.as_bytes());
    assert!(result.is_err(), "a million and one entries were read");
}

#[test]
fn b6_an_ofx_nested_deeper_than_the_limit_is_refused() {
    let depth = 100_000;
    let mut file = String::from("OFXHEADER:100\nDATA:OFXSGML\nVERSION:102\n\n<OFX>");
    file.push_str(&"<A>".repeat(depth));
    file.push_str(&"</A>".repeat(depth));
    file.push_str("</OFX>");
    let result = timed_import(&Ofx, file.as_bytes());
    assert!(
        matches!(result, Err(ImportError::TooDeeplyNested { .. })),
        "{result:?}"
    );
}

fn euro_csv() -> Csv {
    let settings = CsvSettings::from_text(
        "delimiter = \";\"\ndecimal-mark = comma\ndate-order = day-month-year\n\
         booking-date = \"Datum\"\namount = \"Betrag\"\nother-party-name = \"Name\"\n",
    )
    .expect("valid settings");
    let euro = currency(CurrencyCode::new("EUR").expect("a valid code")).expect("a currency");
    Csv::new(settings, euro).expect("an importer")
}

#[test]
fn b6_a_csv_with_one_100_mb_line_is_refused() {
    let mut file = String::from("Datum;Name;Betrag\n25.10.2026;");
    file.push_str(&"x".repeat(100 * 1024 * 1024));
    file.push_str(";-1,00\n");
    let result = timed_import(&euro_csv(), file.as_bytes());
    assert!(result.is_err(), "a 100 MB payee was read");
}
