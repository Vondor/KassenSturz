// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Another program's book into a new book (M2.9), built in code, so it
//! waits for no file format. All data is made up (AGENTS.md §3.1 rule 2).

use std::collections::HashMap;

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, Currency, CurrencyCode, Date, ExchangeRate,
    ImportedDetails, Money, RateSource, Split, Transaction,
};
use kassensturz_import::{
    BookAccount, BookPart, BookRate, BookTransaction, ImportError, ImportedBook, Replacement,
    Skipped, SkippedData, UnlistedCurrency,
};
use kassensturz_pipeline::{PipelineError, fill_book_from};
use kassensturz_store::{Book, StoreError};

const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
    Ok(code) => code,
    Err(_) => panic!("EUR is a valid code"),
};
const USD: CurrencyCode = match CurrencyCode::new("USD") {
    Ok(code) => code,
    Err(_) => panic!("USD is a valid code"),
};
const DEM: CurrencyCode = match CurrencyCode::new("DEM") {
    Ok(code) => code,
    Err(_) => panic!("DEM is a valid code"),
};

const ROL: CurrencyCode = match CurrencyCode::new("ROL") {
    Ok(code) => code,
    Err(_) => panic!("ROL is a valid code"),
};
const RON: CurrencyCode = match CurrencyCode::new("RON") {
    Ok(code) => code,
    Err(_) => panic!("RON is a valid code"),
};

fn currency(code: CurrencyCode) -> Currency {
    Currency::new(code, 2).expect("two decimal places")
}

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn usd(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, USD)
}

fn dem(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, DEM)
}

fn day(day: i8) -> Date {
    Date::new(2026, 9, day).expect("a valid date")
}

fn account(key: &str, name: &str, kind: AccountKind) -> BookAccount {
    BookAccount {
        key: key.into(),
        name: name.into(),
        kind,
        account_type: None,
        currency: currency(EUR),
        parent: None,
        closed: false,
    }
}

fn part(account: Option<&str>, amount: Money) -> BookPart {
    BookPart {
        account: account.map(str::to_owned),
        amount,
        account_amount: None,
        details: None,
    }
}

fn transaction(date: Date, payee: Option<&str>, parts: Vec<BookPart>) -> BookTransaction {
    BookTransaction {
        date,
        payee: payee.map(str::to_owned),
        currency: currency(EUR),
        parts,
    }
}

/// An account or category the other program kept in German marks.
fn in_marks(key: &str, name: &str, kind: AccountKind) -> BookAccount {
    BookAccount {
        currency: currency(DEM),
        ..account(key, name, kind)
    }
}

/// A transaction in German marks.
fn in_marks_on(date: Date, payee: Option<&str>, parts: Vec<BookPart>) -> BookTransaction {
    BookTransaction {
        currency: currency(DEM),
        ..transaction(date, payee, parts)
    }
}

/// A part whose amount in its account's currency differs from the one in
/// the transaction's.
fn part_in(account: &str, amount: Money, account_amount: Money) -> BookPart {
    BookPart {
        account_amount: Some(account_amount),
        ..part(Some(account), amount)
    }
}

fn date(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("a valid date")
}

