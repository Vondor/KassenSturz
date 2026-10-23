// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The complete export: a whole book exported, read back into a new book
//! and compared (D-029, D-046). All data is made up (AGENTS.md §3.1 rule 2);
//! the IBAN is the example for Germany in SWIFT's IBAN Registry (D-055).

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, Actions, AmountTest, BankLink, BookTo, Condition,
    CurrencyCode, Date, DatedRate, DatedRateId, ExchangeRate, ForeignAmount, ImportedDetails,
    MatchMode, Money, RateSource, RoundingMode, Rule, RuleId, Split, TextField, TextOperator,
    Transaction, TransactionId,
};
use kassensturz_export::{BookData, BookExport, ExportData, export, read_book};
use kassensturz_pipeline::{export_book, fill_book};
use kassensturz_store::{Book, HistoryRetention, StoreError};

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

fn eur(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, EUR)
}

fn usd(minor_units: i64) -> Money {
    Money::from_minor_units(minor_units, USD)
}

fn day(day: i8) -> Date {
    Date::new(2026, 10, day).expect("a valid date")
}

fn add(
    book: &mut Book,
    name: &str,
    kind: AccountKind,
    account_type: Option<AccountType>,
    currency: CurrencyCode,
    parent: Option<AccountId>,
) -> AccountId {
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        name: name.into(),
        kind,
        currency,
        account_type,
        parent,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    })
    .expect("account added");
    id
}

fn add_transaction(
    book: &mut Book,
    date: Date,
    payee: Option<&str>,
    splits: Vec<Split>,
) -> Transaction {
    let transaction = Transaction::new(
        TransactionId::generate(),
        date,
        payee.map(str::to_owned),
        splits,
    )
    .expect("balanced");
    book.add_transaction(&transaction).expect("added");
    transaction
}

/// A purchase as an import adds it: the account's part with what the bank
/// delivered, and a part without a category yet (D-083).
fn imported(book: &mut Book, account: AccountId, minor_units: i64, date: Date, other_party: &str) {
    let mut own = Split::new(account, eur(minor_units));
    own.imported = Some(ImportedDetails {
        bank_id: Some(format!("{date}-{minor_units}")),
        booking_date: Some(date),
        value_date: date.tomorrow().ok(),
        other_party_name: Some(other_party.into()),
        other_party_iban: Some("DE89 3704 0044 0532 0130 00".into()),
        remittance_information: Some(format!("{other_party} \"card\" payment\nline two")),
    });
    add_transaction(
        book,
        date,
        None,
        vec![own, Split::uncategorized(eur(-minor_units))],
    );
}

fn rule(name: &str, conditions: Vec<Condition>, book_to: Option<BookTo>) -> Rule {
    Rule {
        id: RuleId::generate(),
        name: name.into(),
        match_mode: MatchMode::All,
        conditions,
        actions: Actions {
            book_to,
            payee: None,
        },
    }
}

fn text(field: TextField, operator: TextOperator, value: &str) -> Condition {
    Condition::Text {
        field,
        operator,
        value: value.into(),
        case_sensitive: false,
    }
}

