// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The categories screen (M3.3): the category tree with adding, renaming and
//! moving (D-082, D-067).
//!
//! Like the accounts screen it reads the open book directly and names
//! categories to QML by their ID as text. Categories are listed in the order
//! of the book's tree: income before spending, each category followed by
//! what is inside it, siblings by name.

use kassensturz_core::{Account, AccountId, AccountKind, CurrencyCode, Report};
use kassensturz_store::{Book, StoreError};

use crate::errors::{ErrorText, report_text};
use crate::format::Locale;

/// Why something the user entered wasn't accepted. The UI shows a message for
/// each code (D-034).
#[derive(Debug)]
pub enum CategoryError {
    /// The currency isn't one KassenSturz lists for new categories.
    UnknownCurrency,
    /// There is no such category, e.g. it was just changed elsewhere.
    NoSuchEntry,
    /// The book refused it, e.g. a name that is taken next to it.
    Store(StoreError),
}

impl From<StoreError> for CategoryError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// One category in the tree.
#[derive(Debug, PartialEq, Eq)]
pub struct CategoryRow {
    /// The category's ID as text, to name it to QML.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The names from the top-level category down to this one, joined, e.g.
    /// "Food › Groceries".
    pub path: String,
    /// How deep it sits: 0 for a top-level category.
    pub depth: usize,
    /// The ID of the category it sits inside as text; empty for a top-level
    /// one.
    pub parent: String,
    /// Whether it is an income category, otherwise a spending one.
    pub income: bool,
    /// Its currency.
    pub currency: String,
}

/// Where a category can be put.
#[derive(Debug, PartialEq, Eq)]
pub struct ParentChoice {
    /// The category's ID as text.
    pub id: String,
    /// Its path, e.g. "Food › Groceries".
    pub path: String,
    /// How deep it sits, for indenting the list.
    pub depth: usize,
}

/// A category as the user entered it.
pub struct NewCategory<'a> {
    /// The name, as typed.
    pub name: &'a str,
    /// Whether it is an income category, otherwise a spending one. Ignored
    /// if it goes inside another category, which decides.
    pub income: bool,
    /// The ID of the category it goes inside; empty for the top level.
    pub parent: &'a str,
    /// The ISO 4217 code of its currency; empty for the household's usual
    /// one. Ignored if it goes inside another category, which decides.
    pub currency: &'a str,
}

/// The whole tree, and whether to say which currency each category counts in.
pub struct CategoryView {
    /// The categories in tree order.
    pub rows: Vec<CategoryRow>,
    /// Whether the book has categories in more than one currency, so the
    /// screen names the currency.
    pub several_currencies: bool,
}

/// The category tree of `book`.
pub fn view(book: &Book) -> Result<CategoryView, StoreError> {
    let rows: Vec<CategoryRow> = book
        .categories()?
        .into_iter()
        .map(|entry| CategoryRow {
            id: entry.category.id.to_string(),
            name: entry.category.name.clone(),
            path: entry.path.join(" › "),
            depth: entry.path.len() - 1,
            parent: entry
                .category
                .parent
                .map(|parent| parent.to_string())
                .unwrap_or_default(),
            income: entry.category.kind == AccountKind::Income,
            currency: entry.category.currency.to_string(),
        })
        .collect();
    let several_currencies = rows.iter().any(|row| row.currency != rows[0].currency);
    Ok(CategoryView {
        rows,
        several_currencies,
    })
}

/// The categories a category of the given kind can go inside, in tree order.
/// With `moving`, the category that is to be moved: it and everything inside
/// it are left out, and so are categories in another currency than it.
pub fn parent_choices(
    book: &Book,
    income: bool,
    moving: &str,
) -> Result<Vec<ParentChoice>, CategoryError> {
    let kind = if income {
        AccountKind::Income
    } else {
        AccountKind::Expense
    };
    let entries = book.categories()?;
    let moved = if moving.is_empty() {
        None
    } else {
        let found = entries
            .iter()
            .find(|entry| entry.category.id.to_string() == moving)
            .ok_or(CategoryError::NoSuchEntry)?;
        Some((found.category.id, found.category.currency, found.path.len()))
    };
    let mut choices = Vec::new();
    // While inside the moved category's subtree, everything is left out.
    let mut skipping_below: Option<usize> = None;
    for entry in &entries {
        let depth = entry.path.len();
        if let Some(below) = skipping_below {
            if depth > below {
                continue;
            }
            skipping_below = None;
        }
        if let Some((id, currency, moved_depth)) = moved {
            if entry.category.id == id {
                skipping_below = Some(moved_depth);
                continue;
            }
            if entry.category.currency != currency {
                continue;
            }
        }
        if entry.category.kind == kind {
            choices.push(ParentChoice {
                id: entry.category.id.to_string(),
                path: entry.path.join(" › "),
                depth: depth - 1,
            });
        }
    }
    Ok(choices)
}