/// A small household as another program kept it: two accounts (one
/// closed and empty, one closed with money still in it), a category tree,
/// a starting balance, a purchase, a transfer, an entry imported from a
/// bank without a category yet, a payment in dollars and a rate.
fn household() -> ImportedBook {
    let mut checking = account("A1", "Checking", AccountKind::Asset);
    checking.account_type = Some(AccountType::Checking);
    let mut old = account("A2", "Old savings", AccountKind::Asset);
    old.closed = true;
    let mut card = account("A3", "Card", AccountKind::Liability);
    card.account_type = Some(AccountType::CreditCard);
    card.closed = true;
    let mut groceries = account("E2", "Groceries", AccountKind::Expense);
    groceries.parent = Some("E1".into());
    let mut travel = account("E3", "Travel", AccountKind::Expense);
    travel.currency = currency(USD);
    travel.parent = Some("E1".into());
    let accounts = vec![
        checking,
        old,
        card,
        account("Q1", "Opening balances", AccountKind::Equity),
        account("E1", "Food", AccountKind::Expense),
        groceries,
        travel,
        account("I1", "Salary", AccountKind::Income),
        // Taken already by an account, ignoring capitals.
        account("A4", "checking ", AccountKind::Asset),
    ];
    let bank = ImportedDetails {
        bank_id: Some("REF-0001".into()),
        remittance_information: Some("Card payment\nCorner Grocery".into()),
        ..ImportedDetails::default()
    };
    let transactions = vec![
        transaction(
            day(1),
            None,
            vec![
                part(Some("A1"), eur(150_000)),
                part(Some("Q1"), eur(-150_000)),
            ],
        ),
        transaction(
            day(2),
            Some("Corner Grocery"),
            vec![
                BookPart {
                    details: Some(bank),
                    ..part(Some("A1"), eur(-5_320))
                },
                part(Some("E2"), eur(5_320)),
            ],
        ),
        // A transfer that empties the old savings account.
        transaction(
            day(3),
            None,
            vec![
                part(Some("A2"), eur(20_000)),
                part(Some("A2"), eur(-20_000)),
            ],
        ),
        // Money onto the card, which stays owed.
        transaction(
            day(4),
            None,
            vec![part(Some("A3"), eur(-1_000)), part(Some("A1"), eur(1_000))],
        ),
        // Only the account's part: the other program had no category yet.
        transaction(
            day(5),
            Some("Payroll"),
            vec![part(Some("A1"), eur(250_000))],
        ),
        // A hotel bill in dollars, paid in euros.
        transaction(
            day(6),
            Some("Hotel"),
            vec![
                part(Some("A1"), eur(-9_250)),
                BookPart {
                    account_amount: Some(usd(10_000)),
                    ..part(Some("E3"), eur(9_250))
                },
            ],
        ),
    ];
    ImportedBook {
        accounts,
        transactions,
        rates: vec![BookRate {
            date: day(6),
            from: EUR,
            to: USD,
            rate: "1.0811".parse::<ExchangeRate>().expect("a rate"),
        }],
        unlisted: Vec::new(),
        skipped: vec![Skipped {
            what: SkippedData::Schedules,
            count: 2,
        }],
    }
}

fn named(book: &Book, name: &str) -> kassensturz_core::Account {
    book.all_accounts()
        .expect("accounts")
        .into_iter()
        .find(|account| account.name == name)
        .unwrap_or_else(|| panic!("{name} is in the book"))
}

#[test]
fn a_book_comes_in_with_its_accounts_categories_and_transactions() {
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from(&mut book, &household(), &HashMap::new()).expect("filled");
    assert_eq!(report.accounts, 4);
    assert_eq!(report.categories, 4);
    assert_eq!(report.transactions, 6);
    assert_eq!(report.rates, 1);
    // Only the payroll entry didn't add up.
    assert_eq!(report.completed, 1);
    assert_eq!(
        report.skipped,
        vec![Skipped {
            what: SkippedData::Schedules,
            count: 2,
        }]
    );

    let checking = named(&book, "Checking");
    assert_eq!(checking.account_type, Some(AccountType::Checking));
    assert_eq!(
        book.balance(checking.id).expect("balance"),
        eur(150_000 - 5_320 + 1_000 + 250_000 - 9_250)
    );
    // The second "checking" got a number, and the report says so.
    let second = named(&book, "checking (2)");
    assert_eq!(report.renamed, vec![(second.id, "checking ".to_owned())]);

    // The tree came along, the dollar category inside the euro one moved
    // to the top.
    let food = named(&book, "Food");
    assert_eq!(named(&book, "Groceries").parent, Some(food.id));
    let travel = named(&book, "Travel");
    assert_eq!(travel.parent, None);
    assert_eq!(report.moved_to_top, vec![travel.id]);
    assert_eq!(book.balance(travel.id).expect("balance"), usd(10_000));

    // The empty account is closed; the card, still owed, is open.
    assert!(named(&book, "Old savings").closed);
    let card = named(&book, "Card");
    assert!(!card.closed);
    assert_eq!(report.left_open, vec![card.id]);

    // What the file delivered stays with the account's part.
    let transactions = book.transactions().expect("transactions");
    let purchase = transactions
        .iter()
        .find(|transaction| transaction.payee() == Some("Corner Grocery"))
        .expect("the purchase");
    let delivered = purchase.splits()[0].imported.as_ref().expect("details");
    assert_eq!(delivered.bank_id.as_deref(), Some("REF-0001"));
    // The payroll entry got a part without a category.
    let payroll = transactions
        .iter()
        .find(|transaction| transaction.payee() == Some("Payroll"))
        .expect("the payroll entry");
    assert_eq!(payroll.splits().len(), 2);
    assert_eq!(payroll.splits()[1].account, None);
    assert_eq!(payroll.splits()[1].amount, eur(-250_000));

    assert_eq!(book.all_rates().expect("rates").len(), 1);
    // Nothing to undo: the book was made this way.
    assert_eq!(book.history().expect("history"), Vec::new());
}

