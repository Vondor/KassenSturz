// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The complete export: a golden file, reading it back, and every way a file
//! is refused. All data is made up (AGENTS.md §3.1 rule 2); the IBAN is the
//! example for Germany in SWIFT's IBAN Registry (D-055).

use std::fs;
use std::path::PathBuf;

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, Actions, AmountTest, BookTo, Condition, Currency,
    CurrencyCode, Date, DatedRate, DatedRateId, ExchangeRate, ForeignAmount, ImportedDetails,
    MatchMode, Money, OriginalAmount, RateSource, RoundingMode, Rule, RuleId, RuleNode, Split,
    SplitId, TextField, TextOperator, Transaction, TransactionError, TransactionId,
};
use kassensturz_export::{
    BookData, BookExport, BookedByRule, CsvExport, Expected, ExportData, ExportError, ExportKind,
    Exporter, ExporterInfo, MAX_RULE_DEPTH, export, export_file, read_book, read_book_file,
};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};
const JPY: CurrencyCode = match CurrencyCode::new("JPY") {
    Ok(code) => code,
    Err(_) => panic!("JPY is a valid code"),
};
const DEM: CurrencyCode = match CurrencyCode::new("DEM") {
    Ok(code) => code,
    Err(_) => panic!("DEM is a valid code"),
};

/// A fixed UUIDv7, so the golden file stays the same: `n` is its last byte.
fn id(n: u8) -> [u8; 16] {
    [
        0x01, 0x9a, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0, 0, 0, 0, 0, n,
    ]
}

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn day(day: i8) -> Date {
    Date::new(2026, 10, day).expect("a valid date")
}

fn account(
    n: u8,
    name: &str,
    kind: AccountKind,
    account_type: Option<AccountType>,
    currency: CurrencyCode,
    parent: Option<u8>,
) -> Account {
    Account {
        id: AccountId::from_bytes(id(n)),
        name: name.into(),
        kind,
        currency,
        account_type,
        parent: parent.map(|parent| AccountId::from_bytes(id(parent))),
        closed: false,
    }
}

fn split(n: u8, account: Option<u8>, amount: Money) -> Split {
    Split {
        id: SplitId::from_bytes(id(n)),
        account: account.map(|account| AccountId::from_bytes(id(account))),
        amount,
        foreign: None,
        imported: None,
        original: None,
    }
}

fn transaction(n: u8, date: Date, payee: Option<&str>, splits: Vec<Split>) -> Transaction {
    Transaction::new(
        TransactionId::from_bytes(id(n)),
        date,
        payee.map(str::to_owned),
        splits,
    )
    .expect("balanced")
}

fn rule(n: u8, name: &str, conditions: Vec<Condition>, book_to: Option<BookTo>) -> Rule {
    Rule {
        id: RuleId::from_bytes(id(n)),
        name: name.into(),
        match_mode: MatchMode::All,
        conditions,
        actions: Actions {
            book_to,
            payee: None,
        },
    }
}

fn rate(n: u8, date: Date, from: CurrencyCode, to: CurrencyCode, rate: &str) -> DatedRate {
    DatedRate {
        id: DatedRateId::from_bytes(id(n)),
        date,
        from,
        to,
        rate: rate.parse::<ExchangeRate>().expect("valid"),
        source: RateSource::Manual,
    }
}

