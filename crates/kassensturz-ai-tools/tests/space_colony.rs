// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The read functions on the synthetic Space Colony book, as the KMyMoney
//! importer, the pipeline and the store really fill it (AGENTS.md §9.4:
//! tests where steps meet). The figures are held against what the store
//! itself says: balances and registers.

#![cfg(feature = "ai")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kassensturz_ai_tools::{FUNCTIONS, Figures, ToolError, call, check_numbers, overview};
use kassensturz_core::{AccountKind, Date, Money};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_store::Book;
use serde_json::{Value, json};

fn space_colony() -> Book {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/golden/kmy-sqlite/space-colony-v15.sqlite");
    let bytes = fs::read(&path).expect("the Space Colony book is in the repository");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    book
}

const TODAY: Date = Date::constant(2026, 10, 25);

fn eur(text: &Value) -> Money {
    let text = text.as_str().expect("an amount is text");
    let (sign, digits) = text
        .strip_prefix('-')
        .map_or((1, text), |digits| (-1, digits));
    let (whole, cents) = digits.split_once('.').expect("two decimal places");
    assert_eq!(cents.len(), 2, "{text}");
    let units: i64 = format!("{whole}{cents}").parse().expect("digits");
    Money::from_minor_units(
        sign * units,
        kassensturz_core::CurrencyCode::new("EUR").expect("EUR"),
    )
}

fn run(book: &Book, name: &str, arguments: Value) -> (Value, Figures) {
    let output =
        call(book, name, &arguments, TODAY).unwrap_or_else(|error| panic!("{name}: {error}"));
    (output.value, output.figures)
}

#[test]
fn the_balances_are_the_stores() {
    let book = space_colony();
    let (value, _) = run(&book, "list_accounts", json!({ "include_closed": true }));
    let accounts = value["accounts"].as_array().expect("accounts");
    assert_eq!(accounts.len(), book.accounts(true).expect("accounts").len());
    let mut together = Money::zero(kassensturz_core::CurrencyCode::new("EUR").expect("EUR"));
    for (row, account) in accounts.iter().zip(book.accounts(true).expect("accounts")) {
        assert_eq!(row["name"], account.name.as_str());
        assert_eq!(row["id"], account.id.to_string().as_str());
        // Every row says what kind of account it is; none leaves it out.
        assert!(row["type"].is_string(), "{}", account.name);
        let balance = book.balance(account.id).expect("a balance");
        assert_eq!(eur(&row["balance"]), balance, "{}", account.name);
        together = together.checked_add(balance).expect("no overflow");
        assert_eq!(
            row["debt"].as_bool().unwrap_or(false),
            account.kind == AccountKind::Liability
        );
    }
    assert_eq!(value["together"][0]["currency"], "EUR");
    assert_eq!(eur(&value["together"][0]["together"]), together);

    // A balance on a day is the store's, by the bank's booking day.
    let checking = accounts
        .iter()
        .find(|row| row["name"] == "Everyday Checking")
        .expect("Everyday Checking");
    let id = checking["id"].as_str().expect("an ID");
    let (value, _) = run(
        &book,
        "account_balance",
        json!({ "account_id": id, "date": "2025-12-31" }),
    );
    let account = book
        .accounts(false)
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == "Everyday Checking")
        .expect("the account");
    assert_eq!(
        eur(&value["balance"]),
        book.balance_on(account.id, Date::constant(2025, 12, 31))
            .expect("a balance")
    );
    assert_eq!(value["date"], "2025-12-31");
}

