// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The example importer plugin (#791 step 9, `examples/plugins/simple-bank-text`),
//! run for real: the plugin file that is checked in, through the same
//! `import` as every built-in importer.

use kassensturz_import::{
    EntryStatus, ImportError, ImportKind, Imported, Importer, Limits, Recognition, import,
};
use kassensturz_plugin_host::Plugin;

use crate::PluginImporter;
use crate::tests::{date, eur};

const PLUGIN: &[u8] =
    include_bytes!("../../../examples/plugins/simple-bank-text/simple-bank-text.wasm");
const MANIFEST: &[u8] = include_bytes!("../../../examples/plugins/simple-bank-text/manifest.json");
const SAMPLE: &[u8] = include_bytes!("../../../examples/plugins/simple-bank-text/sample.txt");

fn importer() -> PluginImporter {
    PluginImporter::new(Plugin::load(PLUGIN, false).unwrap()).unwrap()
}

fn read(text: &str) -> Result<Imported, ImportError> {
    import(&importer(), text.as_bytes(), &Limits::default())
}

#[test]
fn the_plugin_file_holds_the_manifest_beside_it() {
    let plugin = Plugin::load(PLUGIN, false).unwrap();
    assert_eq!(
        plugin.manifest().id.as_str(),
        "io.github.vondor.kassensturz.example.simple-bank-text"
    );
    // The file is built from manifest.json, byte for byte.
    let manifest = std::str::from_utf8(MANIFEST).unwrap();
    assert!(
        PLUGIN
            .windows(MANIFEST.len())
            .any(|window| window == MANIFEST),
        "the plugin file doesn't hold manifest.json:\n{manifest}"
    );
    assert!(plugin.manifest().permissions.is_empty());
}

#[test]
fn the_example_plugin_reads_the_sample_statement() {
    let importer = importer();
    let info = importer.info();
    assert_eq!(info.kind, ImportKind::Statements);
    assert_eq!(info.extensions, ["txt"]);
    assert_eq!(importer.recognize(SAMPLE), Recognition::Yes);
    assert_eq!(importer.recognize(b"Date;Amount\n"), Recognition::No);
    assert_eq!(importer.recognize(b""), Recognition::No);

    let Imported::Statements(statements) =
        import(&importer, SAMPLE, &Limits::default()).expect("statements")
    else {
        panic!("statements expected");
    };
    let [statement] = statements.as_slice() else {
        panic!("one statement");
    };
    assert_eq!(
        statement.account.iban.as_deref(),
        Some("DE89370400440532013000")
    );
    assert_eq!(statement.currency.minor_digits(), 2);
    let opening = statement.opening_balance.unwrap();
    assert_eq!(
        (opening.amount, opening.date),
        (eur(100_000), date("2026-10-31"))
    );
    let closing = statement.closing_balance.unwrap();
    assert_eq!(
        (closing.amount, closing.date),
        (eur(302_655), date("2026-11-02"))
    );

    let [bakery, pay, rent] = statement.entries.as_slice() else {
        panic!("three entries");
    };
    assert_eq!(bakery.status, EntryStatus::Booked);
    assert_eq!(bakery.booking_date, date("2026-11-01"));
    assert_eq!(bakery.amount, eur(-2345));
    assert_eq!(bakery.other_party_name.as_deref(), Some("Example Bakery"));
    assert_eq!(bakery.remittance_information.as_deref(), Some("Bread"));
    assert_eq!(pay.amount, eur(250_000));
    assert_eq!(
        pay.remittance_information.as_deref(),
        Some("Pay for October")
    );
    assert_eq!(rent.amount, eur(-45_000));

    // The opening balance and the entries add up to the closing balance:
    // the amounts crossed the plugin boundary exactly.
    let total: i64 = statement
        .entries
        .iter()
        .map(|entry| entry.amount.minor_units())
        .sum();
    assert_eq!(
        opening.amount.minor_units() + total,
        closing.amount.minor_units()
    );
}

#[test]
fn amounts_are_exact_in_every_shape() {
    let file = |amount: &str| {
        format!(
            "EXAMPLE-BANK STATEMENT\nAccount: DE89370400440532013000\nCurrency: EUR\n\
             Opening: 2026-10-31 0.00\n2026-11-01 | Example Shop | Test | {amount}\n\
             Closing: 2026-11-01 0.00\n"
        )
    };
    for (text, minor) in [
        ("0.00", 0),
        ("-0.00", 0),
        ("0.05", 5),
        ("-0.05", -5),
        ("007.10", 710),
        ("12345678901234.99", 1_234_567_890_123_499),
        ("-12345678901234.99", -1_234_567_890_123_499),
    ] {
        let Imported::Statements(statements) = read(&file(text)).expect(text) else {
            panic!("statements expected");
        };
        assert_eq!(statements[0].entries[0].amount, eur(minor), "{text}");
    }
}

#[test]
fn the_plugin_says_which_line_it_cant_read() {
    let good = "EXAMPLE-BANK STATEMENT\nAccount: DE89370400440532013000\nCurrency: EUR\n\
                Opening: 2026-10-31 10.00\n2026-11-01 | Example Shop | Test | -1.00\n\
                Closing: 2026-11-01 9.00\n";
    assert!(read(good).is_ok());

    let broken = |from: &str, to: &str| good.replace(from, to);
    for (text, line) in [
        // Not this format at all.
        (String::from("Date;Amount\n1;2\n"), 1),
        (broken("EUR", "JPY"), 3),
        (broken("-1.00", "-1,00"), 5),
        (broken("-1.00", "-1.0"), 5),
        (broken("-1.00", "1e3"), 5),
        (
            broken("2026-11-01 | Example Shop", "01.11.2026 | Example Shop"),
            5,
        ),
        (broken("| Test |", "| Test | extra |"), 5),
        (broken("Closing: 2026-11-01 9.00\n", ""), 6),
        (format!("{good}2026-11-02 | Late | x | 1.00\n"), 7),
    ] {
        assert_eq!(
            read(&text).unwrap_err(),
            ImportError::Malformed { line: Some(line) },
            "{text}"
        );
    }
}

#[test]
fn text_from_the_file_is_escaped_not_trusted() {
    let file = "EXAMPLE-BANK STATEMENT\nAccount: DE89370400440532013000\nCurrency: EUR\n\
                Opening: 2026-10-31 0.00\n\
                2026-11-01 | Say \"hi\" \\ there | {\"amount\": \"1\"} | 1.00\n\
                Closing: 2026-11-01 1.00\n";
    let Imported::Statements(statements) = read(file).expect("statements") else {
        panic!("statements expected");
    };
    let entry = &statements[0].entries[0];
    assert_eq!(
        entry.other_party_name.as_deref(),
        Some("Say \"hi\" \\ there")
    );
    assert_eq!(
        entry.remittance_information.as_deref(),
        Some("{\"amount\": \"1\"}")
    );
}

#[test]
fn the_plugin_can_be_called_many_times() {
    // Each call starts the plugin's memory again, so it never runs out.
    let importer = importer();
    for _ in 0..200 {
        assert!(import(&importer, SAMPLE, &Limits::default()).is_ok());
    }
}
