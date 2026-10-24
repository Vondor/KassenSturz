// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a KMyMoney book's payee recognition becomes in a book: Rules that
//! book what a bank delivers (D-160). The importer reads made-up files, the
//! pipeline fills the book, and the Rules run on entries as an import adds
//! them, so each step meets the real output of the one before. All data is
//! made up (AGENTS.md §3.1 rule 2).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_core::{
    AccountId, AccountKind, BookTo, Condition, CurrencyCode, Date, ImportedDetails, MatchMode,
    Money, Rule, Split, TextField, TextOperator, Transaction, TransactionId,
};
use kassensturz_import::{
    BookAccount, BookCondition, BookRule, Imported, ImportedBook, Kmy, KmySqlite, Limits,
    SkippedData, currency, import,
};
use kassensturz_pipeline::{
    BookImportReport, PatternLeftOut, RuleScope, fill_book_from, fill_book_from_with,
};
use kassensturz_store::Book;

fn golden(folder: &str, name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden")
        .join(folder)
        .join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn filled_with(imported: Imported, scope: RuleScope) -> (Book, BookImportReport) {
    let Imported::Book(imported) = imported else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from_with(&mut book, &imported, &HashMap::new(), scope).expect("filled");
    (book, report)
}

fn payee_rules_kmy_with(scope: RuleScope) -> (Book, BookImportReport) {
    filled_with(
        import(&Kmy, &golden("kmy", "payee-rules.kmy"), &Limits::default())
            .expect("the file reads"),
        scope,
    )
}

fn payee_rules_kmy() -> (Book, BookImportReport) {
    payee_rules_kmy_with(RuleScope::default())
}

fn account(book: &Book, name: &str) -> AccountId {
    book.all_accounts()
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == name)
        .unwrap_or_else(|| panic!("{name} is in the book"))
        .id
}

fn rule(book: &Book, name: &str) -> Rule {
    book.rules()
        .expect("Rules")
        .into_iter()
        .map(|node| node.rule)
        .find(|rule| rule.name == name)
        .unwrap_or_else(|| panic!("{name} is a Rule"))
}