#[test]
fn the_totals_of_the_whole_book_add_up_to_the_balances() {
    // Every transaction balances: what the user's accounts hold is what came
    // in minus what went out, plus the starting balances, which count as
    // neither (D-089). Transfers between own accounts cancel out.
    let book = space_colony();
    let (first, last) = book
        .booking_date_range()
        .expect("readable")
        .expect("transactions");
    let (value, _) = run(
        &book,
        "category_totals",
        json!({ "from": first.to_string(), "to": last.to_string() }),
    );
    let totals = &value["currencies"][0];
    assert_eq!(value["currencies"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        eur(&totals["left"]),
        eur(&totals["money_in"])
            .checked_sub(eur(&totals["money_out"]))
            .expect("no overflow")
    );
    let sum = |side: &str| {
        totals[side]
            .as_array()
            .expect("a list")
            .iter()
            .map(|row| eur(&row["amount"]))
            .fold(
                Money::zero(eur(&json!("0.00")).currency()),
                |sum, amount| sum.checked_add(amount).expect("no overflow"),
            )
    };
    assert_eq!(sum("income"), eur(&totals["money_in"]));
    assert_eq!(sum("spending"), eur(&totals["money_out"]));

    let kinds: HashMap<_, _> = book
        .all_accounts()
        .expect("accounts")
        .into_iter()
        .map(|account| (account.id, account.kind))
        .collect();
    let mut held = Money::zero(eur(&json!("0.00")).currency());
    let mut starting = Money::zero(held.currency());
    for transaction in book.transactions().expect("transactions") {
        for split in transaction.splits() {
            match split.account.map(|id| kinds[&id]) {
                Some(AccountKind::Asset | AccountKind::Liability) => {
                    held = held.checked_add(split.amount).expect("no overflow");
                }
                Some(AccountKind::Equity) => {
                    starting = starting.checked_sub(split.amount).expect("no overflow");
                }
                _ => {}
            }
        }
    }
    assert_eq!(
        eur(&totals["left"])
            .checked_add(starting)
            .expect("no overflow"),
        held
    );
}

#[test]
fn a_search_of_one_account_lists_its_register() {
    let book = space_colony();
    let account = book
        .accounts(false)
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == "Everyday Checking")
        .expect("the account");
    let (from, to) = (Date::constant(2026, 9, 1), Date::constant(2026, 9, 10));
    let (value, _) = run(
        &book,
        "search_transactions",
        json!({
            "account_id": account.id.to_string(),
            "from": from.to_string(),
            "to": to.to_string(),
            "limit": 50,
        }),
    );
    // The register, as the app shows it, newest first.
    let mut register: Vec<_> = book
        .register(account.id)
        .expect("the register")
        .into_iter()
        .filter(|entry| from <= entry.booking_date && entry.booking_date <= to)
        .collect();
    register.sort_by_key(|entry| core::cmp::Reverse(entry.booking_date));
    let rows = value["rows"].as_array().expect("rows");
    assert!(!rows.is_empty(), "the days have entries");
    assert!(rows.len() < 50, "the days fit one answer");
    assert_eq!(value["matches"], rows.len());
    assert_eq!(rows.len(), register.len());
    let mut by_day: Vec<(String, Money)> = rows
        .iter()
        .map(|row| {
            (
                row["date"].as_str().expect("a day").to_owned(),
                eur(&row["amount"]),
            )
        })
        .collect();
    let mut expected: Vec<(String, Money)> = register
        .iter()
        .map(|entry| (entry.booking_date.to_string(), entry.amount))
        .collect();
    // Entries of one day may come in either order.
    by_day.sort_by_key(|(day, amount)| (day.clone(), amount.minor_units()));
    expected.sort_by_key(|(day, amount)| (day.clone(), amount.minor_units()));
    assert_eq!(by_day, expected);
    for row in rows {
        assert_eq!(row["account"], "Everyday Checking");
    }
}