/// Adds a category.
pub fn add_category(
    book: &mut Book,
    new: &NewCategory<'_>,
    usual_currency: CurrencyCode,
) -> Result<AccountId, CategoryError> {
    let parent = if new.parent.is_empty() {
        None
    } else {
        Some(find(book, new.parent)?)
    };
    let (kind, currency) = match &parent {
        Some(parent) => (parent.kind, parent.currency),
        None => {
            let currency = if new.currency.is_empty() {
                usual_currency
            } else {
                CurrencyCode::new(new.currency)
                    .ok()
                    .filter(|currency| currency.iso_minor_digits().is_some())
                    .ok_or(CategoryError::UnknownCurrency)?
            };
            let kind = if new.income {
                AccountKind::Income
            } else {
                AccountKind::Expense
            };
            (kind, currency)
        }
    };
    let id = AccountId::generate();
    book.as_one_action(kassensturz_store::Action::AddCategory, |book| {
        book.use_currency(currency)?;
        book.add_account(&Account {
            id,
            name: new.name.trim().to_owned(),
            kind,
            currency,
            account_type: None,
            closed: false,
            parent: parent.map(|parent| parent.id),
        })
    })?;
    Ok(id)
}

/// Gives a category a new name.
pub fn rename_category(book: &mut Book, id: &str, name: &str) -> Result<(), CategoryError> {
    let category = find(book, id)?;
    Ok(book.rename_account(category.id, name.trim())?)
}

/// Moves a category, with everything inside it and its transactions, inside
/// `parent`, or to the top level if `parent` is empty.
pub fn move_category(book: &mut Book, id: &str, parent: &str) -> Result<(), CategoryError> {
    let category = find(book, id)?;
    let parent = if parent.is_empty() {
        None
    } else {
        Some(find(book, parent)?.id)
    };
    Ok(book.move_category(category.id, parent)?)
}