/// A small book with something of everything the format holds, and text
/// that needs care: quotes, a line break, umlauts and a formula.
fn sample() -> BookData {
    let checking = 1;
    let cash = 2;
    let start = 3;
    let food = 4;
    let bakery = 5;
    let travel = 6;
    let mut closed = account(
        7,
        "Old \"savings\"",
        AccountKind::Asset,
        Some(AccountType::Savings),
        EUR,
        None,
    );
    closed.closed = true;

    let mut bought = split(31, Some(checking), eur(-450));
    bought.imported = Some(ImportedDetails {
        bank_id: Some("2026-10-05-0001".into()),
        booking_date: Some(day(5)),
        value_date: Some(day(6)),
        other_party_name: Some("Bäckerei Sonnenschein".into()),
        other_party_iban: Some("DE89 3704 0044 0532 0130 00".into()),
        remittance_information: Some("=Brötchen\nKarte 4711".into()),
    });
    let mut dollars = split(41, Some(cash), eur(9_210));
    dollars.foreign = Some(ForeignAmount {
        amount: Money::from_minor_units(10_000, USD),
        rate: Some(rate(12, day(1), USD, EUR, "0.9210")),
    });
    let mut hotel = split(51, Some(travel), eur(9_000));
    hotel.foreign = Some(ForeignAmount {
        amount: Money::from_minor_units(9_800, USD),
        rate: None,
    });
    // A purchase of 100.00 DEM from a book kept in marks (D-104).
    let fixed = DEM
        .withdrawn()
        .expect("the table lists the mark")
        .dated_rate(DatedRateId::from_bytes(id(73)));
    let mut paid = split(71, Some(checking), eur(-5_113));
    paid.original = Some(OriginalAmount {
        amount: Money::from_minor_units(-10_000, DEM),
        rate: fixed,
    });
    let mut bread_in_marks = split(72, Some(bakery), eur(5_113));
    bread_in_marks.original = Some(OriginalAmount {
        amount: Money::from_minor_units(10_000, DEM),
        rate: DatedRate {
            id: DatedRateId::from_bytes(id(74)),
            ..fixed
        },
    });

    let mut groceries = rule(
        21,
        "Groceries",
        vec![
            Condition::Text {
                field: TextField::OtherPartyName,
                operator: TextOperator::Matches,
                value: "^B(ä|ae)ckerei".into(),
                case_sensitive: true,
            },
            Condition::Amount(AmountTest::Between {
                min: eur(-5_000),
                max: eur(-1),
            }),
            Condition::Amount(AmountTest::MoneyOut),
            Condition::BookingDate {
                first: day(1),
                last: day(31),
            },
            Condition::Currency(EUR),
            Condition::Account(AccountId::from_bytes(id(checking))),
        ],
        Some(BookTo::Category(AccountId::from_bytes(id(food)))),
    );
    groceries.match_mode = MatchMode::Any;
    let mut bread = rule(
        22,
        "Bread",
        vec![
            Condition::Text {
                field: TextField::RemittanceInformation,
                operator: TextOperator::Contains,
                value: "Brötchen".into(),
                case_sensitive: false,
            },
            Condition::Amount(AmountTest::Equals(eur(-450))),
        ],
        Some(BookTo::Category(AccountId::from_bytes(id(bakery)))),
    );
    bread.actions.payee = Some("Bakery".into());
    let cash_rule = rule(
        23,
        "Cash",
        vec![
            Condition::Text {
                field: TextField::Payee,
                operator: TextOperator::StartsWith,
                value: "ATM".into(),
                case_sensitive: false,
            },
            Condition::Amount(AmountTest::MoneyIn),
        ],
        Some(BookTo::Transfer(AccountId::from_bytes(id(cash)))),
    );

    BookData {
        rounding_mode: RoundingMode::HalfEven,
        history_keep_days: Some(30),
        currencies: vec![
            Currency::new(EUR, 2).expect("valid"),
            Currency::new(JPY, 0).expect("valid"),
            Currency::new(USD, 2).expect("valid"),
            Currency::new(DEM, 2).expect("valid"),
        ],
        accounts: vec![
            account(
                checking,
                "Girokonto",
                AccountKind::Asset,
                Some(AccountType::Checking),
                EUR,
                None,
            ),
            account(
                cash,
                "Dollars",
                AccountKind::Asset,
                Some(AccountType::Cash),
                USD,
                None,
            ),
            closed,
            account(
                start,
                "Starting balances",
                AccountKind::Equity,
                None,
                EUR,
                None,
            ),
            account(food, "Food", AccountKind::Expense, None, EUR, None),
            account(
                bakery,
                "Bakery",
                AccountKind::Expense,
                None,
                EUR,
                Some(food),
            ),
            account(travel, "Travel", AccountKind::Expense, None, USD, None),
        ],
        rates: vec![
            rate(11, day(2), EUR, USD, "1.0850"),
            rate(12, day(1), USD, EUR, "0.9210"),
        ],
        rules: vec![
            RuleNode {
                rule: groceries,
                sub_rules: vec![RuleNode {
                    rule: bread,
                    sub_rules: vec![],
                }],
            },
            RuleNode {
                rule: cash_rule,
                sub_rules: vec![],
            },
        ],
        transactions: vec![
            transaction(
                30,
                day(1),
                Some("Opening balance"),
                vec![
                    split(32, Some(checking), eur(100_000)),
                    split(33, Some(start), eur(-100_000)),
                ],
            ),
            transaction(
                34,
                day(5),
                None,
                vec![bought, split(35, Some(bakery), eur(450))],
            ),
            transaction(
                40,
                day(4),
                Some("Exchange office"),
                vec![dollars, split(42, Some(checking), eur(-9_210))],
            ),
            transaction(
                50,
                day(6),
                Some("Hotel"),
                vec![split(52, Some(checking), eur(-9_000)), hotel],
            ),
            transaction(
                60,
                day(7),
                Some("Unknown"),
                vec![split(61, Some(checking), eur(-1)), split(62, None, eur(1))],
            ),
            transaction(70, day(8), Some("Bakery"), vec![paid, bread_in_marks]),
        ],
        rule_marks: vec![
            BookedByRule {
                split: SplitId::from_bytes(id(35)),
                rule: Some(RuleId::from_bytes(id(22))),
            },
            BookedByRule {
                split: SplitId::from_bytes(id(62)),
                rule: None,
            },
        ],
    }
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/book/sample.json")
}