/// A book with something of everything a book holds: settings, three
/// currencies, every kind of account, a closed one and a category tree,
/// rates, a part in another currency converted with a rate, what a bank
/// delivered, Rules with every kind of condition, and parts the Rules booked,
/// one of them by a Rule removed since.
fn household() -> Book {
    let mut book = Book::create_in_memory().expect("book created");
    book.set_rounding_mode(RoundingMode::HalfEven).expect("set");
    book.set_history_retention(HistoryRetention::Days(90))
        .expect("set");
    for currency in [EUR, USD, JPY] {
        book.use_currency(currency).expect("used");
    }
    let checking = add(
        &mut book,
        "Checking",
        AccountKind::Asset,
        Some(AccountType::Checking),
        EUR,
        None,
    );
    let card = add(
        &mut book,
        "Credit card",
        AccountKind::Liability,
        Some(AccountType::CreditCard),
        EUR,
        None,
    );
    let old = add(
        &mut book,
        "Old savings",
        AccountKind::Asset,
        Some(AccountType::Savings),
        EUR,
        None,
    );
    // The checking account is filled from the bank account with SWIFT's
    // example IBAN and was fetched once; the card is linked by its number
    // (M4.4a, D-148).
    book.link_bank_account(&BankLink {
        account: checking,
        iban: Some("DE89370400440532013000".into()),
        bank_code: None,
        number: None,
        last_fetched_on: None,
    })
    .expect("linked");
    book.note_bank_fetch(checking, day(20)).expect("fetched");
    book.link_bank_account(&BankLink {
        account: card,
        iban: None,
        bank_code: Some("37040044".into()),
        number: Some("4000 0000 0000 0001".into()),
        last_fetched_on: None,
    })
    .expect("linked");
    let cash = add(
        &mut book,
        "Dollars",
        AccountKind::Asset,
        Some(AccountType::Cash),
        USD,
        None,
    );
    let start = add(
        &mut book,
        "Starting balances",
        AccountKind::Equity,
        None,
        EUR,
        None,
    );
    let salary = add(&mut book, "Salary", AccountKind::Income, None, EUR, None);
    let food = add(&mut book, "Food", AccountKind::Expense, None, EUR, None);
    let groceries = add(
        &mut book,
        "Groceries",
        AccountKind::Expense,
        None,
        EUR,
        Some(food),
    );
    let bakery = add(
        &mut book,
        "Bakery",
        AccountKind::Expense,
        None,
        EUR,
        Some(groceries),
    );
    let travel = add(&mut book, "Travel", AccountKind::Expense, None, USD, None);

    let rate = DatedRate {
        id: DatedRateId::generate(),
        date: day(1),
        from: USD,
        to: EUR,
        rate: "0.9210".parse::<ExchangeRate>().expect("valid"),
        source: RateSource::Manual,
    };
    book.add_rate(&rate).expect("added");
    book.add_rate(&DatedRate {
        id: DatedRateId::generate(),
        date: day(2),
        from: EUR,
        to: USD,
        rate: "1.0856".parse::<ExchangeRate>().expect("valid"),
        source: RateSource::Manual,
    })
    .expect("added");

    add_transaction(
        &mut book,
        day(1),
        Some("Opening balance"),
        vec![
            Split::new(checking, eur(100_000)),
            Split::new(start, eur(-100_000)),
        ],
    );
    add_transaction(
        &mut book,
        day(1),
        None,
        vec![Split::new(old, eur(5_000)), Split::new(start, eur(-5_000))],
    );
    add_transaction(
        &mut book,
        day(2),
        None,
        vec![
            Split::new(old, eur(-5_000)),
            Split::new(checking, eur(5_000)),
        ],
    );
    book.close_account(old).expect("closed");
    add_transaction(
        &mut book,
        day(3),
        Some("Example Employer"),
        vec![
            Split::new(checking, eur(250_000)),
            Split::new(salary, eur(-250_000)),
        ],
    );
    // Dollars bought with euros, converted with the price table's rate; and
    // a hotel paid in dollars whose euro value the bank gave.
    let (euros, used) = book.convert(usd(10_000), EUR, day(4)).expect("converted");
    let mut dollars = Split::new(cash, euros);
    dollars.foreign = Some(ForeignAmount {
        amount: usd(10_000),
        rate: Some(used),
    });
    add_transaction(
        &mut book,
        day(4),
        Some("Exchange office"),
        vec![
            dollars,
            Split::new(checking, euros.checked_neg().expect("small")),
        ],
    );
    let mut hotel = Split::new(travel, eur(9_000));
    hotel.foreign = Some(ForeignAmount {
        amount: usd(9_800),
        rate: None,
    });
    add_transaction(
        &mut book,
        day(5),
        Some("Hotel"),
        vec![Split::new(card, eur(-9_000)), hotel],
    );
    // Several parts, and a part booked to the same account twice.
    add_transaction(
        &mut book,
        day(6),
        Some("Market"),
        vec![
            Split::new(checking, eur(-3_000)),
            Split::new(groceries, eur(2_000)),
            Split::new(bakery, eur(500)),
            Split::new(bakery, eur(500)),
        ],
    );

    // Rules with every kind of condition; one with a sub-Rule.
    let bread = rule(
        "Bread",
        vec![text(
            TextField::OtherPartyName,
            TextOperator::Contains,
            "Bakery",
        )],
        Some(BookTo::Category(bakery)),
    );
    let mut market = rule(
        "Market",
        vec![
            text(
                TextField::OtherPartyName,
                TextOperator::StartsWith,
                "Market",
            ),
            text(
                TextField::OtherPartyIban,
                TextOperator::Equals,
                "DE89370400440532013000",
            ),
            Condition::Text {
                field: TextField::RemittanceInformation,
                operator: TextOperator::Matches,
                value: "^Market.*$".into(),
                case_sensitive: true,
            },
            Condition::Amount(AmountTest::Between {
                min: eur(-5_000),
                max: eur(-100),
            }),
            Condition::Amount(AmountTest::MoneyOut),
            Condition::BookingDate {
                first: day(1),
                last: day(31),
            },
            Condition::Currency(EUR),
            Condition::Account(checking),
        ],
        Some(BookTo::Category(groceries)),
    );
    market.actions.payee = Some("Market".into());
    let organic = rule(
        "Organic",
        vec![
            text(TextField::Payee, TextOperator::EndsWith, "Organic"),
            Condition::Amount(AmountTest::Equals(eur(-999))),
            Condition::Amount(AmountTest::MoneyIn),
        ],
        None,
    );
    let mut transfer = rule(
        "To the card",
        vec![text(
            TextField::OtherPartyName,
            TextOperator::Contains,
            "Card bill",
        )],
        Some(BookTo::Transfer(card)),
    );
    transfer.match_mode = MatchMode::Any;
    let gone = rule(
        "Gone",
        vec![text(
            TextField::OtherPartyName,
            TextOperator::Contains,
            "Kiosk",
        )],
        Some(BookTo::Category(food)),
    );
    book.add_rule(&bread, None).expect("added");
    book.add_rule(&market, None).expect("added");
    book.add_rule(&organic, Some(market.id)).expect("added");
    book.add_rule(&transfer, None).expect("added");
    book.add_rule(&gone, None).expect("added");

    imported(&mut book, checking, -450, day(7), "Bakery Sunrise");
    imported(&mut book, checking, -1_250, day(8), "Kiosk");
    imported(&mut book, checking, -777, day(9), "Somebody else");
    book.run_rules().expect("ran");
    book.remove_rule(gone.id).expect("removed");

    // The card bill, paid from checking on the 20th and booked by the
    // card's bank on the 21st, joined into a Transfer: the card's part keeps
    // the date and payee the card showed (D-108).
    let paid = add_transaction(
        &mut book,
        day(20),
        Some("Example Card Services"),
        vec![
            Split::new(checking, eur(-9_840)),
            Split::uncategorized(eur(9_840)),
        ],
    );
    let received = add_transaction(
        &mut book,
        day(21),
        Some("Alex Example"),
        vec![
            Split::new(card, eur(9_840)),
            Split::uncategorized(eur(-9_840)),
        ],
    );
    book.join_transfer(paid.id(), received.id())
        .expect("joined");
    book
}

