// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AI breaks a transaction into parts (US5.1, D-737,
//! `docs/AI_SPLITS.md` §1): `split_transactions` behind its own cell, the
//! parts adding up to the account's amount exactly, the account's own part
//! never changing, one Undo, and the parts in `search_transactions`.
//!
//! The seam (AGENTS.md §9.4, X3): a transaction from every importer's real
//! output is split and undone, the KMyMoney book's own entries as the
//! pipeline fills them and the statements of CAMT.053, MT940, OFX and CSV as
//! `import_statements` books them. Synthetic data only.

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use kassensturz_ai_tools::{Confirmed, ProposedChange, ToolError, call, propose};
use kassensturz_core::ai_switch::{Field, Permission, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Report as _, Split,
    Tag, TagId, Transaction, TransactionId,
};
use kassensturz_import::{
    Camt053, Csv, CsvSettings, Imported, Importer, KmySqlite, Limits, Mt940, Ofx, currency, import,
};
use kassensturz_pipeline::{Choices, fill_book_from, import_statements};
use kassensturz_store::{Action, Book};
use proptest::prelude::*;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 11, 12);
const DAY: Date = Date::constant(2026, 11, 3);

fn eur() -> CurrencyCode {
    CurrencyCode::new("EUR").unwrap()
}

fn money(minor: i64) -> Money {
    Money::from_minor_units(minor, eur())
}

/// Every cell on and no field locked: a split needs the Category and the
/// Amount unlocked, and a name the Note.
fn unlocked() -> Permissions {
    Field::ALL
        .into_iter()
        .fold(Permissions::all(), |permissions, field| {
            permissions.with_lock(field, false)
        })
}

fn account(name: &str, kind: AccountKind, currency: CurrencyCode) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        kind,
        currency,
        account_type: kind.is_account().then_some(AccountType::Checking),
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

struct Fixture {
    book: Book,
    checking: AccountId,
    office: AccountId,
    food: AccountId,
    /// Checking pays 19.99 to a shop, not sorted yet.
    debit: TransactionId,
}

fn fixture() -> Fixture {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(eur()).unwrap();
    let checking = account("Everyday Checking", AccountKind::Asset, eur());
    let savings = account("Rainy Day Savings", AccountKind::Asset, eur());
    let office = account("Office supplies", AccountKind::Expense, eur());
    let food = account("Food", AccountKind::Expense, eur());
    for account in [&checking, &savings, &office, &food] {
        book.add_account(account).unwrap();
    }
    let debit = TransactionId::generate();
    book.add_transaction(
        &Transaction::new(
            debit,
            DAY,
            Some("Papierladen Nord".into()),
            vec![
                Split::new(checking.id, money(-1_999)),
                Split::uncategorized(money(1_999)),
            ],
        )
        .unwrap(),
    )
    .unwrap();
    Fixture {
        book,
        checking: checking.id,
        office: office.id,
        food: food.id,
        debit,
    }
}

fn split_call(transaction: TransactionId, parts: Value) -> Value {
    json!({"split_transactions": [{"transaction_id": transaction.to_string(), "parts": parts}]})
}

/// The code of a refusal, without the item it came from.
fn code(result: Result<impl Sized, ToolError>) -> &'static str {
    match result {
        Ok(_) => "ok",
        Err(error) => error.kind().report().code,
    }
}

fn own_part(transaction: &Transaction, account: AccountId) -> &Split {
    transaction
        .splits()
        .iter()
        .find(|split| split.account == Some(account))
        .expect("the account's part")
}