fn golden() -> String {
    let path = golden_path();
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn written(data: BookData) -> String {
    String::from_utf8(export(&BookExport, &ExportData::Book(data)).expect("written"))
        .expect("UTF-8")
}

/// `KASSENSTURZ_BLESS=1` writes the golden file instead, as for the
/// importers (D-091); check what changed before committing it.
#[test]
fn a_book_is_written_as_the_golden_file_says() {
    if std::env::var_os("KASSENSTURZ_BLESS").is_some_and(|bless| bless == "1") {
        fs::create_dir_all(golden_path().parent().expect("a folder")).expect("folder made");
        fs::write(golden_path(), written(sample())).expect("written");
    }
    assert_eq!(written(sample()), golden());
}

#[test]
fn the_golden_file_reads_back_as_the_book_it_came_from() {
    assert_eq!(read_book(golden().as_bytes()), Ok(sample()));
}

/// The example in `docs/EXPORT_FORMAT.md` reads, and is written back exactly
/// as it is there, so the document can't drift from the code.
#[test]
fn the_documented_example_reads_and_is_written_the_same() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/EXPORT_FORMAT.md");
    let document =
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    // Git may check the document out with Windows line endings.
    let document = document.replace("\r\n", "\n");
    let (_, after) = document.split_once("## Example").expect("an example");
    let (_, block) = after.split_once("```json\n").expect("a JSON block");
    let (example, _) = block.split_once("```").expect("its end");
    let book = read_book(example.as_bytes()).expect("the example reads");
    assert_eq!(book.transactions.len(), 1);
    assert_eq!(written(book), example);
}

#[test]
fn the_exporter_says_what_it_writes() {
    assert_eq!(
        BookExport.info(),
        ExporterInfo {
            id: "book",
            kind: ExportKind::Book,
            extension: "json",
        }
    );
}

