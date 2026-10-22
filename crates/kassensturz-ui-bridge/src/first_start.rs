// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The first start (M3.4, D-111, D-116): which currency the household uses,
//! the default categories written in the user's language (D-029, D-067), and
//! whether the first-run steps are done.
//!
//! The screens read the open book directly, through the public functions of
//! core and store. The names of the default categories come from QML, where
//! KI18n translates them (D-034); once written they are ordinary data of the
//! book and aren't translated again (D-067).

use kassensturz_core::{Account, AccountId, AccountKind, CurrencyCode, Report};
use kassensturz_store::{Action, Book, StoreError};

use crate::accounts;
use crate::errors::{ErrorText, report_text};
use crate::format::Locale;

/// Where a book stands at the start of the app.
#[derive(Debug, PartialEq, Eq)]
pub struct FirstStart {
    /// Whether the book has an account, so the first-run steps are done and
    /// the screens are offered.
    pub has_account: bool,
    /// Whether the first start still has to ask which currency the household
    /// uses: the book has neither accounts nor categories.
    pub needs_currency: bool,
    /// The currencies to offer, the one to preselect first: the household's
    /// usual one, or the regional settings' currency in a new book
    /// (`accounts::currency_choices`).
    pub currencies: Vec<CurrencyCode>,
}

/// One category of the default set, as QML passes it in the user's language.
#[derive(Debug, PartialEq, Eq)]
pub struct DefaultCategory<'a> {
    /// Its name.
    pub name: &'a str,
    /// The name of the category it sits inside, which comes earlier in the
    /// list and is of the same kind; empty for a top-level category.
    pub parent: &'a str,
    /// Whether it is for money received rather than spent.
    pub income: bool,
}

/// Why the first start couldn't be completed. The UI shows a message for
/// each code (D-034).
#[derive(Debug)]
pub enum FirstStartError {
    /// The currency isn't one KassenSturz lists.
    UnknownCurrency,
    /// A default category names a parent that isn't earlier in the list, or
    /// is of the other kind. The list is the app's own, so this can only be
    /// a mistake in it.
    NoSuchParent(String),
    /// The book refused it.
    Store(StoreError),
}

impl From<StoreError> for FirstStartError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Where `book` stands. `regional` is the regional settings' currency, or
/// the euro if KassenSturz doesn't know it (D-111).
pub fn state(book: &Book, regional: CurrencyCode) -> Result<FirstStart, StoreError> {
    let all = book.all_accounts()?;
    let has_account = all.iter().any(|account| account.kind.is_account());
    let has_category = all.iter().any(|account| account.kind.is_category());
    Ok(FirstStart {
        has_account,
        needs_currency: !has_account && !has_category,
        currencies: accounts::currency_choices(book, regional)?,
    })
}