/// Splits `transaction` into two parts, the first with `category`, the
/// second not sorted yet, applies it, checks what changed, and undoes it.
fn split_apply_undo(
    book: &mut Book,
    transaction: TransactionId,
    account: AccountId,
    category: AccountId,
) {
    let before = book.transaction(transaction).unwrap();
    let own = own_part(&before, account).clone();
    let digits = book.currency_minor_digits(own.amount.currency()).unwrap();
    let total = own.amount.minor_units();
    // One cent, or one unit, to the second part: money out stays negative.
    let unit = if total < 0 { -1 } else { 1 };
    let text = |minor: i64| {
        let scale = 10_i64.pow(u32::from(digits));
        let sign = if minor < 0 { "-" } else { "" };
        let abs = minor.unsigned_abs();
        if digits == 0 {
            format!("{sign}{abs}")
        } else {
            format!(
                "{sign}{}.{:0width$}",
                abs / scale.unsigned_abs(),
                abs % scale.unsigned_abs(),
                width = usize::from(digits)
            )
        }
    };
    let call = split_call(
        transaction,
        json!([
            {"category_id": category.to_string(), "amount": text(total - unit), "name": "Invoice RE-0042"},
            {"amount": text(unit)}
        ]),
    );
    let proposal = propose(book, &call, unlocked()).expect("prepared");
    let history = book.history().unwrap().len();
    proposal
        .changes
        .apply(book, Confirmed::by_the_user())
        .expect("applied");
    let after = book.transaction(transaction).unwrap();
    assert_eq!(own_part(&after, account), &own, "the account's part stays");
    assert_eq!(after.splits().len(), 3);
    let named = after
        .splits()
        .iter()
        .find(|split| split.account == Some(category))
        .expect("the category's part");
    assert_eq!(named.note.as_deref(), Some("Invoice RE-0042"));
    assert_eq!(named.amount.minor_units(), -(total - unit));
    assert!(
        after
            .splits()
            .iter()
            .any(|split| split.account.is_none() && split.amount.minor_units() == -unit)
    );
    assert_eq!(after.note(), before.note());
    assert_eq!(after.tags(), before.tags());
    let entries = book.history().unwrap();
    assert_eq!(entries.len(), history + 1, "one entry for the whole set");
    assert_eq!(entries[0].action, Action::ApplyAiChanges);
    book.undo().unwrap().expect("a change");
    assert_eq!(
        book.transaction(transaction).unwrap(),
        before,
        "Undo takes it all back"
    );
}

#[test]
fn splitting_is_off_until_its_own_cell_is_on() {
    let f = fixture();
    let call = split_call(f.debit, json!([{"amount": "-19.99"}, {"amount": "0.00"}]));
    let off = unlocked().with(Permission::SplitTransactions, false);
    assert_eq!(code(propose(&f.book, &call, off)), "ai-tools.not-allowed");
    assert!(!Permissions::defaults().allows(Permission::SplitTransactions));
    assert!(Permission::SplitTransactions.needs_read());
}

#[test]
fn a_locked_category_or_amount_leaves_splitting_out_and_a_locked_note_the_names() {
    let f = fixture();
    let call = split_call(
        f.debit,
        json!([
            {"category_id": f.office.to_string(), "amount": "-12.50"},
            {"amount": "-7.49"}
        ]),
    );
    for field in [Field::Category, Field::Amount] {
        let locked = unlocked().with_lock(field, true);
        assert_eq!(
            code(propose(&f.book, &call, locked)),
            "ai-tools.not-allowed"
        );
        let offered = kassensturz_ai_tools::propose_spec(locked).expect("other lists");
        assert!(!offered.parameters.contains("split_transactions"));
    }
    let no_notes = unlocked().with_lock(Field::Note, true);
    assert!(propose(&f.book, &call, no_notes).is_ok());
    let named = split_call(
        f.debit,
        json!([{"amount": "-19.99", "name": "Invoice RE-0042"}]),
    );
    assert_eq!(
        code(propose(&f.book, &named, no_notes)),
        "ai-tools.field-locked"
    );
    let offered = kassensturz_ai_tools::propose_spec(no_notes).expect("offered");
    let parameters: Value = serde_json::from_str(&offered.parameters).unwrap();
    let part = &parameters["properties"]["split_transactions"]["items"]["properties"]["parts"]["items"]
        ["properties"];
    assert!(part.get("amount").is_some());
    assert!(
        part.get("name").is_none(),
        "a locked note leaves the names out"
    );
}