/// A file another program might write: the least the format needs, with
/// every value that may be `null` left out.
const SHORT: &str = r#"{
  "format": "kassensturz-book",
  "version": 1,
  "settings": {"rounding_mode": "floor"},
  "currencies": [{"code": "EUR", "minor_digits": 2}],
  "accounts": [
    {"id": "019a0000-0000-7000-8000-000000000001", "name": "Cash", "kind": "asset", "currency": "EUR", "closed": false},
    {"id": "019A0000-0000-7000-8000-000000000002", "name": "Food", "kind": "expense", "currency": "EUR", "closed": false}
  ],
  "rates": [],
  "rules": [
    {"id": "019a0000-0000-7000-8000-000000000003", "name": "Market", "match": "all", "conditions": [
      {"field": "payee", "operator": "equals", "text": "Market", "case_sensitive": false}
    ]}
  ],
  "transactions": [
    {"id": "019a0000-0000-7000-8000-000000000004", "date": "2026-10-05", "currency": "EUR", "parts": [
      {"id": "019a0000-0000-7000-8000-000000000005", "account": "019a0000-0000-7000-8000-000000000001", "amount": "-5"},
      {"id": "019a0000-0000-7000-8000-000000000006", "amount": "5.0"}
    ]}
  ]
}"#;

#[test]
fn values_that_may_be_empty_can_be_left_out() {
    let book = read_book(SHORT.as_bytes()).expect("read");
    let mut cash = account(1, "Cash", AccountKind::Asset, None, EUR, None);
    cash.account_type = None;
    assert_eq!(
        book,
        BookData {
            rounding_mode: RoundingMode::Floor,
            history_keep_days: None,
            currencies: vec![Currency::new(EUR, 2).expect("valid")],
            accounts: vec![
                cash,
                account(2, "Food", AccountKind::Expense, None, EUR, None)
            ],
            rates: vec![],
            rules: vec![RuleNode {
                rule: rule(
                    3,
                    "Market",
                    vec![Condition::Text {
                        field: TextField::Payee,
                        operator: TextOperator::Equals,
                        value: "Market".into(),
                        case_sensitive: false,
                    }],
                    None,
                ),
                sub_rules: vec![],
            }],
            transactions: vec![transaction(
                4,
                day(5),
                None,
                vec![split(5, Some(1), eur(-500)), split(6, None, eur(500))],
            )],
            rule_marks: vec![],
        }
    );
}

/// The golden file with `from` replaced by `to`, once.
fn changed(from: &str, to: &str) -> Vec<u8> {
    let file = golden();
    assert!(file.contains(from), "{from}");
    file.replacen(from, to, 1).into_bytes()
}

fn bad(path: &str, expected: Expected) -> Result<BookData, ExportError> {
    Err(ExportError::BadValue {
        path: path.into(),
        expected,
    })
}

#[test]
fn what_is_not_a_complete_export_is_refused() {
    assert_eq!(
        read_book(b"{\"format\": \"kassensturz-book\",\n  \"version\": 1"),
        Err(ExportError::NotJson {
            line: 2,
            column: 14
        })
    );
    assert_eq!(read_book(b"[1, 2]"), Err(ExportError::NotABook));
    assert_eq!(read_book(b"{\"name\": \"x\"}"), Err(ExportError::NotABook));
    assert_eq!(
        read_book(&changed("\"version\": 2", "\"version\": 3")),
        Err(ExportError::NewerVersion { version: 3 })
    );
    for version in ["\"2\"", "0", "-1", "2.5"] {
        assert_eq!(
            read_book(&changed(
                "\"version\": 2",
                &format!("\"version\": {version}")
            )),
            bad("version", Expected::Number),
            "{version}"
        );
    }
}

