// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The importer as a whole; what it makes of the sample and of damaged
//! files is in its golden files (`tests/golden/kmy-sqlite/`). Cases the
//! sample doesn't have are made by editing a copy of it in memory (D-102).

use kassensturz_core::{Currency, CurrencyCode, Date};
use proptest::prelude::*;
use rusqlite::{Connection, MAIN_DB};

use super::*;
use crate::book::{Replacement, UnlistedCurrency};
use crate::importer::import;
use crate::kmy::Kmy;

/// The project's KMyMoney sample, saved by KMyMoney as a SQLite file
/// (D-102).
const SAMPLE: &[u8] = include_bytes!("../../../tests/golden/kmy-sqlite/kassensturz-sample.sqlite");

/// The same book as a `.kmy` file.
const KMY_SAMPLE: &[u8] = include_bytes!("../../../tests/golden/kmy/kassensturz-sample.kmy");

/// A copy of the sample changed by `change`.
fn edited(change: impl FnOnce(&Connection)) -> Vec<u8> {
    let mut connection = Connection::open_in_memory().expect("a database in memory");
    connection
        .deserialize_read_exact(MAIN_DB, SAMPLE, SAMPLE.len(), false)
        .expect("the sample is a database");
    change(&connection);
    connection
        .serialize(MAIN_DB)
        .expect("a database in memory serializes")
        .to_vec()
}

/// A copy of the sample after running `statements` on it.
fn after(statements: &str) -> Vec<u8> {
    edited(|connection| {
        connection
            .execute_batch(statements)
            .unwrap_or_else(|error| panic!("{statements}: {error}"));
    })
}

fn read(input: &[u8]) -> Result<Imported, ImportError> {
    import(&KmySqlite, input, &Limits::default())
}

fn skipped(input: &[u8]) -> Vec<(SkippedData, usize)> {
    let Ok(Imported::Book(book)) = read(input) else {
        panic!("a book");
    };
    book.skipped
        .into_iter()
        .map(|skipped| (skipped.what, skipped.count))
        .collect()
}

fn count_of(skipped: &[(SkippedData, usize)], what: SkippedData) -> usize {
    skipped
        .iter()
        .find(|(kind, _)| *kind == what)
        .map_or(0, |&(_, count)| count)
}

#[test]
fn a_kmymoney_database_is_recognized_by_its_start() {
    let info = KmySqlite.info();
    assert_eq!(
        (info.id, info.kind, info.extensions),
        ("kmy-sqlite", ImportKind::Book, &["sqlite", "db"][..])
    );
    // The tables' names can be further into the file than the first bytes,
    // as in the sample; any SQLite file could be KMyMoney's.
    assert_eq!(
        KmySqlite.recognize(&SAMPLE[..RECOGNITION_BYTES]),
        Recognition::Maybe
    );
    let mut named = MAGIC.to_vec();
    named.extend_from_slice(b"....kmmFileInfo....");
    assert_eq!(KmySqlite.recognize(&named), Recognition::Yes);
    for other in [
        &b""[..],
        b"SQLite format",
        b"<?xml version=\"1.0\"?>\n<OFX></OFX>",
        &[0x1f, 0x8b],
    ] {
        assert_eq!(KmySqlite.recognize(other), Recognition::No, "{other:?}");
    }
    // The two KMyMoney importers never take each other's files.
    assert_eq!(Kmy.recognize(&SAMPLE[..RECOGNITION_BYTES]), Recognition::No);
    assert_eq!(
        KmySqlite.recognize(&KMY_SAMPLE[..RECOGNITION_BYTES]),
        Recognition::No
    );
}

#[test]
fn the_sample_is_the_book_the_kmy_sample_is() {
    let Ok(Imported::Book(book)) = read(SAMPLE) else {
        panic!("the sample is a book");
    };
    assert_eq!(book.accounts.len(), 57);
    assert_eq!(book.transactions.len(), 538);
    let parts: usize = book
        .transactions
        .iter()
        .map(|transaction| transaction.parts.len())
        .sum();
    assert_eq!(parts, 540);
    // Both files were saved by KMyMoney from the same data, so what they
    // give must be the same, down to what is left out and its counts.
    assert_eq!(read(SAMPLE), import(&Kmy, KMY_SAMPLE, &Limits::default()));
}