#[test]
fn a_search_finds_text_case_blind_and_stops_at_its_limit() {
    let book = space_colony();
    let (value, _) = run(
        &book,
        "search_transactions",
        json!({ "text": "NORTHERN ICE", "limit": 3 }),
    );
    let rows = value["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 3);
    assert!(value["matches"].as_u64().expect("a count") > 3);
    for row in rows {
        assert_eq!(row["payee"], "Northern Ice Works");
    }
    let dates: Vec<&str> = rows
        .iter()
        .map(|row| row["date"].as_str().expect("a day"))
        .collect();
    let mut newest_first = dates.clone();
    newest_first.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(dates, newest_first);
}

#[test]
fn a_category_search_includes_the_categories_below() {
    let book = space_colony();
    let (categories, _) = run(&book, "list_categories", Value::Null);
    let food = categories["categories"]
        .as_array()
        .expect("categories")
        .iter()
        .find(|row| row["name"] == "Food")
        .expect("Food");
    let (value, _) = run(
        &book,
        "search_transactions",
        json!({ "category_id": food["id"], "limit": 50 }),
    );
    let rows = value["rows"].as_array().expect("rows");
    assert!(!rows.is_empty());
    for row in rows {
        let category = row["category"].as_str().unwrap_or_default();
        let several = row["categories"].as_array().is_some_and(|names| {
            names
                .iter()
                .any(|name| name.as_str().is_some_and(|name| name.starts_with("Food")))
        });
        assert!(category.starts_with("Food") || several, "{row}");
    }
}

#[test]
fn an_answer_quoting_the_results_passes_the_number_check() {
    let book = space_colony();
    let mut known = Figures::new();
    let mut results = Vec::new();
    for function in FUNCTIONS {
        let arguments = match function.name {
            "account_balance" => {
                let id = book.accounts(false).expect("accounts")[0].id.to_string();
                json!({ "account_id": id })
            }
            "category_totals" => json!({ "from": "2026-10-01", "to": "2026-10-31" }),
            _ => json!({}),
        };
        let (value, figures) = run(&book, function.name, arguments);
        known.extend(&figures);
        results.push(value);
    }
    // October as a model would say it in German, from the results.
    let october = &results[2]["currencies"][0];
    let german = |amount: &Value| amount.as_str().expect("text").replace('.', ",");
    let answer = format!(
        "Im Oktober 2026 kamen {} € herein und {} € gingen hinaus; es fehlen {} €. \
         Auf allen Konten zusammen sind {} €.",
        german(&october["money_in"]),
        german(&october["money_out"]),
        german(&october["left"]).trim_start_matches('-'),
        german(&results[0]["together"][0]["together"]),
    );
    assert_eq!(check_numbers(&answer, &known), Vec::new(), "{answer}");

    // An invented figure stands out.
    let invented = "Im Oktober hast du 1.234,56 € für Essen ausgegeben.";
    let flagged = check_numbers(invented, &known);
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged[0].text, "1.234,56");
}

#[test]
fn the_overview_is_small_and_names_this_and_last_month() {
    let book = space_colony();
    let output = overview(&book, TODAY).expect("an overview");
    let value = &output.value;
    assert_eq!(value["today"], "2026-10-25");
    assert_eq!(value["this_month"]["from"], "2026-10-01");
    assert_eq!(value["this_month"]["to"], "2026-10-31");
    assert_eq!(value["last_month"]["from"], "2026-09-01");
    assert_eq!(value["last_month"]["to"], "2026-09-30");
    // Its months are the totals the function gives for them.
    let (mut september, _) = run(
        &book,
        "category_totals",
        json!({ "from": "2026-09-01", "to": "2026-09-30" }),
    );
    // The overview says once that it holds data, not per month.
    september.as_object_mut().expect("an object").remove("note");
    assert_eq!(value["last_month"], september);
    let (accounts, _) = run(&book, "list_accounts", json!({}));
    assert_eq!(value["accounts"], accounts["accounts"]);
    // Small enough to send with every message (D-052).
    let size = serde_json::to_string(value).expect("JSON").len();
    assert!(size < 8_000, "the overview has {size} bytes");
    assert!(!output.figures.is_empty());
}

/// Every amount of a search's rows as minor units of euros.
fn amounts(value: &Value) -> Vec<i64> {
    value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| eur(&row["amount"]).minor_units())
        .collect()
}

/// All amounts of the book's entries, found day by day so that no day hits
/// the row limit: an oracle that doesn't use the sort.
fn all_amounts(book: &Book) -> Vec<i64> {
    let mut all = Vec::new();
    let mut day = Date::constant(2024, 7, 1);
    let last = Date::constant(2027, 6, 30);
    while day <= last {
        let (value, _) = run(
            book,
            "search_transactions",
            json!({ "from": day.to_string(), "to": day.to_string(), "limit": 50 }),
        );
        assert!(
            value["matches"].as_u64().expect("a count") <= 50,
            "{day}: the oracle needs every entry of the day"
        );
        all.extend(amounts(&value));
        day = day.tomorrow().expect("a next day");
    }
    all
}

#[test]
fn the_biggest_expenses_come_from_the_sort_and_say_what_was_left_out() {
    let book = space_colony();
    let mut oracle = all_amounts(&book);
    oracle.sort_unstable();
    let (value, _) = run(
        &book,
        "search_transactions",
        json!({ "sort": "largest_out", "limit": 10 }),
    );
    assert_eq!(value["order"], "largest_out");
    assert_eq!(amounts(&value), oracle[..10]);
    let matches = value["matches"].as_u64().expect("a count");
    assert_eq!(matches, oracle.len() as u64);
    assert_eq!(value["rows_left_out"].as_u64(), Some(matches - 10));

    let (value, _) = run(
        &book,
        "search_transactions",
        json!({ "sort": "largest_in", "limit": 10 }),
    );
    let mut biggest_in = oracle.clone();
    biggest_in.reverse();
    assert_eq!(amounts(&value), biggest_in[..10]);
}