#[test]
fn a_book_that_cant_be_filled_stays_empty() {
    // A part booked to a key no account has.
    let mut broken = household();
    broken.transactions[1].parts[1].account = Some("E9".into());
    let mut book = Book::create_in_memory().expect("book created");
    assert!(matches!(
        fill_book_from(&mut book, &broken, &HashMap::new()),
        Err(PipelineError::Import(ImportError::Malformed { .. }))
    ));
    assert!(book.all_accounts().expect("accounts").is_empty());

    // A name the book refuses.
    let mut unnamed = household();
    unnamed.accounts[0].name = "  ".into();
    assert!(matches!(
        fill_book_from(&mut book, &unnamed, &HashMap::new()),
        Err(PipelineError::Store(StoreError::EmptyAccountName))
    ));
    assert!(book.all_accounts().expect("accounts").is_empty());

    // Only a new, empty book is filled.
    fill_book_from(&mut book, &household(), &HashMap::new()).expect("filled");
    assert!(matches!(
        fill_book_from(&mut book, &household(), &HashMap::new()),
        Err(PipelineError::Store(StoreError::NotEmpty))
    ));
}

/// A household that kept its book in German marks, as another program kept
/// it, and went on in euros in 2002: a checking account in marks emptied
/// into a new one in euros, categories in marks, one of them inside a
/// category in euros, and an account in marks that was never used.
fn household_in_marks() -> ImportedBook {
    let mut bakery = in_marks("C5", "Bakery", AccountKind::Expense);
    bakery.parent = Some("C4".into());
    let accounts = vec![
        in_marks("A1", "Checking", AccountKind::Asset),
        account("A2", "Girokonto", AccountKind::Asset),
        in_marks("A3", "Old savings", AccountKind::Asset),
        in_marks("Q1", "Opening balances", AccountKind::Equity),
        in_marks("C1", "Groceries", AccountKind::Expense),
        in_marks("C2", "Household", AccountKind::Expense),
        in_marks("C3", "Drinks", AccountKind::Expense),
        account("C4", "Food", AccountKind::Expense),
        bakery,
    ];
    let transactions = vec![
        in_marks_on(
            date(1998, 12, 1),
            None,
            vec![
                part(Some("A1"), dem(100_000)),
                part(Some("Q1"), dem(-100_000)),
            ],
        ),
        in_marks_on(
            date(1998, 12, 15),
            Some("Market"),
            vec![
                part(Some("A1"), dem(-10_000)),
                part(Some("C1"), dem(10_000)),
            ],
        ),
        // 0.01 DEM is 0.0051... EUR, but 0.03 DEM is 0.0153... EUR.
        in_marks_on(
            date(1999, 2, 1),
            Some("Corner shop"),
            vec![
                part(Some("A1"), dem(-3)),
                part(Some("C1"), dem(1)),
                part(Some("C2"), dem(1)),
                part(Some("C3"), dem(1)),
            ],
        ),
        in_marks_on(
            date(1999, 3, 1),
            Some("Hardware store"),
            vec![part(Some("A1"), dem(-1)), part(Some("C2"), dem(1))],
        ),
        // The rest moved to the new account in euros: the file gives the
        // amount in both currencies.
        transaction(
            date(2002, 1, 2),
            None,
            vec![
                part_in("A1", eur(-46_014), dem(-89_996)),
                part(Some("A2"), eur(46_014)),
            ],
        ),
        // Old prices at the bakery: the category is still in marks.
        transaction(
            date(2002, 2, 1),
            Some("Bakery"),
            vec![
                part(Some("A2"), eur(-1_000)),
                part_in("C5", eur(1_000), dem(1_956)),
            ],
        ),
    ];
    ImportedBook {
        accounts,
        transactions,
        rates: Vec::new(),
        unlisted: Vec::new(),
        skipped: Vec::new(),
    }
}

fn paid_by(transactions: &[Transaction], payee: &str) -> Transaction {
    transactions
        .iter()
        .find(|transaction| transaction.payee() == Some(payee))
        .unwrap_or_else(|| panic!("{payee} is in the book"))
        .clone()
}

fn amounts(transaction: &Transaction) -> Vec<Money> {
    transaction
        .splits()
        .iter()
        .map(|split| split.amount)
        .collect()
}