#[test]
fn a_split_is_reviewed_applied_as_one_undo_and_leaves_the_bank_total() {
    let mut f = fixture();
    let call = split_call(
        f.debit,
        json!([
            {"category_id": f.office.to_string(), "amount": "-12.50", "name": "  Invoice RE-0042 printer paper "},
            {"amount": "-7.49", "name": "Invoice RE-0043"}
        ]),
    );
    let proposal = propose(&f.book, &call, unlocked()).expect("prepared");
    let [ProposedChange::SplitTransaction(change)] = proposal.changes.changes() else {
        panic!("one split: {:?}", proposal.changes.changes());
    };
    assert_eq!(change.total, money(-1_999));
    assert_eq!(change.account, "Everyday Checking");
    assert_eq!(change.payee.as_deref(), Some("Papierladen Nord"));
    assert_eq!(change.date, DAY);
    assert_eq!(change.before.len(), 1);
    assert_eq!(change.before[0].category, None);
    assert_eq!(change.before[0].amount, money(-1_999));
    assert_eq!(change.parts.len(), 2);
    assert_eq!(
        change.parts[0].category,
        Some(vec!["Office supplies".to_owned()])
    );
    assert_eq!(change.parts[0].amount, money(-1_250));
    assert_eq!(
        change.parts[0].name.as_deref(),
        Some("Invoice RE-0042 printer paper")
    );
    assert_eq!(change.parts[1].category, None);
    // Nothing changes before the user confirms.
    let stored = f.book.transaction(f.debit).unwrap();
    assert_eq!(stored.splits().len(), 2);
    split_apply_undo(&mut f.book, f.debit, f.checking, f.office);
}

#[test]
fn parts_that_dont_add_up_are_refused_with_the_difference_and_nothing_is_filled_in() {
    let f = fixture();
    let call = split_call(
        f.debit,
        json!([
            {"category_id": f.office.to_string(), "amount": "-12.50"},
            {"amount": "-7.48"}
        ]),
    );
    let error = propose(&f.book, &call, unlocked()).expect_err("a cent short");
    assert_eq!(error.kind().report().code, "ai-tools.parts-dont-add-up");
    assert_eq!(
        error.amounts(),
        Some([
            ("total", "-19.99"),
            ("sum", "-19.98"),
            ("difference", "-0.01")
        ])
    );
    // Money in instead of out is refused too: the sign is the account's.
    let flipped = split_call(f.debit, json!([{"amount": "19.99"}]));
    let error = propose(&f.book, &flipped, unlocked()).unwrap_err();
    assert_eq!(error.report().code, "ai-tools.in-item", "named by its item");
    assert_eq!(error.kind().report().code, "ai-tools.parts-dont-add-up");
}

#[test]
fn one_part_joins_a_split_back_into_one() {
    let mut f = fixture();
    let call = split_call(
        f.debit,
        json!([
            {"category_id": f.office.to_string(), "amount": "-12.50"},
            {"category_id": f.food.to_string(), "amount": "-7.49"}
        ]),
    );
    propose(&f.book, &call, unlocked())
        .unwrap()
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .unwrap();
    assert_eq!(f.book.transaction(f.debit).unwrap().splits().len(), 3);
    let join = split_call(
        f.debit,
        json!([{"category_id": f.food.to_string(), "amount": "-19.99"}]),
    );
    propose(&f.book, &join, unlocked())
        .unwrap()
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .unwrap();
    let joined = f.book.transaction(f.debit).unwrap();
    assert_eq!(joined.splits().len(), 2);
    assert_eq!(own_part(&joined, f.food).amount, money(1_999));
    // The same parts again change nothing, so they are refused.
    assert_eq!(
        code(propose(&f.book, &join, unlocked())),
        "ai-tools.bad-argument"
    );
}

#[test]
fn a_split_is_out_of_date_when_the_entry_changed_in_between() {
    let mut f = fixture();
    let call = split_call(
        f.debit,
        json!([{"category_id": f.office.to_string(), "amount": "-19.99"}]),
    );
    let first = propose(&f.book, &call, unlocked()).unwrap();
    let second = propose(&f.book, &call, unlocked()).unwrap();
    first
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .unwrap();
    let error = second
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .expect_err("out of date");
    assert_eq!(error.report().code, "ai-changes.out-of-date");
}