#[test]
fn a_book_keeps_the_limits() {
    let few = Limits {
        max_entries: 5,
        ..Limits::default()
    };
    assert_eq!(
        import(&KmySqlite, SAMPLE, &few),
        Err(ImportError::TooManyEntries { max: 5 })
    );
    let small = Limits {
        max_file_bytes: 64 * 1024,
        ..Limits::default()
    };
    assert_eq!(
        import(&KmySqlite, SAMPLE, &small),
        Err(ImportError::FileTooLarge {
            max_bytes: 64 * 1024
        })
    );
    // The sample has memos of more than 12 bytes.
    let short = Limits {
        max_text_bytes: 12,
        ..Limits::default()
    };
    assert_eq!(
        import(&KmySqlite, SAMPLE, &short),
        Err(ImportError::TextTooLong {
            max_bytes: 12,
            line: None
        })
    );
}

#[test]
fn what_is_no_kmymoney_database_is_malformed() {
    let malformed = Err(ImportError::Malformed { line: None });
    let mut not_a_database = MAGIC.to_vec();
    not_a_database.extend_from_slice(&[7; 200]);
    for (what, input) in [
        ("empty", Vec::new()),
        ("text", b"Not a database\n".to_vec()),
        ("garbage after the header", not_a_database),
        ("cut off", SAMPLE[..SAMPLE.len() / 2].to_vec()),
        (
            "another layout of the tables",
            after("UPDATE kmmFileInfo SET version = '14'"),
        ),
        ("no layout", after("DELETE FROM kmmFileInfo")),
        (
            "two layouts",
            after("INSERT INTO kmmFileInfo (version) VALUES ('15')"),
        ),
        ("a table missing", after("DROP TABLE kmmSplits")),
        (
            "a view in place of a table",
            after(
                "DROP TABLE kmmPrices; CREATE VIEW kmmPrices (fromId, toId, priceDate, price) \
                 AS SELECT 'EUR', 'USD', '2020-01-01', '1/1'",
            ),
        ),
        (
            "a column missing",
            after("ALTER TABLE kmmSplits RENAME COLUMN bankId TO other"),
        ),
        (
            "a split of no transaction",
            after(
                "UPDATE kmmSplits SET transactionId = 'T-none' \
                 WHERE transactionId = 'T000000000000000002'",
            ),
        ),
        (
            "binary data in place of text",
            after("UPDATE kmmAccounts SET accountName = x'4142' WHERE id = 'A000001'"),
        ),
        (
            "text that isn't UTF-8",
            after("UPDATE kmmAccounts SET accountName = CAST(x'ff' AS TEXT) WHERE id = 'A000001'"),
        ),
    ] {
        assert_eq!(read(&input), malformed, "{what}");
    }
}

#[test]
fn an_amount_is_read_exactly_or_refused() {
    // The column is text, so a decimal number is kept as the text of one.
    assert_eq!(
        read(&after(
            "UPDATE kmmSplits SET value = 1.5 WHERE transactionId = 'T000000000000000002'"
        )),
        Err(ImportError::BadAmount {
            found: "1.5".to_owned(),
            line: None
        })
    );
    // 1/3 isn't a whole number of cents.
    assert!(matches!(
        read(&after(
            "UPDATE kmmSplits SET value = '1/3', shares = '1/3' \
             WHERE transactionId = 'T000000000000000002' AND splitId = 0"
        )),
        Err(ImportError::TooManyDecimalPlaces { .. })
    ));
    // A currency KassenSturz doesn't know is refused, as for a `.kmy` file
    // (D-101).
    assert!(matches!(
        read(&after(
            "UPDATE kmmAccounts SET currencyId = 'ABC' WHERE id = 'A000001'"
        )),
        Err(ImportError::UnknownCurrency { line: None, .. })
    ));
    // A withdrawn one is read, for the new book to convert (D-104).
    let Ok(Imported::Book(book)) = read(&after(
        "UPDATE kmmAccounts SET currencyId = 'DEM' WHERE id = 'A000001'",
    )) else {
        panic!("a book");
    };
    let account = book
        .accounts
        .iter()
        .find(|account| account.key == "A000001")
        .expect("the account");
    assert_eq!(account.currency.code().as_str(), "DEM");
}