/// The amounts each part had in marks, if it had one.
fn originals(transaction: &Transaction) -> Vec<Option<Money>> {
    transaction
        .splits()
        .iter()
        .map(|split| split.original.map(|original| original.amount))
        .collect()
}

/// Checks that `split` was converted out of marks at the fixed rate.
fn assert_converted_from_marks(split: &Split) {
    let original = split.original.expect("an original amount");
    assert_eq!(original.amount.currency(), DEM);
    assert_eq!(original.rate.from, EUR);
    assert_eq!(original.rate.to, DEM);
    assert_eq!(
        original.rate.rate,
        "1.95583".parse::<ExchangeRate>().expect("a rate")
    );
    assert_eq!(original.rate.date, date(1999, 1, 1));
    assert_eq!(original.rate.source, RateSource::Changeover);
}

#[test]
fn a_book_in_marks_comes_in_in_euros_and_keeps_the_amounts_in_marks() {
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from(&mut book, &household_in_marks(), &HashMap::new()).expect("filled");
    assert_eq!(report.accounts, 3);
    assert_eq!(report.categories, 5);
    assert_eq!(report.transactions, 6);
    assert_eq!(report.completed, 0);
    let mark = DEM.withdrawn().expect("the table lists the mark");
    assert_eq!(report.converted.len(), 1);
    assert_eq!(report.converted[0].withdrawn, mark);
    // All but the two accounts in euros, and every part in marks.
    assert_eq!(report.converted[0].accounts, 7);
    assert_eq!(report.converted[0].parts, 12);

    // Every account and category is in euros now; the book lists the mark
    // only for the parts' original amounts.
    for account in book.all_accounts().expect("accounts") {
        assert_eq!(account.currency, EUR, "{}", account.name);
    }
    let mut currencies = book.currencies().expect("currencies");
    currencies.sort_by_key(|currency| currency.code().as_str().to_owned());
    assert_eq!(currencies, vec![currency(DEM), currency(EUR)]);
    // In euros both, the bakery stays inside food.
    assert_eq!(named(&book, "Bakery").parent, Some(named(&book, "Food").id));
    assert!(report.moved_to_top.is_empty());

    // Each balance is the balance in marks converted once: 1,000.00 DEM
    // are 511.29 EUR. The account the old book emptied is empty.
    let balance = |name: &str| book.balance(named(&book, name).id).expect("balance");
    assert_eq!(balance("Checking"), eur(0));
    assert_eq!(balance("Opening balances"), eur(-51_129));
    assert_eq!(balance("Groceries"), eur(5_113));
    assert_eq!(balance("Girokonto"), eur(46_014 - 1_000));
    assert_eq!(balance("Old savings"), eur(0));

    let transactions = book.transactions().expect("transactions");
    // Converted on their own, the four parts would be -0.02, 0.01, 0.01
    // and 0.01 EUR, which don't add up. The checking account's part follows
    // its balance (899.97 DEM are 460.15 EUR, one cent less than 900.00
    // DEM); a category takes what is left over.
    let corner_shop = paid_by(&transactions, "Corner shop");
    assert_eq!(amounts(&corner_shop), vec![eur(-1), eur(0), eur(0), eur(1)]);
    assert_eq!(
        originals(&corner_shop),
        vec![Some(dem(-3)), Some(dem(1)), Some(dem(1)), Some(dem(1))]
    );
    for split in corner_shop.splits() {
        assert_converted_from_marks(split);
        assert_eq!(split.foreign, None);
    }
    // The household category's next penny catches up: 0.02 DEM are 0.01
    // EUR.
    let hardware = paid_by(&transactions, "Hardware store");
    assert_eq!(amounts(&hardware), vec![eur(-1), eur(1)]);
    assert_eq!(balance("Household"), eur(1));
    assert_eq!(balance("Drinks"), eur(1));

    // Where the file gave the amount in euros, it stays.
    let bakery = paid_by(&transactions, "Bakery");
    assert_eq!(amounts(&bakery), vec![eur(-1_000), eur(1_000)]);
    assert_eq!(originals(&bakery), vec![None, Some(dem(1_956))]);
    assert_converted_from_marks(&bakery.splits()[1]);
    assert_eq!(bakery.splits()[1].foreign, None);
    assert_eq!(balance("Bakery"), eur(1_000));

    assert!(book.all_rates().expect("rates").is_empty());
    assert_eq!(book.history().expect("history"), Vec::new());
}