#[test]
fn what_the_model_cannot_split_is_refused_and_nothing_is_prepared() {
    let mut f = fixture();
    let accounts = f.book.all_accounts().unwrap();
    let savings = accounts
        .iter()
        .find(|a| a.name == "Rainy Day Savings")
        .unwrap()
        .id;
    let usd = CurrencyCode::new("USD").unwrap();
    f.book.use_currency(usd).unwrap();
    let dollars = account("Space dollars", AccountKind::Expense, usd);
    f.book.add_account(&dollars).unwrap();
    let tag = TagId::generate();
    f.book
        .add_tag(&Tag {
            id: tag,
            name: "Tax".into(),
        })
        .unwrap();
    let add = |book: &mut Book, splits: Vec<Split>| {
        let id = TransactionId::generate();
        book.add_transaction(&Transaction::new(id, DAY, None, splits).unwrap())
            .unwrap();
        id
    };
    let transfer = add(
        &mut f.book,
        vec![
            Split::new(f.checking, money(-5_000)),
            Split::new(savings, money(5_000)),
        ],
    );
    let tagged = add(
        &mut f.book,
        vec![
            Split::new(f.checking, money(-3_000)),
            Split {
                tags: vec![tag],
                ..Split::new(f.office, money(3_000))
            },
        ],
    );
    let numbered = add(
        &mut f.book,
        vec![
            Split::new(f.checking, money(-3_000)),
            Split {
                number: Some("1042".into()),
                ..Split::new(f.office, money(3_000))
            },
        ],
    );
    let refused = |book: &Book, call: Value| {
        let error = propose(book, &call, unlocked()).expect_err("refused");
        let report = error.kind().report();
        let reason = report.params.iter().find_map(|(name, value)| match value {
            kassensturz_core::ErrorParam::Keyword(word) if *name == "reason" => Some(*word),
            _ => None,
        });
        (report.code, reason)
    };
    let one = |id: TransactionId, amount: &str| {
        split_call(
            id,
            json!([{"category_id": f.office.to_string(), "amount": amount}]),
        )
    };
    assert_eq!(
        refused(&f.book, one(transfer, "-50.00")),
        ("ai-tools.not-editable", Some("transfer"))
    );
    assert_eq!(
        refused(&f.book, one(tagged, "-30.00")),
        ("ai-tools.not-editable", Some("part-details"))
    );
    assert_eq!(
        refused(&f.book, one(numbered, "-30.00")),
        ("ai-tools.not-editable", Some("part-details"))
    );
    let cases = [
        // More decimal places than the currency has, and zero.
        (one(f.debit, "-19.990"), "ai-tools.bad-argument"),
        (
            split_call(f.debit, json!([{"amount": "-19.99"}, {"amount": "0"}])),
            "ai-tools.bad-argument",
        ),
        // No parts at all.
        (split_call(f.debit, json!([])), "ai-tools.bad-argument"),
        // A category that isn't one, or in another currency.
        (
            split_call(
                f.debit,
                json!([{"category_id": savings.to_string(), "amount": "-19.99"}]),
            ),
            "ai-tools.unknown-category",
        ),
        (
            split_call(
                f.debit,
                json!([{"category_id": dollars.id.to_string(), "amount": "-19.99"}]),
            ),
            "ai-tools.other-currency",
        ),
        // A key a part doesn't take.
        (
            split_call(f.debit, json!([{"amount": "-19.99", "rest": true}])),
            "ai-tools.unknown-field",
        ),
        // A name on two lines.
        (
            split_call(f.debit, json!([{"amount": "-19.99", "name": "a\nb"}])),
            "ai-tools.bad-argument",
        ),
    ];
    for (call, expected) in cases {
        let error = propose(&f.book, &call, unlocked()).expect_err("refused");
        assert_eq!(error.report().code, "ai-tools.in-item", "{call}");
        assert_eq!(error.kind().report().code, expected, "{call}");
    }
    let many: Vec<Value> = (0..51)
        .map(|n| json!({"amount": if n == 0 { "-19.49" } else { "-0.01" }}))
        .collect();
    assert_eq!(
        refused(&f.book, split_call(f.debit, Value::Array(many))),
        ("ai-tools.not-changeable", Some("too-many-parts"))
    );
    // The same transaction twice in a set, or also tagged in it.
    let twice = json!({"split_transactions": [
        {"transaction_id": f.debit.to_string(), "parts": [{"amount": "-19.99", "category_id": f.office.to_string()}]},
        {"transaction_id": f.debit.to_string(), "parts": [{"amount": "-19.99", "category_id": f.food.to_string()}]}
    ]});
    assert_eq!(
        refused(&f.book, twice),
        ("ai-tools.not-changeable", Some("also-changed"))
    );
    // The book is as it was.
    assert_eq!(f.book.transaction(f.debit).unwrap().splits().len(), 2);
}