#[test]
fn a_search_that_shows_everything_does_not_say_rows_were_left_out() {
    let book = space_colony();
    let (value, _) = run(
        &book,
        "search_transactions",
        json!({ "from": "2027-02-24", "to": "2027-02-24", "limit": 50 }),
    );
    assert!(value["matches"].as_u64().expect("a count") > 0);
    assert!(value["matches"].as_u64().expect("a count") <= 50);
    assert!(value.get("rows_left_out").is_none());
    assert_eq!(value["order"], "newest");
}

#[test]
fn the_amount_filters_keep_entries_by_size_and_include_the_bounds() {
    let book = space_colony();
    let oracle = all_amounts(&book);
    let sizes: Vec<i64> = oracle.iter().map(|amount| amount.abs()).collect();
    let (value, _) = run(
        &book,
        "search_transactions",
        json!({ "min_amount": "100", "max_amount": "200.50", "limit": 50 }),
    );
    let wanted = sizes
        .iter()
        .filter(|&&size| (10_000..=20_050).contains(&size))
        .count();
    assert_eq!(value["matches"].as_u64(), Some(wanted as u64));
    assert!(wanted > 0, "the book has entries of that size");
    for amount in amounts(&value) {
        assert!((10_000..=20_050).contains(&amount.abs()), "{amount}");
    }
    // The bounds belong to the range: one entry's own size finds it.
    let exact = sizes[0];
    let text = format!("{}.{:02}", exact / 100, exact % 100);
    let (value, _) = run(
        &book,
        "search_transactions",
        json!({ "min_amount": text, "max_amount": text, "limit": 50 }),
    );
    let found = amounts(&value);
    assert!(!found.is_empty());
    assert!(found.iter().all(|amount| amount.abs() == exact));
}

#[test]
fn a_sort_and_a_size_that_are_not_plain_are_refused() {
    let book = space_colony();
    for arguments in [
        json!({ "sort": "biggest" }),
        json!({ "min_amount": "-5" }),
        json!({ "min_amount": "1e3" }),
        json!({ "min_amount": "5." }),
        json!({ "max_amount": ".5" }),
        json!({ "max_amount": "1,000" }),
        json!({ "max_amount": 5 }),
        json!({ "max_amount": "1234567890123" }),
        json!({ "min_amount": "0.1234567" }),
    ] {
        let error = call(&book, "search_transactions", &arguments, TODAY)
            .expect_err(&arguments.to_string());
        assert!(
            matches!(error, ToolError::BadArgument { .. }),
            "{arguments}: {error:?}"
        );
    }
}

#[test]
fn moves_between_own_accounts_and_starting_balances_can_be_left_out() {
    let book = space_colony();
    let (all, _) = run(
        &book,
        "search_transactions",
        json!({ "sort": "largest_out", "limit": 50 }),
    );
    let rows_of = |value: &Value| value["rows"].as_array().expect("rows").clone();
    let is_move =
        |row: &Value| row.get("transfer").is_some() || row.get("starting_balance").is_some();
    // The biggest amount in the book is a starting balance: without the
    // filter it would be called the biggest expense.
    assert!(rows_of(&all).iter().any(is_move));
    let (spending, _) = run(
        &book,
        "search_transactions",
        json!({ "sort": "largest_out", "limit": 50, "skip_transfers": true }),
    );
    let rows = rows_of(&spending);
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|row| !is_move(row)));
    assert!(
        spending["matches"].as_u64().expect("a count") < all["matches"].as_u64().expect("a count")
    );
}

/// Models send `"text": ""` when they mean no text filter (#195).
#[test]
fn an_empty_text_means_no_text_filter() {
    let book = space_colony();
    let (all, _) = run(&book, "search_transactions", json!({ "limit": 20 }));
    for empty in ["", "   "] {
        let (value, _) = run(
            &book,
            "search_transactions",
            json!({ "text": empty, "limit": 20 }),
        );
        assert_eq!(value, all);
    }
}