#[test]
fn parts_in_other_currencies_keep_what_the_file_gave() {
    let accounts = vec![
        in_marks("A1", "Checking", AccountKind::Asset),
        account("A2", "Girokonto", AccountKind::Asset),
        in_marks("C1", "Groceries", AccountKind::Expense),
        BookAccount {
            currency: currency(USD),
            ..account("C2", "Travel", AccountKind::Expense)
        },
    ];
    let transactions = vec![
        // From the account in euros, in marks: the file gives 51.12 EUR,
        // one cent less than the fixed rate gives for 100.00 DEM, and the
        // category takes the difference.
        in_marks_on(
            date(1998, 11, 2),
            Some("Market"),
            vec![
                part_in("A2", dem(-10_000), eur(-5_112)),
                part(Some("C1"), dem(10_000)),
            ],
        ),
        // In dollars, from the account in marks.
        BookTransaction {
            currency: currency(USD),
            ..transaction(
                date(1998, 11, 3),
                Some("Hotel"),
                vec![
                    part_in("A1", usd(-6_000), dem(-10_000)),
                    part(Some("C2"), usd(6_000)),
                ],
            )
        },
        // In marks, into the category in dollars.
        in_marks_on(
            date(1998, 11, 4),
            Some("Car rental"),
            vec![
                part_in("C2", dem(10_000), usd(6_000)),
                part(Some("A1"), dem(-10_000)),
            ],
        ),
        // Only the account's part, without a category yet.
        in_marks_on(
            date(1998, 11, 5),
            Some("Bank fee"),
            vec![part(Some("A1"), dem(-1_000))],
        ),
    ];
    let imported = ImportedBook {
        accounts,
        transactions,
        rates: Vec::new(),
        unlisted: Vec::new(),
        skipped: Vec::new(),
    };
    let mut book = Book::create_in_memory().expect("book created");
    let report = fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    assert_eq!(report.completed, 1);
    assert_eq!(report.converted[0].accounts, 2);
    assert_eq!(report.converted[0].parts, 6);

    let transactions = book.transactions().expect("transactions");
    let market = paid_by(&transactions, "Market");
    assert_eq!(amounts(&market), vec![eur(-5_112), eur(5_112)]);
    assert_eq!(
        originals(&market),
        vec![Some(dem(-10_000)), Some(dem(10_000))]
    );
    assert_eq!(market.splits()[0].foreign, None);

    // The dollars stay; the amount in marks, converted, is what the part
    // books to the account in euros now.
    let hotel = paid_by(&transactions, "Hotel");
    let paid = &hotel.splits()[0];
    assert_eq!(paid.amount, usd(-6_000));
    assert_eq!(paid.account_amount(), eur(-5_113));
    assert_eq!(paid.foreign.and_then(|foreign| foreign.rate), None);
    assert_converted_from_marks(paid);
    assert_eq!(originals(&hotel), vec![Some(dem(-10_000)), None]);

    // The dollars the file gave stay what the category got; the marks are
    // converted into the transaction's euros.
    let rental = paid_by(&transactions, "Car rental");
    assert_eq!(amounts(&rental), vec![eur(5_113), eur(-5_113)]);
    assert_eq!(rental.splits()[0].account_amount(), usd(6_000));
    assert_converted_from_marks(&rental.splits()[0]);

    // 10.00 DEM more out of the account: 210.00 DEM are 107.37 EUR, and
    // the part without a category takes the other side.
    let fee = paid_by(&transactions, "Bank fee");
    assert_eq!(amounts(&fee), vec![eur(-511), eur(511)]);
    assert_eq!(originals(&fee), vec![Some(dem(-1_000)), None]);

    let balance = |name: &str| book.balance(named(&book, name).id).expect("balance");
    assert_eq!(balance("Checking"), eur(-10_737));
    assert_eq!(balance("Girokonto"), eur(-5_112));
    assert_eq!(balance("Groceries"), eur(5_112));
    assert_eq!(balance("Travel"), usd(12_000));
}

fn rol(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, ROL)
}

fn ron(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, RON)
}