#[test]
fn the_search_shows_the_parts_and_finds_a_split_by_one_part() {
    let mut f = fixture();
    let split = split_call(
        f.debit,
        json!([
            {"category_id": f.office.to_string(), "amount": "-12.50", "name": "Invoice RE-0042"},
            {"amount": "-7.49", "name": "Invoice RE-0043"}
        ]),
    );
    propose(&f.book, &split, unlocked())
        .unwrap()
        .changes
        .apply(&mut f.book, Confirmed::by_the_user())
        .unwrap();
    let rows = |arguments: Value| {
        call(&f.book, "search_transactions", &arguments, TODAY)
            .unwrap()
            .value["rows"]
            .as_array()
            .unwrap()
            .clone()
    };
    let all = rows(json!({}));
    assert_eq!(all.len(), 1);
    let parts = all[0]["parts"].as_array().expect("parts");
    assert_eq!(
        parts,
        &[
            json!({"category": "Office supplies", "category_id": f.office.to_string(), "amount": "-12.50", "name": "Invoice RE-0042"}),
            json!({"unsorted": true, "amount": "-7.49", "name": "Invoice RE-0043"}),
        ]
    );
    assert!(all[0].get("matched_part").is_none());
    // By one part's amount: the row names the part.
    let by_part = rows(json!({"min_amount": "7.49", "max_amount": "7.49"}));
    assert_eq!(by_part.len(), 1);
    assert_eq!(by_part[0]["matched_part"], 2);
    assert_eq!(by_part[0]["amount"], "-19.99");
    // By the bank total: no part named.
    let by_total = rows(json!({"min_amount": "19.99", "max_amount": "19.99"}));
    assert_eq!(by_total.len(), 1);
    assert!(by_total[0].get("matched_part").is_none());
    // By a part's name.
    assert_eq!(rows(json!({"text": "re-0043"})).len(), 1);
    // An amount no part has finds nothing.
    assert!(rows(json!({"min_amount": "8", "max_amount": "9"})).is_empty());
}

// --- The seam: every importer's real output ---------------------------

fn import_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kassensturz-import/tests")
}

/// A fresh book with a checking account and a category, the statements of
/// `path` read by `importer` imported into the account as the app does.
fn statement_book(importer: &dyn Importer, path: &Path) -> (Book, AccountId, AccountId) {
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(eur()).unwrap();
    let checking = account("Everyday Checking", AccountKind::Asset, eur());
    let office = account("Office supplies", AccountKind::Expense, eur());
    book.add_account(&checking).unwrap();
    book.add_account(&office).unwrap();
    let bytes = fs::read(path).unwrap_or_else(|_| panic!("{}", path.display()));
    let Imported::Statements(statements) =
        import(importer, &bytes, &Limits::default()).expect("the statement reads")
    else {
        panic!("statements");
    };
    import_statements(
        &mut book,
        checking.id,
        &statements,
        &Choices::default(),
        None,
    )
    .expect("imported");
    (book, checking.id, office.id)
}

/// Splits the first entry of `book` the bank delivered, then undoes it.
fn split_first_delivered(mut book: Book, checking: AccountId, office: AccountId, what: &str) {
    let transaction = book
        .transactions()
        .unwrap()
        .into_iter()
        .find(|t| own_part(t, checking).imported.is_some() && t.splits().len() == 2)
        .unwrap_or_else(|| panic!("{what}: a delivered entry"));
    split_apply_undo(&mut book, transaction.id(), checking, office);
}

#[test]
fn a_camt053_entry_splits_and_undoes() {
    let path = import_dir().join("books/examples/statements/space-colony.camt053.xml");
    let (book, checking, office) = statement_book(&Camt053, &path);
    split_first_delivered(book, checking, office, "CAMT.053");
}

#[test]
fn an_mt940_entry_splits_and_undoes() {
    let path = import_dir().join("golden/mt940/german-bank.sta");
    let (book, checking, office) = statement_book(&Mt940, &path);
    split_first_delivered(book, checking, office, "MT940");
}

#[test]
fn an_ofx_entry_splits_and_undoes() {
    let path = import_dir().join("golden/ofx/german-bank-sgml.ofx");
    let (book, checking, office) = statement_book(&Ofx, &path);
    split_first_delivered(book, checking, office, "OFX");
}

#[test]
fn a_csv_entry_splits_and_undoes() {
    let dir = import_dir().join("books/examples/statements");
    let settings = fs::read_to_string(dir.join("space-colony.csv.settings")).unwrap();
    let csv = Csv::new(
        CsvSettings::from_text(&settings).expect("settings"),
        currency(eur()).expect("a current currency"),
    )
    .expect("the importer");
    let (book, checking, office) = statement_book(&csv, &dir.join("space-colony.csv"));
    split_first_delivered(book, checking, office, "CSV");
}