fn complete(book: &Book) -> BookData {
    match export_book(book).expect("gathered") {
        ExportData::Book(data) => data,
        _ => panic!("a book was gathered"),
    }
}

#[test]
fn a_book_comes_back_from_its_export_as_it_was() {
    let book = household();
    let data = complete(&book);
    // Everything is there: the marks of both Rules, one removed since.
    assert_eq!(data.currencies.len(), 3);
    assert_eq!(data.accounts.len(), 10);
    assert_eq!(data.rates.len(), 2);
    assert_eq!(data.transactions.len(), 11);
    let card_part = data
        .transactions
        .iter()
        .flat_map(|transaction| transaction.splits())
        .find(|part| part.own_date.is_some())
        .expect("the card's part of the Transfer");
    assert_eq!(
        (card_part.own_date, card_part.own_payee.as_deref()),
        (Some(day(21)), Some("Alex Example"))
    );
    assert_eq!(data.rule_marks.len(), 2);
    assert_eq!(
        data.rule_marks
            .iter()
            .filter(|mark| mark.rule.is_none())
            .count(),
        1
    );
    assert_eq!(data.bank_links, book.bank_links().expect("read"));
    assert_eq!(data.bank_links.len(), 2);
    assert_eq!(data.bank_links[0].last_fetched_on, Some(day(20)));

    let file = export(&BookExport, &ExportData::Book(data.clone())).expect("written");
    let read = read_book(&file).expect("read back");
    assert_eq!(read, data);

    let mut copy = Book::create_in_memory().expect("book created");
    fill_book(&mut copy, &read).expect("filled");
    // The new book holds the same: written again, it gives the same file.
    assert_eq!(complete(&copy), data);
    let again = export(&BookExport, &export_book(&copy).expect("gathered")).expect("written");
    assert_eq!(
        String::from_utf8(again).expect("UTF-8"),
        String::from_utf8(file).expect("UTF-8")
    );

    for account in book.all_accounts().expect("read") {
        assert_eq!(
            copy.balance(account.id).expect("balance"),
            book.balance(account.id).expect("balance"),
        );
        assert_eq!(
            copy.register(account.id).expect("register"),
            book.register(account.id).expect("register"),
        );
    }
    assert_eq!(
        copy.categories().expect("read"),
        book.categories().expect("read")
    );
    assert_eq!(copy.rules().expect("read"), book.rules().expect("read"));
    assert_eq!(
        copy.bank_links().expect("read"),
        book.bank_links().expect("read")
    );
    assert_eq!(copy.rounding_mode().expect("read"), RoundingMode::HalfEven);
    assert_eq!(
        copy.history_retention().expect("read"),
        HistoryRetention::Days(90)
    );
    // The new book starts without a history; changes are recorded from now.
    assert_eq!(copy.history().expect("read"), vec![]);
}