#[test]
fn the_owner_is_no_payee_and_is_left_out_when_given() {
    let base = skipped(SAMPLE);
    assert_eq!(count_of(&base, SkippedData::Owner), 0);
    let payees = |input: &[u8]| match read(input).expect("read") {
        Imported::Book(book) => book.payees.len(),
        other => panic!("a book, not {other:?}"),
    };
    assert_eq!(payees(SAMPLE), 131);
    let named_owner = after("UPDATE kmmPayees SET name = 'Someone' WHERE id = 'USER'");
    let named = skipped(&named_owner);
    assert_eq!(count_of(&named, SkippedData::Owner), 1);
    assert_eq!(payees(&named_owner), 131);
    let addressed = skipped(&after(
        "UPDATE kmmPayees SET addressCity = 'Somewhere' WHERE id = 'USER'",
    ));
    assert_eq!(count_of(&addressed, SkippedData::Owner), 1);
}

#[test]
fn tags_come_with_the_parts_that_carry_them() {
    let tagged = after(
        "INSERT INTO kmmTags (id, name) VALUES ('G000001', 'Holiday'), ('G000002', 'Gift');
         INSERT INTO kmmTagSplits (transactionId, tagId, splitId)
             SELECT transactionId, 'G000002', splitId FROM kmmSplits
             WHERE transactionId = (SELECT min(id) FROM kmmTransactions) AND splitId = 0;
         INSERT INTO kmmTagSplits (transactionId, tagId, splitId)
             SELECT transactionId, 'G000001', splitId FROM kmmSplits
             WHERE transactionId = (SELECT min(id) FROM kmmTransactions) AND splitId = 0;
         INSERT INTO kmmTagSplits (transactionId, tagId, splitId)
             VALUES ('T000000000000000001', 'G000001', 99);",
    );
    let Imported::Book(book) = read(&tagged).expect("read") else {
        panic!("a book");
    };
    let names: Vec<&str> = book.tags.iter().map(|tag| tag.name.as_str()).collect();
    assert_eq!(names, ["Holiday", "Gift"]);
    let first = book
        .transactions
        .iter()
        .find(|transaction| !transaction.parts[0].tags.is_empty())
        .expect("a tagged part");
    assert_eq!(first.parts[0].tags, ["G000002", "G000001"]);
    // The tag on a part the file doesn't have is left out, counted.
    assert_eq!(count_of(&skipped(&tagged), SkippedData::Tags), 1);
}

#[test]
fn what_the_book_cant_hold_is_counted() {
    let base = skipped(SAMPLE);
    let more = skipped(&after(
        "INSERT INTO kmmTags (id, name) VALUES ('G000001', 'a'), ('G000002', 'b');
         INSERT INTO kmmCostCenter (id, name) VALUES ('C000001', 'a');
         INSERT INTO kmmBudgetConfig (id, name, start) VALUES ('B000001', 'a', '2020-01-01');
         INSERT INTO kmmReportConfig (name, id) VALUES ('a', 'R000001');
         INSERT INTO kmmOnlineJobs (id, type, state, locked) VALUES ('O000001', 'a', 'a', 'N');
         INSERT INTO kmmSchedules (id, name, type, occurence, occurenceMultiplier, startDate,
             fixed, autoEnter, weekendOption)
             VALUES ('SCH000001', 'a', 1, 1, 1, '2020-01-01', 'Y', 'N', 0);
         INSERT INTO kmmAccountsPayeeIdentifier (accountId, userOrder, identifierId)
             VALUES ('A000001', 0, 'x');
         INSERT INTO kmmPayeesPayeeIdentifier (payeeId, userOrder, identifierId)
             VALUES ('P000001', 0, 'x');
         INSERT INTO kmmKeyValuePairs (kvpType, kvpId, kvpKey, kvpData)
             VALUES ('ONLINEBANKING', 'A000007', 'provider', 'a');
         CREATE TABLE somethingElse (x);",
    ));
    assert_eq!(count_of(&base, SkippedData::OnlineBanking), 2);
    for (what, extra) in [
        // Cost centers, the account's and the payee's identifiers, and the
        // unknown table.
        (SkippedData::Other, 4),
        (SkippedData::Budgets, 1),
        (SkippedData::Reports, 1),
        // An order, and the settings of one more account.
        (SkippedData::OnlineBanking, 2),
        (SkippedData::Schedules, 1),
    ] {
        assert_eq!(
            count_of(&more, what),
            count_of(&base, what) + extra,
            "{what:?}"
        );
    }
}