/// A holiday in Romania in 2003, kept in the old leu, which neither of
/// KassenSturz's tables lists (D-106): cash in lei and a hotel paid with it.
fn holiday_in_lei() -> ImportedBook {
    let in_lei = |key: &str, name: &str, kind| BookAccount {
        currency: currency(ROL),
        ..account(key, name, kind)
    };
    let on = |date, payee, parts| BookTransaction {
        currency: currency(ROL),
        ..transaction(date, payee, parts)
    };
    ImportedBook {
        accounts: vec![
            account("A1", "Checking", AccountKind::Asset),
            in_lei("A2", "Lei in cash", AccountKind::Asset),
            in_lei("Q1", "Opening balances", AccountKind::Equity),
            in_lei("C1", "Holiday", AccountKind::Expense),
        ],
        transactions: vec![
            // 2,000,000.00 ROL, then a hotel for 1,500,000.00 ROL.
            on(
                date(2003, 8, 1),
                None,
                vec![
                    part(Some("A2"), rol(200_000_000)),
                    part(Some("Q1"), rol(-200_000_000)),
                ],
            ),
            on(
                date(2003, 8, 4),
                Some("Hotel"),
                vec![
                    part(Some("A2"), rol(-150_000_000)),
                    part(Some("C1"), rol(150_000_000)),
                ],
            ),
        ],
        rates: Vec::new(),
        unlisted: vec![UnlistedCurrency {
            currency: currency(ROL),
            suggestion: Some(Replacement {
                successor: RON,
                rate: "10000".parse().expect("a rate"),
                date: date(2005, 6, 30),
            }),
        }],
        skipped: Vec::new(),
    }
}

#[test]
fn a_currency_no_table_lists_comes_in_as_the_user_says_it_was_replaced() {
    let imported = holiday_in_lei();
    let replaced = |successor| {
        HashMap::from([(
            ROL,
            Replacement {
                successor,
                rate: "10000".parse().expect("a rate"),
                date: date(2005, 7, 1),
            },
        )])
    };
    // The file's rate is only a suggestion: without the user's word,
    // nothing comes in. Nor does it into a currency an account can't be in.
    let mut book = Book::create_in_memory().expect("book created");
    assert!(matches!(
        fill_book_from(&mut book, &imported, &HashMap::new()),
        Err(PipelineError::Import(ImportError::SuccessorNeeded {
            currency: ROL
        }))
    ));
    for successor in [DEM, ROL] {
        assert!(matches!(
            fill_book_from(&mut book, &imported, &replaced(successor)),
            Err(PipelineError::Import(ImportError::SuccessorNotCurrent {
                currency: ROL,
                successor: found,
            })) if found == successor
        ));
    }
    assert!(book.all_accounts().expect("accounts").is_empty());

    let report = fill_book_from(&mut book, &imported, &replaced(RON)).expect("filled");
    assert_eq!(report.converted.len(), 1);
    let leu = report.converted[0].withdrawn;
    assert_eq!(
        (leu.currency(), leu.successor(), leu.date()),
        (currency(ROL), RON, date(2005, 7, 1))
    );
    assert_eq!(
        (report.converted[0].accounts, report.converted[0].parts),
        (3, 4)
    );
    let balance = |name: &str| book.balance(named(&book, name).id).expect("balance");
    assert_eq!(balance("Lei in cash"), ron(5_000));
    assert_eq!(balance("Holiday"), ron(15_000));
    assert_eq!(balance("Opening balances"), ron(-20_000));
    assert_eq!(balance("Checking"), eur(0));
    let hotel = paid_by(&book.transactions().expect("transactions"), "Hotel");
    assert_eq!(amounts(&hotel), vec![ron(-15_000), ron(15_000)]);
    assert_eq!(
        originals(&hotel),
        vec![Some(rol(-150_000_000)), Some(rol(150_000_000))]
    );
    let rate = hotel.splits()[0].original.expect("converted").rate;
    assert_eq!(
        (rate.from, rate.to, rate.date),
        (RON, ROL, date(2005, 7, 1))
    );
    assert_eq!(rate.rate, "10000".parse::<ExchangeRate>().expect("a rate"));
    assert_eq!(rate.source, RateSource::Changeover);

    // The book lists the old leu for the original amounts, but no account
    // can be in it.
    let mut currencies = book.currencies().expect("currencies");
    currencies.sort_by_key(|currency| currency.code().as_str().to_owned());
    assert_eq!(
        currencies,
        vec![currency(EUR), currency(ROL), currency(RON)]
    );
    let in_lei = Account {
        id: AccountId::generate(),
        name: "More lei".into(),
        kind: AccountKind::Asset,
        currency: ROL,
        account_type: None,
        parent: None,
        closed: false,
    };
    assert!(matches!(
        book.add_account(&in_lei),
        Err(StoreError::WithdrawnCurrency(ROL))
    ));
}