#[test]
fn a_book_that_keeps_its_history_forever_says_so() {
    let mut book = Book::create_in_memory().expect("book created");
    book.set_history_retention(HistoryRetention::Forever)
        .expect("set");
    let data = complete(&book);
    assert_eq!(data.history_keep_days, None);
    let mut copy = Book::create_in_memory().expect("book created");
    fill_book(&mut copy, &data).expect("filled");
    assert_eq!(
        copy.history_retention().expect("read"),
        HistoryRetention::Forever
    );
}

#[test]
fn only_a_new_book_is_filled() {
    let data = complete(&household());
    let mut book = Book::create_in_memory().expect("book created");
    book.use_currency(EUR).expect("used");
    assert!(matches!(
        fill_book(&mut book, &data),
        Err(StoreError::NotEmpty)
    ));
}

/// Fills a new book with `data` and returns the error, checking that the
/// book stayed empty.
fn refused(data: &BookData) -> StoreError {
    let mut book = Book::create_in_memory().expect("book created");
    let error = fill_book(&mut book, data).expect_err("refused");
    assert_eq!(book.currencies().expect("read"), vec![]);
    assert_eq!(book.all_accounts().expect("read"), vec![]);
    assert_eq!(book.history().expect("read"), vec![]);
    error
}

#[test]
fn what_a_book_would_refuse_is_refused_and_the_book_stays_empty() {
    let data = complete(&household());

    // A transaction booked to an account the file doesn't have.
    let mut broken = data.clone();
    let checking = broken.accounts[0].id;
    broken.accounts.remove(0);
    broken.rules.clear();
    assert!(matches!(refused(&broken), StoreError::UnknownAccount(id) if id == checking));

    // A closed account that still holds money.
    let mut broken = data.clone();
    let open = broken
        .accounts
        .iter_mut()
        .find(|account| account.name == "Checking")
        .expect("listed");
    open.closed = true;
    assert!(matches!(
        refused(&broken),
        StoreError::AccountNotEmpty { .. }
    ));

    // Categories inside each other in a loop.
    let mut broken = data.clone();
    let food = broken
        .accounts
        .iter()
        .find(|account| account.name == "Food")
        .expect("listed")
        .id;
    let bakery = broken
        .accounts
        .iter()
        .find(|account| account.name == "Bakery")
        .expect("listed")
        .id;
    broken
        .accounts
        .iter_mut()
        .find(|account| account.id == food)
        .expect("listed")
        .parent = Some(bakery);
    assert!(matches!(
        refused(&broken),
        StoreError::CategoryInsideItself(_)
    ));

    // Two accounts with one name.
    let mut broken = data.clone();
    broken.accounts[1].name = broken.accounts[0].name.to_uppercase();
    assert!(matches!(
        refused(&broken),
        StoreError::DuplicateAccountName { .. }
    ));

    // A part in another currency than its account.
    let mut broken = data;
    let dollars = broken
        .accounts
        .iter_mut()
        .find(|account| account.name == "Dollars")
        .expect("listed");
    dollars.currency = JPY;
    assert!(matches!(
        refused(&broken),
        StoreError::SplitCurrencyMismatch { .. }
    ));
}