#[test]
fn every_value_must_be_what_the_format_says() {
    let part = "transactions[2].parts[0].amount";
    for amount in [
        "\"92.105\"",
        "92.1",
        "\"92,10\"",
        "\"+92.10\"",
        "\"92.\"",
        "\" 92.10\"",
    ] {
        assert_eq!(
            read_book(&changed(
                "\"amount\": \"92.10\"",
                &format!("\"amount\": {amount}")
            )),
            bad(part, Expected::Amount),
            "{amount}"
        );
    }
    // The largest amount fits, one minor unit more doesn't.
    assert!(
        read_book(&changed(
            "\"code\": \"JPY\", \"minor_digits\": 0",
            "\"code\": \"JPY\", \"minor_digits\": 18"
        ))
        .is_ok()
    );
    assert_eq!(
        read_book(&changed(
            "\"amount\": \"-4.50\"",
            "\"amount\": \"-92233720368547758.09\""
        )),
        bad(
            "rules[0].sub_rules[0].conditions[1].amount",
            Expected::Amount
        )
    );
    assert_eq!(
        read_book(&changed(
            "\"date\": \"2026-10-05\"",
            "\"date\": \"5.10.2026\""
        )),
        bad("transactions[1].date", Expected::Date)
    );
    assert_eq!(
        read_book(&changed(
            "\"date\": \"2026-10-05\"",
            "\"date\": \"2026-02-30\""
        )),
        bad("transactions[1].date", Expected::Date)
    );
    assert_eq!(
        read_book(&changed("\"closed\": false", "\"closed\": 0")),
        bad("accounts[0].closed", Expected::YesOrNo)
    );
    assert_eq!(
        read_book(&changed("\"kind\": \"asset\"", "\"kind\": \"Asset\"")),
        bad("accounts[0].kind", Expected::Keyword)
    );
    assert_eq!(
        read_book(&changed("\"name\": \"Girokonto\"", "\"name\": null")),
        bad("accounts[0].name", Expected::Text)
    );
    assert_eq!(
        read_book(&changed("\"rate\": \"1.0850\"", "\"rate\": \"0\"")),
        bad("rates[0].rate", Expected::Rate)
    );
    assert_eq!(
        read_book(&changed("\"rate\": \"1.0850\"", "\"rate\": \"1e3\"")),
        bad("rates[0].rate", Expected::Rate)
    );
    assert_eq!(
        read_book(&changed("\"minor_digits\": 2", "\"minor_digits\": 19")),
        bad("currencies[0].minor_digits", Expected::Number)
    );
    assert_eq!(
        read_book(&changed(
            "\"history_keep_days\": 30",
            "\"history_keep_days\": -1"
        )),
        bad("settings.history_keep_days", Expected::Number)
    );
    assert_eq!(
        read_book(&changed("\"code\": \"EUR\"", "\"code\": \"eur\"")),
        bad("currencies[0].code", Expected::Currency)
    );
    assert_eq!(
        read_book(&changed(
            "\"id\": \"019a0000-0000-7000-8000-000000000001\"",
            "\"id\": \"019a00000000700080000000000000001\""
        )),
        bad("accounts[0].id", Expected::Id)
    );
    assert_eq!(
        read_book(&changed(
            "\"field\": \"currency\"",
            "\"field\": \"weather\""
        )),
        bad("rules[0].conditions[4].field", Expected::Keyword)
    );
    assert_eq!(
        read_book(&changed(
            "\"operator\": \"money_out\"",
            "\"operator\": \"contains\""
        )),
        bad("rules[0].conditions[2].operator", Expected::Keyword)
    );
    assert_eq!(
        read_book(&changed(
            "\"transfer_account\": null",
            "\"transfer_account\": \"019a0000-0000-7000-8000-000000000002\""
        )),
        bad("rules[0].transfer_account", Expected::Null)
    );
    assert_eq!(
        read_book(
            &SHORT
                .replacen("\"rates\": []", "\"rates\": {}", 1)
                .into_bytes()
        ),
        bad("rates", Expected::List)
    );
    assert_eq!(
        read_book(
            &SHORT
                .replacen("\"rates\": []", "\"rates\": [1]", 1)
                .into_bytes()
        ),
        bad("rates[0]", Expected::Object)
    );
}