/// Completes the first start: the household's `currency` is the book's first
/// one, and the default `categories` are written in that currency, in the
/// order given, with each inside its parent. All of it is one user action
/// (D-032), so a book never has half the set. A book that has categories
/// already keeps them and gets none of these: the first start happened on
/// another day, or the book came from an import.
pub fn start(
    book: &mut Book,
    currency: &str,
    categories: &[DefaultCategory<'_>],
) -> Result<(), FirstStartError> {
    let currency = CurrencyCode::new(currency)
        .ok()
        .filter(|currency| currency.iso_minor_digits().is_some())
        .ok_or(FirstStartError::UnknownCurrency)?;
    // Checked before anything is written, so a mistake in the list leaves
    // the book as it was.
    for (index, category) in categories.iter().enumerate() {
        if !category.parent.is_empty()
            && !categories[..index]
                .iter()
                .any(|earlier| earlier.income == category.income && earlier.name == category.parent)
        {
            return Err(FirstStartError::NoSuchParent(category.parent.to_owned()));
        }
    }
    let has_category = book
        .all_accounts()?
        .iter()
        .any(|account| account.kind.is_category());
    if has_category {
        return Ok(());
    }
    book.as_one_action(Action::AddCategory, |book| -> Result<(), StoreError> {
        book.use_currency(currency)?;
        // The IDs of what was written so far, to find a parent by name.
        let mut written: Vec<(&DefaultCategory<'_>, AccountId)> = Vec::new();
        for category in categories {
            let parent = written
                .iter()
                .find(|(earlier, _)| {
                    earlier.income == category.income && earlier.name == category.parent
                })
                .map(|(_, id)| *id);
            let id = AccountId::generate();
            book.add_account(&Account {
                id,
                name: category.name.trim().to_owned(),
                kind: if category.income {
                    AccountKind::Income
                } else {
                    AccountKind::Expense
                },
                currency,
                account_type: None,
                closed: false,
                parent,
            })?;
            written.push((category, id));
        }
        Ok(())
    })?;
    Ok(())
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &FirstStartError, locale: &impl Locale) -> ErrorText {
    match error {
        FirstStartError::UnknownCurrency => ErrorText::plain("input.unknown-currency"),
        FirstStartError::NoSuchParent(parent) => ErrorText {
            code: "input.no-such-parent",
            params: vec![("parent", parent.clone())],
        },
        FirstStartError::Store(error) => report_text(book, &error.report(), locale),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::categories;
    use crate::register::tests::TestLocale;
    use kassensturz_core::AccountType;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    /// A small default set in the shape of the real one: a tree of both
    /// kinds, the parents before what sits inside them.
    pub(crate) fn defaults() -> Vec<DefaultCategory<'static>> {
        vec![
            income("Salary"),
            income("Other income"),
            spending("Food", ""),
            spending("Groceries", "Food"),
            spending("Eating out", "Food"),
            spending("Rent", ""),
            spending("Clothes", ""),
        ]
    }

    fn income(name: &'static str) -> DefaultCategory<'static> {
        DefaultCategory {
            name,
            parent: "",
            income: true,
        }
    }

    fn spending(name: &'static str, parent: &'static str) -> DefaultCategory<'static> {
        DefaultCategory {
            name,
            parent,
            income: false,
        }
    }

    /// Does the first start on a new book in euros, with [`defaults`].
    pub(crate) fn started() -> Book {
        let mut book = Book::create_in_memory().expect("book in memory");
        start(&mut book, "EUR", &defaults()).expect("started");
        book
    }

    fn paths(book: &Book) -> Vec<String> {
        book.categories()
            .expect("categories")
            .into_iter()
            .map(|entry| entry.path.join(" › "))
            .collect()
    }

    fn code(book: &Book, error: &FirstStartError) -> ErrorText {
        error_text(book, error, &TestLocale)
    }

    #[test]
    fn a_new_book_asks_for_the_currency_and_offers_the_regional_one_first() {
        let book = Book::create_in_memory().expect("book in memory");
        let chf = CurrencyCode::new("CHF").expect("valid code");
        let state = state(&book, chf).expect("state");
        assert!(!state.has_account);
        assert!(state.needs_currency);
        assert_eq!(state.currencies[0], chf);
        assert!(state.currencies.contains(&EUR));
    }

    #[test]
    fn writes_the_default_categories_as_a_tree_in_the_chosen_currency() {
        let mut book = Book::create_in_memory().expect("book in memory");
        let chf = CurrencyCode::new("CHF").expect("valid code");
        start(&mut book, "CHF", &defaults()).expect("started");

        // The categories screen lists them as a tree (D-082).
        assert_eq!(
            paths(&book),
            [
                "Other income",
                "Salary",
                "Clothes",
                "Food",
                "Food › Eating out",
                "Food › Groceries",
                "Rent"
            ]
        );
        for entry in book.categories().expect("categories") {
            assert_eq!(entry.category.currency, chf);
        }
        let view = categories::view(&book).expect("view");
        let groceries = view
            .rows
            .iter()
            .find(|row| row.name == "Groceries")
            .expect("groceries listed");
        assert_eq!(groceries.depth, 1);
        assert!(!groceries.income);

        // The currency is chosen: a new account and a new category are
        // offered in it, before any account exists (D-115, D-116).
        let state = state(&book, EUR).expect("state");
        assert!(!state.has_account);
        assert!(!state.needs_currency);
        assert_eq!(state.currencies[0], chf);
        assert_eq!(accounts::usual_currency(&book, EUR).expect("usual"), chf);
        // It is in one user action, so undo takes the whole set back.
        assert_eq!(book.history().expect("history").len(), 1);
        book.undo().expect("undone");
        assert!(paths(&book).is_empty());
    }

    #[test]
    fn the_first_account_ends_the_first_start() {
        let mut book = started();
        add_account(
            &mut book,
            &NewAccount {
                name: "Checking account",
                account_type: AccountType::Checking,
                currency: "EUR",
                balance: "1.000,00",
                date: "",
            },
            &TestLocale,
        )
        .expect("account added");
        let state = state(&book, EUR).expect("state");
        assert!(state.has_account);
        assert!(!state.needs_currency);
        assert_eq!(state.currencies[0], EUR);
    }

    #[test]
    fn a_book_with_categories_keeps_them() {
        let mut book = started();
        // Opened again in another language: nothing is added twice.
        let german = [
            income("Gehalt"),
            spending("Lebensmittel", ""),
            spending("Supermarkt", "Lebensmittel"),
        ];
        start(&mut book, "EUR", &german).expect("started again");
        assert_eq!(paths(&book).len(), 7);
        assert!(!paths(&book).iter().any(|path| path == "Gehalt"));
        assert_eq!(book.history().expect("history").len(), 1);
    }

    #[test]
    fn refuses_a_currency_it_does_not_know_and_a_list_that_is_wrong() {
        let mut book = Book::create_in_memory().expect("book in memory");
        let error = start(&mut book, "XTS", &defaults()).expect_err("test code refused");
        assert_eq!(code(&book, &error).code, "input.unknown-currency");
        let error = start(&mut book, "euro", &defaults()).expect_err("not a code");
        assert_eq!(code(&book, &error).code, "input.unknown-currency");

        // A parent that comes later, or is of the other kind.
        let later = [spending("Groceries", "Food"), spending("Food", "")];
        let error = start(&mut book, "EUR", &later).expect_err("parent comes later");
        assert_eq!(
            code(&book, &error),
            ErrorText {
                code: "input.no-such-parent",
                params: vec![("parent", "Food".into())],
            }
        );
        let other_kind = [income("Food"), spending("Groceries", "Food")];
        let error = start(&mut book, "EUR", &other_kind).expect_err("parent of the other kind");
        assert_eq!(code(&book, &error).code, "input.no-such-parent");

        // A name the book refuses: nothing of the set is written.
        let twice = [spending("Rent", ""), spending("rent", "")];
        let error = start(&mut book, "EUR", &twice).expect_err("name taken");
        assert_eq!(code(&book, &error).code, "store.duplicate-account-name");
        assert!(paths(&book).is_empty());
        assert!(state(&book, EUR).expect("state").needs_currency);
        assert!(book.history().expect("history").is_empty());
    }
}