#[test]
fn a_kmymoney_entry_splits_and_undoes() {
    let path = import_dir().join("golden/kmy-sqlite/space-colony-v15.sqlite");
    let bytes = fs::read(&path).unwrap();
    let Imported::Book(imported) = import(&KmySqlite, &bytes, &Limits::default()).unwrap() else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().unwrap();
    fill_book_from(&mut book, &imported, &HashMap::new()).unwrap();
    let accounts = book.all_accounts().unwrap();
    let by_id: HashMap<AccountId, &Account> = accounts.iter().map(|a| (a.id, a)).collect();
    // A plain entry of an account in its own currency: one category part
    // without details.
    let (transaction, own, category) = book
        .transactions()
        .unwrap()
        .into_iter()
        .find_map(|t| {
            let [a, b] = t.splits() else { return None };
            let kind = |s: &Split| s.account.and_then(|id| by_id.get(&id)).map(|a| a.kind);
            let (own, other) = match (kind(a), kind(b)) {
                (Some(k), Some(o)) if k.is_account() && o.is_category() => (a, b),
                (Some(o), Some(k)) if k.is_account() && o.is_category() => (b, a),
                _ => return None,
            };
            let plain = own.foreign.is_none()
                && other.foreign.is_none()
                && other.tags.is_empty()
                && other.number.is_none()
                && other.status.is_none()
                && by_id[&own.account?].currency == t.currency()
                && !by_id[&own.account?].closed
                && !by_id[&other.account?].closed;
            plain.then(|| (t.id(), own.account.unwrap(), other.account.unwrap()))
        })
        .expect("a plain entry in the KMyMoney book");
    split_apply_undo(&mut book, transaction, own, category);
}

// --- Money: any accepted split balances ------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Parts that add up are taken, and the account's amount never moves;
    /// parts one cent off are refused.
    #[test]
    fn a_split_is_taken_exactly_when_its_parts_add_up(
        cuts in proptest::collection::vec(1_i64..5_000, 1..8),
        off in prop_oneof![Just(0_i64), Just(1), Just(-1)],
    ) {
        let mut book = Book::create_in_memory().unwrap();
        book.use_currency(eur()).unwrap();
        let checking = account("Everyday Checking", AccountKind::Asset, eur());
        let office = account("Office supplies", AccountKind::Expense, eur());
        book.add_account(&checking).unwrap();
        book.add_account(&office).unwrap();
        let total: i64 = cuts.iter().sum();
        let id = TransactionId::generate();
        book.add_transaction(&Transaction::new(id, DAY, None, vec![
            Split::new(checking.id, money(-total)),
            Split::uncategorized(money(total)),
        ]).unwrap()).unwrap();
        let mut amounts: Vec<i64> = cuts.iter().map(|cut| -cut).collect();
        amounts[0] += off;
        prop_assume!(amounts[0] != 0);
        let parts: Vec<Value> = amounts
            .iter()
            .map(|minor| {
                let sign = if *minor < 0 { "-" } else { "" };
                let abs = minor.unsigned_abs();
                json!({"category_id": office.id.to_string(), "amount": format!("{sign}{}.{:02}", abs / 100, abs % 100)})
            })
            .collect();
        let call = split_call(id, Value::Array(parts));
        let result = propose(&book, &call, unlocked());
        if off != 0 {
            prop_assert_eq!(code(result), "ai-tools.parts-dont-add-up");
            return Ok(());
        }
        let proposal = match result {
            Ok(proposal) => proposal,
            // The same parts as before (one part equal to the old one).
            Err(_) if amounts.len() == 1 => return Ok(()),
            Err(error) => panic!("{error:?}"),
        };
        proposal.changes.apply(&mut book, Confirmed::by_the_user()).unwrap();
        let after = book.transaction(id).unwrap();
        prop_assert_eq!(own_part(&after, checking.id).amount, money(-total));
        let sum: i64 = after.splits().iter().map(|s| s.amount.minor_units()).sum();
        prop_assert_eq!(sum, 0);
        prop_assert_eq!(after.splits().len(), amounts.len() + 1);
    }
}