#[test]
fn a_version_1_file_has_no_original_amounts() {
    // SHORT is a version 1 file. A version 2 file may give its parts the
    // original amounts they had in a withdrawn currency (D-104), with the
    // fixed rate as a rate from `changeover`.
    let original = r#""amount": "5.0", "original": {"amount": "9.78", "currency": "DEM", "rate": {"id": "019a0000-0000-7000-8000-000000000007", "date": "1999-01-01", "from": "EUR", "to": "DEM", "rate": "1.95583", "source": "changeover"}}"#;
    let marks = r#"{"code": "EUR", "minor_digits": 2}, {"code": "DEM", "minor_digits": 2}"#;
    let with_original = SHORT
        .replacen(r#"{"code": "EUR", "minor_digits": 2}"#, marks, 1)
        .replacen(
            r#""amount": "-5""#,
            r#""amount": "-5", "original": null"#,
            1,
        )
        .replacen(r#""amount": "5.0""#, original, 1);
    assert_eq!(
        read_book(with_original.as_bytes()),
        Err(ExportError::UnknownValue {
            path: "transactions[0].parts[0].original".into()
        })
    );
    let version_2 = with_original.replacen("\"version\": 1", "\"version\": 2", 1);
    let book = read_book(version_2.as_bytes()).expect("read");
    let part = &book.transactions[0].splits()[1];
    assert_eq!(
        part.original
            .map(|original| (original.amount, original.rate.source)),
        Some((Money::from_minor_units(978, DEM), RateSource::Changeover))
    );
    // A rate in version 1 is entered by hand.
    let changeover = SHORT.replacen(
        r#""rates": []"#,
        r#""rates": [{"id": "019a0000-0000-7000-8000-000000000007", "date": "1999-01-01", "from": "EUR", "to": "EUR", "rate": "1", "source": "changeover"}]"#,
        1,
    );
    assert_eq!(
        read_book(changeover.as_bytes()),
        bad("rates[0].source", Expected::Keyword)
    );
}

#[test]
fn nothing_is_missing_or_left_over() {
    assert_eq!(
        read_book(&changed("\"name\": \"Girokonto\", ", "")),
        Err(ExportError::MissingValue {
            path: "accounts[0].name".into()
        })
    );
    assert_eq!(
        read_book(&changed(
            "\"closed\": false",
            "\"closed\": false, \"colour\": \"red\""
        )),
        Err(ExportError::UnknownValue {
            path: "accounts[0].colour".into()
        })
    );
    assert_eq!(
        read_book(&changed(
            "\"format\": \"kassensturz-book\",",
            "\"format\": \"kassensturz-book\", \"notes\": [],"
        )),
        Err(ExportError::UnknownValue {
            path: "notes".into()
        })
    );
    assert_eq!(
        read_book(&changed(
            "\"currency\": \"EUR\", \"type\": \"checking\"",
            "\"currency\": \"EUR\", \"type\": \"checking\", \"currency\": \"USD\""
        )),
        // A key given twice: the last one counts, as with most JSON readers.
        read_book(&changed(
            "\"currency\": \"EUR\", \"type\": \"checking\"",
            "\"currency\": \"USD\", \"type\": \"checking\""
        ))
    );
}

#[test]
fn ids_are_unique_and_amounts_in_listed_currencies() {
    // Part IDs are unique across the whole book.
    assert_eq!(
        read_book(&changed("8000-00000000001f", "8000-000000000020")),
        Err(ExportError::Duplicate {
            path: "transactions[1].parts[0].id".into()
        })
    );
    assert_eq!(
        read_book(&changed(
            "8000-000000000002\", \"name\": \"Dollars",
            "8000-000000000001\", \"name\": \"Dollars"
        )),
        Err(ExportError::Duplicate {
            path: "accounts[1].id".into()
        })
    );
    assert_eq!(
        read_book(&changed(
            "{\"code\": \"JPY\", \"minor_digits\": 0}",
            "{\"code\": \"EUR\", \"minor_digits\": 0}"
        )),
        Err(ExportError::Duplicate {
            path: "currencies[1]".into()
        })
    );
    assert_eq!(
        read_book(&changed(
            "{\"code\": \"USD\", \"minor_digits\": 2}",
            "{\"code\": \"CHF\", \"minor_digits\": 2}"
        )),
        Err(ExportError::UnlistedCurrency {
            currency: USD,
            path: Some("transactions[2].parts[0].foreign.amount".into()),
        })
    );
}

#[test]
fn a_transaction_must_add_up() {
    assert_eq!(
        read_book(&changed("\"amount\": \"4.50\"", "\"amount\": \"4.60\"")),
        Err(ExportError::InvalidTransaction {
            path: "transactions[1]".into(),
            error: TransactionError::Unbalanced { total: eur(10) },
        })
    );
}

fn nested(depth: usize) -> Vec<RuleNode> {
    let mut book = sample();
    let mut node = book.rules.remove(1);
    for _ in 1..depth {
        node = RuleNode {
            rule: Rule {
                id: RuleId::generate(),
                ..node.rule.clone()
            },
            sub_rules: vec![node],
        };
    }
    vec![node]
}

#[test]
fn rules_nest_only_so_deep() {
    let mut book = sample();
    book.rules = nested(MAX_RULE_DEPTH);
    let file = written(book.clone());
    assert_eq!(read_book(file.as_bytes()), Ok(book.clone()));

    book.rules = nested(MAX_RULE_DEPTH + 1);
    let too_deep: Result<(), _> = Err(ExportError::RulesTooDeep {
        max: MAX_RULE_DEPTH,
    });
    assert_eq!(
        export(&BookExport, &ExportData::Book(book.clone())).map(|_| ()),
        too_deep.clone()
    );
    // Written by another program, it is refused when read.
    let deeper = file.replacen(
        "\"sub_rules\": []",
        &format!(
            "\"sub_rules\": [{{\"id\": \"{}\", \"name\": \"Deeper\", \"match\": \"all\", \"conditions\": [], \"sub_rules\": []}}]",
            RuleId::generate()
        ),
        1,
    );
    assert_eq!(read_book(deeper.as_bytes()).map(|_| ()), too_deep);
}

#[test]
fn data_that_would_not_read_back_is_not_written() {
    let mut book = sample();
    book.currencies.remove(2);
    assert_eq!(
        export(&BookExport, &ExportData::Book(book)),
        Err(ExportError::UnlistedCurrency {
            currency: USD,
            path: None
        })
    );
    let mut book = sample();
    book.rule_marks[0].split = SplitId::from_bytes(id(99));
    assert_eq!(
        export(&BookExport, &ExportData::Book(book)),
        Err(ExportError::MarkWithoutPart(SplitId::from_bytes(id(99))))
    );
    assert_eq!(
        export(&BookExport, &ExportData::Transactions(vec![])),
        Err(ExportError::WrongKind)
    );
    assert_eq!(
        export(&CsvExport::default(), &ExportData::Book(sample())),
        Err(ExportError::WrongKind)
    );
}

#[test]
fn a_file_is_written_and_read_back() {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("export-book");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("test folder");
    let path = folder.join("Household.json");
    export_file(&BookExport, &ExportData::Book(sample()), &path).expect("written");
    assert_eq!(read_book_file(&path), Ok(sample()));
    assert_eq!(
        export_file(&BookExport, &ExportData::Book(sample()), &path),
        Err(ExportError::AlreadyExists {
            file: path.display().to_string()
        })
    );
    assert_eq!(
        read_book_file(&folder.join("missing.json")),
        Err(ExportError::Io(std::io::ErrorKind::NotFound))
    );
}