/// The category (not account) the UI means by `id`.
fn find(book: &Book, id: &str) -> Result<Account, CategoryError> {
    book.all_accounts()?
        .into_iter()
        .find(|account| account.kind.is_category() && account.id.to_string() == id)
        .ok_or(CategoryError::NoSuchEntry)
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &CategoryError, locale: &impl Locale) -> ErrorText {
    match error {
        CategoryError::UnknownCurrency => ErrorText::plain("input.unknown-currency"),
        CategoryError::NoSuchEntry => ErrorText::plain("input.no-such-entry"),
        CategoryError::Store(error) => report_text(book, &error.report(), locale),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::first_run::FirstRun;
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use kassensturz_core::AccountType;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn book() -> Book {
        Book::create_in_memory().expect("book in memory")
    }

    fn add(book: &mut Book, name: &str, income: bool, parent: &str) -> String {
        let new = NewCategory {
            name,
            income,
            parent,
            currency: "",
        };
        add_category(book, &new, EUR)
            .expect("category added")
            .to_string()
    }

    fn paths(book: &Book) -> Vec<String> {
        view(book)
            .expect("view")
            .rows
            .iter()
            .map(|row| format!("{}{}", "  ".repeat(row.depth), row.name))
            .collect()
    }

    fn code(book: &Book, error: &CategoryError) -> &'static str {
        error_text(book, error, &TestLocale).code
    }

    /// Food, Groceries and Restaurants inside it, Rent, and Salary.
    fn household(book: &mut Book) -> [String; 5] {
        let food = add(book, "Food", false, "");
        let groceries = add(book, "Groceries", false, &food);
        let restaurants = add(book, "Restaurants", false, &food);
        let rent = add(book, "Rent", false, "");
        let salary = add(book, "Salary", true, "");
        [food, groceries, restaurants, rent, salary]
    }

    #[test]
    fn lists_the_tree_income_first_and_each_category_before_what_is_inside() {
        let mut book = book();
        household(&mut book);
        assert_eq!(
            paths(&book),
            ["Salary", "Food", "  Groceries", "  Restaurants", "Rent"]
        );
        let shown = view(&book).expect("view");
        assert_eq!(shown.rows[2].path, "Food › Groceries");
        assert!(shown.rows[0].income && !shown.rows[1].income);
        assert_eq!(shown.rows[2].parent, shown.rows[1].id);
        assert_eq!(shown.rows[1].parent, "");
        assert_eq!(shown.rows[0].currency, "EUR");
        assert!(!shown.several_currencies);
    }

    #[test]
    fn a_category_inside_another_takes_its_kind_and_currency() {
        let mut book = book();
        let [food, ..] = household(&mut book);
        // Ignores what the form says when it goes inside Food.
        let new = NewCategory {
            name: "Snacks",
            income: true,
            parent: &food,
            currency: "USD",
        };
        add_category(&mut book, &new, EUR).expect("added");
        let snacks = view(&book)
            .expect("view")
            .rows
            .into_iter()
            .find(|row| row.name == "Snacks")
            .expect("category");
        assert_eq!(
            (snacks.income, snacks.currency.as_str(), snacks.depth),
            (false, "EUR", 1)
        );

        // At the top level it counts in the currency chosen, if any.
        let new = NewCategory {
            name: "Holiday",
            income: false,
            parent: "",
            currency: "USD",
        };
        add_category(&mut book, &new, EUR).expect("added");
        assert!(view(&book).expect("view").several_currencies);
    }

    #[test]
    fn refuses_what_the_book_refuses_with_a_reason() {
        let mut book = book();
        let [food, groceries, ..] = household(&mut book);
        let mut refuse = |name: &str, income: bool, parent: &str, currency: &str| {
            let new = NewCategory {
                name,
                income,
                parent,
                currency,
            };
            let error = add_category(&mut book, &new, EUR).expect_err("refused");
            code(&book, &error)
        };
        assert_eq!(refuse("  ", false, "", ""), "store.empty-account-name");
        // The same name next to it, but not elsewhere.
        assert_eq!(
            refuse("food", false, "", ""),
            "store.duplicate-account-name"
        );
        assert_eq!(
            refuse("GROCERIES", false, &food, ""),
            "store.duplicate-account-name"
        );
        assert_eq!(
            refuse("Groceries", true, "", "XXX"),
            "input.unknown-currency"
        );
        assert_eq!(refuse("Groceries", true, "gone", ""), "input.no-such-entry");
        // Another category is no place for this one if it isn't a category.
        assert_eq!(
            refuse("Other", false, &groceries.to_uppercase(), ""),
            "input.no-such-entry"
        );
        add(&mut book, "Groceries", true, "");
    }

    #[test]
    fn renames_a_category_and_everything_inside_keeps_its_place() {
        let mut book = book();
        let [food, groceries, ..] = household(&mut book);
        rename_category(&mut book, &food, " Eating ").expect("renamed");
        assert_eq!(
            paths(&book),
            ["Salary", "Eating", "  Groceries", "  Restaurants", "Rent"]
        );
        let error = rename_category(&mut book, &groceries, "restaurants").expect_err("taken");
        assert_eq!(code(&book, &error), "store.duplicate-account-name");
        let error = rename_category(&mut book, &groceries, "").expect_err("empty");
        assert_eq!(code(&book, &error), "store.empty-account-name");
    }

    #[test]
    fn moves_a_category_with_what_is_inside_it() {
        let mut book = book();
        let [food, groceries, _, rent, salary] = household(&mut book);
        let home = add(&mut book, "Home", false, "");
        move_category(&mut book, &food, &home).expect("moved");
        assert_eq!(
            paths(&book),
            [
                "Salary",
                "Home",
                "  Food",
                "    Groceries",
                "    Restaurants",
                "Rent"
            ]
        );
        move_category(&mut book, &rent, &home).expect("moved");
        move_category(&mut book, &groceries, "").expect("moved to the top");
        assert_eq!(
            paths(&book),
            [
                "Salary",
                "Groceries",
                "Home",
                "  Food",
                "    Restaurants",
                "  Rent"
            ]
        );

        // Not inside itself or what is inside it, not into another kind.
        let error = move_category(&mut book, &home, &home).expect_err("itself");
        assert_eq!(code(&book, &error), "store.category-inside-itself");
        let error = move_category(&mut book, &home, &food).expect_err("below itself");
        assert_eq!(code(&book, &error), "store.category-inside-itself");
        let error = move_category(&mut book, &home, &salary).expect_err("other kind");
        assert_eq!(code(&book, &error), "store.parent-kind-mismatch");
        // A name that is taken where it would go.
        let other = add(&mut book, "Food", false, "");
        let error = move_category(&mut book, &other, &home).expect_err("taken");
        assert_eq!(code(&book, &error), "store.duplicate-account-name");
    }

    #[test]
    fn offers_only_places_the_category_can_go() {
        let mut book = book();
        let [food, groceries, restaurants, rent, salary] = household(&mut book);
        let offered = |book: &Book, income: bool, moving: &str| -> Vec<String> {
            parent_choices(book, income, moving)
                .expect("choices")
                .into_iter()
                .map(|choice| choice.path)
                .collect()
        };
        // For a new category: every one of its kind, in tree order.
        assert_eq!(
            offered(&book, false, ""),
            ["Food", "Food › Groceries", "Food › Restaurants", "Rent"]
        );
        assert_eq!(offered(&book, true, ""), ["Salary"]);
        // For Food: itself and what is inside it are left out.
        assert_eq!(offered(&book, false, &food), ["Rent"]);
        assert_eq!(
            offered(&book, false, &groceries),
            ["Food", "Food › Restaurants", "Rent"]
        );
        assert_eq!(
            offered(&book, false, &rent),
            ["Food", "Food › Groceries", "Food › Restaurants"]
        );
        let choices = parent_choices(&book, false, &restaurants).expect("choices");
        assert_eq!(choices[0].depth, 0);
        assert_eq!(choices[1].depth, 1);
        // Another currency than the category's is no place for it.
        let new = NewCategory {
            name: "Holiday",
            income: false,
            parent: "",
            currency: "USD",
        };
        add_category(&mut book, &new, EUR).expect("added");
        assert_eq!(
            offered(&book, false, &rent),
            ["Food", "Food › Groceries", "Food › Restaurants"]
        );
        assert_eq!(offered(&book, false, "").len(), 5);
        assert!(parent_choices(&book, false, "gone").is_err());
        assert!(
            parent_choices(&book, true, &salary)
                .expect("choices")
                .is_empty()
        );
    }

    #[test]
    fn only_categories_are_found_by_their_id() {
        let mut book = book();
        let account = add_account(
            &mut book,
            &NewAccount {
                name: "Checking account",
                account_type: AccountType::Checking,
                currency: "EUR",
                balance: "",
                date: "",
            },
            &TestLocale,
        )
        .expect("account added")
        .to_string();
        for id in [account.as_str(), "nonsense"] {
            let error = rename_category(&mut book, id, "Anything").expect_err("not a category");
            assert_eq!(code(&book, &error), "input.no-such-entry");
            assert!(move_category(&mut book, id, "").is_err());
        }
        let food = add(&mut book, "Food", false, "");
        let error = move_category(&mut book, &food, &account).expect_err("not a category");
        assert_eq!(code(&book, &error), "input.no-such-entry");
    }

    #[test]
    fn the_categories_of_the_first_start_are_in_the_tree_and_can_be_changed() {
        let mut book = started();
        assert_eq!(paths(&book)[..3], ["Other income", "Salary", "Clothes"]);

        // Changes made here reach the mock the next time it loads.
        let groceries = view(&book)
            .expect("view")
            .rows
            .iter()
            .find(|row| row.name == "Groceries")
            .expect("groceries")
            .id
            .clone();
        let home = add(&mut book, "Home", false, "");
        move_category(&mut book, &groceries, &home).expect("moved");
        rename_category(&mut book, &groceries, "Supermarket").expect("renamed");
        let mock = FirstRun::load(&book, EUR).expect("mock loaded");
        assert_eq!(
            mock.category_names(false),
            [
                "Food",
                "Home › Supermarket",
                "Food › Eating out",
                "Rent",
                "Clothes",
                "Home"
            ]
        );
    }
}