/// An entry a bank delivered for `account`, as an import adds it (D-083):
/// the account's part with the other party's name, and a part without a
/// category yet.
fn delivered(book: &mut Book, account: AccountId, cents: i64, other_party: &str) -> Transaction {
    let day = Date::new(2026, 10, 5).expect("a date");
    let euro = kassensturz_core::CurrencyCode::new("EUR").expect("a code");
    let own = Split {
        imported: Some(ImportedDetails {
            other_party_name: Some(other_party.into()),
            ..ImportedDetails::default()
        }),
        ..Split::new(account, Money::from_minor_units(cents, euro))
    };
    let transaction = Transaction::new(
        TransactionId::generate(),
        day,
        None,
        vec![
            own,
            Split::uncategorized(Money::from_minor_units(-cents, euro)),
        ],
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction
}

/// The category the other side was booked to, and the payee, after the
/// Rules ran.
fn booked(
    book: &Book,
    transaction: &Transaction,
    own: AccountId,
) -> (Option<AccountId>, Option<String>) {
    let stored = book.transaction(transaction.id()).expect("read");
    let other = stored
        .splits()
        .iter()
        .find(|split| split.account != Some(own))
        .expect("two parts")
        .account;
    (other, stored.payee().map(str::to_owned))
}

#[test]
fn a_payee_with_a_default_category_becomes_a_rule() {
    let (book, report) = payee_rules_kmy();
    let names: Vec<String> = book
        .rules()
        .expect("Rules")
        .into_iter()
        .map(|node| node.rule.name)
        .collect();
    assert_eq!(
        names,
        [
            "Corner Grocery",
            "Hotel Example",
            "Pays Salary",
            "Only Default",
            "Only Unusable"
        ],
        "in the order of the payees"
    );
    assert_eq!(report.rules, 5);

    let grocery = rule(&book, "Corner Grocery");
    assert_eq!(
        grocery.actions.book_to,
        Some(BookTo::Category(account(&book, "Groceries")))
    );
    assert_eq!(grocery.actions.payee.as_deref(), Some("Corner Grocery"));
    assert_eq!(grocery.match_mode, MatchMode::Any);
    // Two of its three texts (one is a pattern KassenSturz can't use), then
    // the name itself, as the payee and as the other party.
    assert_eq!(grocery.conditions.len(), 4);
    for condition in &grocery.conditions[..2] {
        assert!(matches!(
            condition,
            Condition::Text {
                operator: TextOperator::Matches,
                case_sensitive: false,
                ..
            }
        ));
    }
    for (condition, wanted_field) in grocery.conditions[2..]
        .iter()
        .zip([TextField::Payee, TextField::OtherPartyName])
    {
        assert!(matches!(
            condition,
            Condition::Text {
                field,
                operator: TextOperator::Equals,
                case_sensitive: false,
                ..
            } if *field == wanted_field
        ));
    }

    // Recognizing by the name is "contains", and capitals count when the
    // file says so; the name itself is added.
    let hotel = rule(&book, "Hotel Example");
    assert_eq!(hotel.conditions.len(), 3);
    assert!(matches!(
        &hotel.conditions[0],
        Condition::Text {
            operator: TextOperator::Contains,
            case_sensitive: true,
            ..
        }
    ));

    // A payee that has only a default category is recognized by its name.
    let only_default = rule(&book, "Only Default");
    assert_eq!(only_default.conditions.len(), 2);
    assert_eq!(
        only_default.actions.book_to,
        Some(BookTo::Category(account(&book, "Groceries")))
    );
}

/// The "Advanced" choice: only payees the file has a matching text for.
#[test]
fn only_with_matching_text_leaves_out_what_the_file_cant_recognize() {
    let (book, report) = payee_rules_kmy_with(RuleScope::WithMatchingText);
    let names: Vec<String> = book
        .rules()
        .expect("Rules")
        .into_iter()
        .map(|node| node.rule.name)
        .collect();
    assert_eq!(names, ["Corner Grocery", "Hotel Example", "Pays Salary"]);
    assert_eq!(report.rules, 3);
    // The file's texts only: no name added.
    assert_eq!(rule(&book, "Corner Grocery").conditions.len(), 2);
    assert_eq!(rule(&book, "Corner Grocery").match_mode, MatchMode::Any);
    assert_eq!(rule(&book, "Hotel Example").conditions.len(), 1);
    assert_eq!(rule(&book, "Hotel Example").match_mode, MatchMode::All);
    // The two without a default category, and the one without a text.
    let skipped = report
        .skipped
        .iter()
        .find(|skipped| skipped.what == SkippedData::PayeeRules)
        .expect("counted");
    assert_eq!(skipped.count, 3);
    assert_eq!(
        report.patterns_left_out,
        vec![
            PatternLeftOut {
                payee: "Corner Grocery".into(),
                pattern: "^(?!x)Corner$".into(),
                rule_made: true,
            },
            PatternLeftOut {
                payee: "Only Unusable".into(),
                pattern: "^(?=a).*$".into(),
                rule_made: false,
            },
        ]
    );
}

#[test]
fn what_cant_become_part_of_a_rule_is_reported_not_dropped() {
    let (_, report) = payee_rules_kmy();
    // The name itself still recognizes "Only Unusable", so it has a Rule.
    assert_eq!(
        report.patterns_left_out,
        vec![
            PatternLeftOut {
                payee: "Corner Grocery".into(),
                pattern: "^(?!x)Corner$".into(),
                rule_made: true,
            },
            PatternLeftOut {
                payee: "Only Unusable".into(),
                pattern: "^(?=a).*$".into(),
                rule_made: true,
            },
        ]
    );
    // Matching texts without a default category: nothing to book to.
    let skipped = report
        .skipped
        .iter()
        .find(|skipped| skipped.what == SkippedData::PayeeRules)
        .expect("counted");
    assert_eq!(skipped.count, 2);
}

#[test]
fn the_rules_book_what_a_bank_delivers_as_kmymoney_recognized_it() {
    let (mut book, _) = payee_rules_kmy();
    let checking = account(&book, "Checking");
    let groceries = account(&book, "Groceries");
    let travel = account(&book, "Travel");
    let salary = account(&book, "Salary");

    let exact = delivered(&mut book, checking, -2_350, "corner grocery");
    let text = delivered(&mut book, checking, -990, "Corner/Grocery Market 12");
    let longer = delivered(&mut book, checking, -500, "Corner Grocery Berlin");
    let hotel = delivered(&mut book, checking, -12_000, "The Hotel Example Berlin");
    let lower = delivered(&mut book, checking, -8_000, "the hotel example berlin");
    let pay = delivered(&mut book, checking, 250_000, "PAYS SALARY");
    let named = delivered(&mut book, checking, -1_000, "only default");
    let named_longer = delivered(&mut book, checking, -1_000, "Only Default Shop");
    let run = book.run_rules().expect("Rules run");
    assert_eq!(run.changed.len(), 5);

    let grocery = Some("Corner Grocery".to_owned());
    // Anchored texts match the whole name and ignore capitals.
    assert_eq!(
        booked(&book, &exact, checking),
        (Some(groceries), grocery.clone())
    );
    assert_eq!(booked(&book, &text, checking), (Some(groceries), grocery));
    assert_eq!(booked(&book, &longer, checking), (None, None));
    // "Contains" with capitals counting.
    assert_eq!(
        booked(&book, &hotel, checking),
        (Some(travel), Some("Hotel Example".to_owned()))
    );
    assert_eq!(booked(&book, &lower, checking), (None, None));
    assert_eq!(
        booked(&book, &pay, checking),
        (Some(salary), Some("Pays Salary".to_owned()))
    );
    // A payee with only a default category: its name, whole, ignoring
    // capitals.
    assert_eq!(
        booked(&book, &named, checking),
        (Some(groceries), Some("Only Default".to_owned()))
    );
    assert_eq!(booked(&book, &named_longer, checking), (None, None));
}

#[test]
fn only_with_matching_text_the_name_alone_books_nothing() {
    let (mut book, _) = payee_rules_kmy_with(RuleScope::WithMatchingText);
    let checking = account(&book, "Checking");
    let groceries = account(&book, "Groceries");
    let named = delivered(&mut book, checking, -1_000, "only default");
    let text = delivered(&mut book, checking, -990, "Corner/Grocery Market 12");
    let run = book.run_rules().expect("Rules run");
    assert_eq!(run.changed.len(), 1);
    assert_eq!(booked(&book, &named, checking), (None, None));
    assert_eq!(
        booked(&book, &text, checking),
        (Some(groceries), Some("Corner Grocery".to_owned()))
    );
}

fn space_colony_v15(scope: RuleScope) -> (Book, BookImportReport) {
    filled_with(
        import(
            &KmySqlite,
            &golden("kmy-sqlite", "space-colony-v15.sqlite"),
            &Limits::default(),
        )
        .expect("the file reads"),
        scope,
    )
}

fn payee_rules_skipped(report: &BookImportReport) -> Option<usize> {
    report
        .skipped
        .iter()
        .find(|skipped| skipped.what == SkippedData::PayeeRules)
        .map(|skipped| skipped.count)
}

#[test]
fn a_default_account_of_the_user_makes_a_transfer_rule() {
    let (book, report) = space_colony_v15(RuleScope::default());
    // 26 payees: 24 have a default category the book has, one of them an
    // account of the user's, so 24 Rules are made, two of them recognizing
    // the payee by its name only. Two had no default category, or
    // something the importer doesn't know.
    assert_eq!(report.rules, 24);
    assert_eq!(payee_rules_skipped(&report), Some(2));
    let water = rule(&book, "Europa Water Board");
    assert_eq!(
        water.actions.book_to,
        Some(BookTo::Transfer(account(&book, "Everyday Checking")))
    );
    // Both of its texts and its name twice, and the one with a lookahead,
    // which isn't usable, has its name.
    assert_eq!(rule(&book, "Hydroponics Cooperative").conditions.len(), 4);
    assert_eq!(rule(&book, "Suit Tailors Guild").conditions.len(), 3);
    assert_eq!(rule(&book, "Rover Garage Gamma").conditions.len(), 2);
    assert_eq!(
        report.patterns_left_out,
        vec![
            PatternLeftOut {
                payee: "Suit Tailors Guild".into(),
                pattern: "^(?!Test).*Tailors$".into(),
                rule_made: true,
            },
            PatternLeftOut {
                payee: "Rover Garage Gamma".into(),
                pattern: "^(?=Rover).*Gamma$".into(),
                rule_made: true,
            },
        ]
    );
}

#[test]
fn only_with_matching_text_a_book_gets_the_rules_it_had_before() {
    let (book, report) = space_colony_v15(RuleScope::WithMatchingText);
    // 22 payees have a matching text and a default category; one has only
    // a pattern KassenSturz can't use. The other four have no text or no
    // default category.
    assert_eq!(report.rules, 21);
    assert_eq!(payee_rules_skipped(&report), Some(4));
    assert_eq!(rule(&book, "Hydroponics Cooperative").conditions.len(), 2);
    assert_eq!(rule(&book, "Suit Tailors Guild").conditions.len(), 1);
    assert_eq!(
        report.patterns_left_out,
        vec![
            PatternLeftOut {
                payee: "Suit Tailors Guild".into(),
                pattern: "^(?!Test).*Tailors$".into(),
                rule_made: true,
            },
            PatternLeftOut {
                payee: "Rover Garage Gamma".into(),
                pattern: "^(?=Rover).*Gamma$".into(),
                rule_made: false,
            },
        ]
    );
}

fn rule_for(name: &str, pattern: &str, book_to: &str) -> BookRule {
    BookRule {
        name: name.into(),
        match_mode: MatchMode::Any,
        conditions: vec![BookCondition {
            field: TextField::OtherPartyName,
            operator: TextOperator::Matches,
            value: pattern.into(),
        }],
        name_conditions: vec![BookCondition {
            field: TextField::OtherPartyName,
            operator: TextOperator::Equals,
            value: name.into(),
        }],
        case_sensitive: false,
        book_to: book_to.into(),
        payee: name.into(),
    }
}

fn two_accounts() -> ImportedBook {
    let euro = currency(CurrencyCode::new("EUR").expect("a code")).expect("EUR is known");
    let book_account = |key: &str, name: &str, kind| BookAccount {
        key: key.into(),
        name: name.into(),
        kind,
        account_type: None,
        currency: euro,
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    };
    ImportedBook {
        accounts: vec![
            book_account("A1", "Checking", AccountKind::Asset),
            book_account("E1", "Food", AccountKind::Expense),
        ],
        ..ImportedBook::default()
    }
}

#[test]
fn rules_of_a_handed_over_book_get_names_that_differ_and_a_known_account() {
    let mut imported = two_accounts();
    imported.rules = vec![
        rule_for("Shop", "^shop$", "E1"),
        rule_for("shop", "^other$", "A1"),
    ];
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    assert_eq!(report.rules, 2);
    let rules = book.rules().expect("Rules");
    assert_eq!(
        rules
            .iter()
            .map(|node| node.rule.name.as_str())
            .collect::<Vec<_>>(),
        ["Shop", "shop (2)"]
    );
    let food = account(&book, "Food");
    let checking = account(&book, "Checking");
    assert_eq!(rules[0].rule.actions.book_to, Some(BookTo::Category(food)));
    assert_eq!(
        rules[1].rule.actions.book_to,
        Some(BookTo::Transfer(checking))
    );

    // A key no account has is a broken book, not a guess.
    imported.rules = vec![rule_for("Shop", "^shop$", "nowhere")];
    let mut book = Book::create_in_memory().expect("book created");
    assert!(fill_book_from(&mut book, &imported, &HashMap::new()).is_err());
    assert_eq!(book.rules().expect("Rules"), Vec::new());
}

/// KMyMoney writes a name as a pattern by escaping everything that isn't a
/// letter, a digit or an underscore; the `regex` crate takes all of those
/// escapes, so a name KMyMoney wrote in full is never left out (D-160).
#[test]
fn every_escaped_punctuation_mark_is_usable() {
    let escaped: String = (0x21_u8..0x7f)
        .map(char::from)
        .filter(|c| !c.is_ascii_alphanumeric() && *c != '_')
        .map(|c| format!("\\{c}"))
        .collect();
    let mut imported = two_accounts();
    imported.rules = vec![rule_for("Shop", &format!("^a\\ {escaped}b$"), "E1")];
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    assert_eq!(report.rules, 1);
    assert_eq!(report.patterns_left_out, Vec::new());
}