#[test]
fn a_database_in_memory_is_never_changed() {
    // The connection is read-only: nothing the importer runs can write.
    let connection = open(SAMPLE).expect("the sample opens");
    assert!(connection.execute("DELETE FROM kmmAccounts", []).is_err());
    assert!(connection.execute("DROP TABLE kmmSplits", []).is_err());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn no_input_makes_reading_panic(input in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = KmySqlite.recognize(&input);
        let _ = read(&input);
        let mut with_header = MAGIC.to_vec();
        with_header.extend_from_slice(&input);
        let _ = read(&with_header);
    }

    #[test]
    fn no_change_to_a_database_makes_reading_panic(
        at in 0..SAMPLE.len(),
        removed in 0_usize..64,
        inserted in proptest::collection::vec(any::<u8>(), 0..16),
    ) {
        let end = (at + removed).min(SAMPLE.len());
        let mut input = SAMPLE[..at].to_vec();
        input.extend_from_slice(&inserted);
        input.extend_from_slice(&SAMPLE[end..]);
        let _ = read(&input);
    }

    #[test]
    fn a_file_cut_off_is_refused_or_the_same_book(cut in 0..SAMPLE.len()) {
        // The end of the sample holds pages the data doesn't need.
        let result = read(&SAMPLE[..cut]);
        prop_assert!(result.is_err() || result == read(SAMPLE));
    }
}

#[test]
fn a_currency_no_table_lists_comes_from_the_currency_list() {
    // Cash in the old Romanian leu, which the sample's currency list and
    // price table have (D-106).
    let cash = "INSERT INTO kmmAccounts (id, parentId, accountType, accountName, currencyId) \
                VALUES ('A999999', 'AStd::Asset', '3', 'Lei in cash', 'ROL');";
    let code = |text| CurrencyCode::new(text).expect("a code");
    let Ok(Imported::Book(book)) = read(&after(cash)) else {
        panic!("a book");
    };
    assert_eq!(
        book.unlisted,
        vec![UnlistedCurrency {
            currency: Currency::new(code("ROL"), 2).expect("2 places"),
            suggestion: Some(Replacement {
                successor: code("RON"),
                rate: "10000".parse().expect("a rate"),
                date: Date::constant(2005, 6, 30),
            }),
        }]
    );
    // Without it in the list, or with a fraction that isn't a power of ten,
    // its amounts can't be read.
    let unknown = Err(ImportError::UnknownCurrency {
        currency: code("ROL"),
        line: None,
    });
    for change in [
        "DELETE FROM kmmCurrencies WHERE ISOcode = 'ROL'",
        "DROP TABLE kmmCurrencies",
    ] {
        assert_eq!(
            read(&after(&format!("{cash} {change}"))),
            unknown,
            "{change}"
        );
    }
    assert_eq!(
        read(&after(&format!(
            "{cash} UPDATE kmmCurrencies SET smallestAccountFraction = '50' \
             WHERE ISOcode = 'ROL'"
        ))),
        Err(ImportError::Malformed { line: None })
    );
}
