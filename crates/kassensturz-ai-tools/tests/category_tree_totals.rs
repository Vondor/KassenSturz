// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `category_totals` and subcategories (M5-B40, #850): every row leads with
//! the category's total with everything below it, so a model never has to
//! add rows up itself, and a parent's own bookings are only under a name no
//! model takes for the total. The book is synthetic and filled through the
//! store's own operations.

#![cfg(feature = "ai")]

use kassensturz_ai_tools::{ToolError, call, check_numbers};
use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Split, Transaction, TransactionId,
};
use kassensturz_store::Book;
use serde_json::{Value, json};

const TODAY: Date = Date::constant(2026, 10, 25);

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, CurrencyCode::new("EUR").expect("EUR"))
}

fn account(name: &str, kind: AccountKind, parent: Option<AccountId>) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.to_owned(),
        kind,
        currency: CurrencyCode::new("EUR").expect("EUR"),
        account_type: None,
        parent,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    }
}

fn book_with_food_tree() -> Book {
    let mut book = Book::create_in_memory().expect("a book in memory");
    book.use_currency(CurrencyCode::new("EUR").expect("EUR"))
        .expect("euros");
    let checking = account("Everyday Checking", AccountKind::Asset, None);
    let food = account("Food", AccountKind::Expense, None);
    let groceries = account("Groceries", AccountKind::Expense, Some(food.id));
    let dining = account("Dining out", AccountKind::Expense, Some(food.id));
    // A parent with nothing of its own booked to it, only children.
    let home = account("Home", AccountKind::Expense, None);
    let rent = account("Rent", AccountKind::Expense, Some(home.id));
    let salary = account("Salary", AccountKind::Income, None);
    let bonus = account("Bonus", AccountKind::Income, Some(salary.id));
    for a in [
        &checking, &food, &groceries, &dining, &home, &rent, &salary, &bonus,
    ] {
        book.add_account(a).expect("an account");
    }
    let mut day = 0;
    let mut book_to = |category: &Account, minor_units: i64| {
        day += 1;
        let split = if category.kind == AccountKind::Income {
            minor_units
        } else {
            -minor_units
        };
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, day),
            None,
            vec![
                Split::new(checking.id, eur(split)),
                Split::new(category.id, eur(-split)),
            ],
        )
        .expect("a transaction");
        book.add_transaction(&transaction).expect("added");
    };
    book_to(&food, 1_786);
    book_to(&groceries, 2_735);
    book_to(&dining, 705);
    book_to(&rent, 80_000);
    book_to(&salary, 200_000);
    book_to(&bonus, 10_000);
    book
}

fn row<'a>(side: &'a Value, name: &str) -> &'a Value {
    side.as_array()
        .expect("a list")
        .iter()
        .find(|row| row["category"] == name)
        .unwrap_or_else(|| panic!("no row {name}"))
}

#[test]
fn a_row_leads_with_the_total_and_names_the_own_bookings_apart() -> Result<(), ToolError> {
    // #850: two models quoted a parent's own bookings, then the row's
    // "amount", as the category's total. Every row now has "total", and a
    // parent's own bookings are only under "own_bookings_only".
    let book = book_with_food_tree();
    let value = call(
        &book,
        "category_totals",
        &json!({ "from": "2026-10-01", "to": "2026-10-31" }),
        TODAY,
    )?
    .value;
    let totals = &value["currencies"][0];
    let spending = &totals["spending"];
    for side in [spending, &totals["income"]] {
        for row in side.as_array().expect("a list") {
            assert!(row.get("amount").is_none(), "{row}");
            assert!(row.get("amount_with_subcategories").is_none(), "{row}");
        }
    }

    let food = row(spending, "Food");
    assert_eq!(food["total"], json!("52.26"));
    assert_eq!(food["own_bookings_only"], json!("17.86"));
    // A subcategory is a leaf here: only its total.
    let groceries = row(spending, "Food › Groceries");
    assert_eq!(groceries["total"], json!("27.35"));
    assert!(groceries.get("own_bookings_only").is_none());
    // A parent with nothing booked to itself is still listed, with its total.
    let home = row(spending, "Home");
    assert_eq!(home["total"], json!("800.00"));
    assert_eq!(home["own_bookings_only"], json!("0.00"));
    // Same for income.
    let salary = row(&totals["income"], "Salary");
    assert_eq!(salary["total"], json!("2100.00"));
    assert_eq!(salary["own_bookings_only"], json!("2000.00"));

    // The totals of the top-level rows add up to money in and money out,
    // as the function's description says: nothing is counted twice there.
    assert_eq!(totals["money_out"], json!("852.26"));
    assert_eq!(totals["money_in"], json!("2100.00"));
    for (side, sum) in [("spending", "money_out"), ("income", "money_in")] {
        let top: i64 = totals[side]
            .as_array()
            .expect("a list")
            .iter()
            .filter(|row| row.get("parent_id").is_none())
            .map(|row| cents(&row["total"]))
            .sum();
        assert_eq!(top, cents(&totals[sum]), "{side}");
    }
    Ok(())
}

/// Exact decimal text such as "852.26" in cents, read without floating
/// point.
fn cents(text: &Value) -> i64 {
    let text = text.as_str().expect("decimal text");
    let (whole, fraction) = text.split_once('.').expect("two places");
    whole.parse::<i64>().expect("digits") * 100 + fraction.parse::<i64>().expect("digits")
}

#[test]
fn the_number_check_flags_the_own_bookings_quoted_as_the_total() -> Result<(), ToolError> {
    // #850, where steps meet: the figures of the real result, checked as
    // the chat checks an answer (D-052).
    let book = book_with_food_tree();
    let output = call(
        &book,
        "category_totals",
        &json!({ "from": "2026-10-01", "to": "2026-10-31" }),
        TODAY,
    )?;
    let flagged = |answer: &str| -> Vec<String> {
        check_numbers(answer, &output.figures)
            .into_iter()
            .map(|number| number.text)
            .collect()
    };
    assert_eq!(flagged("You spent 17.86 EUR on Food."), ["17.86"]);
    assert_eq!(
        flagged("You spent 52.26 EUR on Food."),
        Vec::<String>::new()
    );
    assert_eq!(
        flagged("Food: 52.26 EUR, of which 17.86 EUR on Food itself."),
        Vec::<String>::new()
    );
    // A leaf's total is a figure of its own.
    assert_eq!(flagged("Groceries: 27.35 EUR."), Vec::<String>::new());
    assert_eq!(flagged("Salary brought 2,000.00 EUR."), ["2,000.00"]);
    Ok(())
}

#[test]
fn rows_are_sorted_by_the_total_with_subcategories() -> Result<(), ToolError> {
    // M6-B131: Home has nothing of its own but 800.00 below it, so it comes
    // before Food (52.26 with its subcategories) instead of last, where the
    // limit would cut it off.
    let book = book_with_food_tree();
    let value = call(
        &book,
        "category_totals",
        &json!({ "from": "2026-10-01", "to": "2026-10-31" }),
        TODAY,
    )?
    .value;
    let names: Vec<&str> = value["currencies"][0]["spending"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|row| row["category"].as_str().expect("a name"))
        .collect();
    assert_eq!(
        names,
        [
            "Home",
            "Home › Rent",
            "Food",
            "Food › Groceries",
            "Food › Dining out"
        ]
    );
    Ok(())
}
