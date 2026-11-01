// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a component is asked for besides its dataset: the shared filters of
//! the dashboard (M6c.7, D-350, D-351, D-416). No Qt in this file.
//!
//! `kassensturz-report` works out, for each component of a layout, what it
//! asks for ([`Asked`]): the period, and the accounts and categories its own
//! settings or the filter bar choose. The layout hands QML the part the
//! datasets need as one text, a [`Scope`]; QML passes it unread to the object
//! that draws the component (`BookCharts`, `BookCards`, `BookFormula`,
//! `BookBudgets`), which checks it against the book ([`Scope::apply`]) and
//! gives back the figures with a [`FilterView`]: the marks and notes the
//! component shows. QML maps nothing of this itself (D-350).

use kassensturz_report::{
    Applied, Asked, Chosen, Dimension, Filter, FilterNote, Origin, ReportError, Source, Takes,
};
use kassensturz_store::Book;
use serde_json::{Value, json};

use crate::variant::{list, map, text};
use cxx_qt_lib::QVariant;

/// The most names a mark lists before it counts the rest.
const NAMES_SHOWN: usize = 4;

/// The filters one component is drawn with, as the layout works them out.
/// The default narrows nothing and takes every dimension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    /// Whether the component's own setting chose its period.
    period_own: bool,
    /// The IDs of the accounts it is narrowed to; empty for all.
    accounts: Vec<String>,
    /// Whether the accounts are the component's own.
    accounts_own: bool,
    /// The IDs of the categories and groups it is narrowed to; empty for all.
    categories: Vec<String>,
    /// Whether the categories are the component's own.
    categories_own: bool,
    /// The dimensions its dataset takes.
    takes: Takes,
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            period_own: false,
            accounts: Vec::new(),
            accounts_own: false,
            categories: Vec::new(),
            categories_own: false,
            takes: Takes::ALL,
        }
    }
}

impl Scope {
    /// What `asked` narrows.
    pub fn from_asked(asked: &Asked) -> Self {
        let ids = |chosen: &Option<Chosen>| {
            chosen
                .as_ref()
                .map_or_else(Vec::new, |chosen| chosen.ids.clone())
        };
        let own = |chosen: &Option<Chosen>| {
            chosen
                .as_ref()
                .is_some_and(|chosen| chosen.origin == Origin::Own)
        };
        Self {
            period_own: asked.period_origin == Origin::Own,
            accounts: ids(&asked.accounts),
            accounts_own: own(&asked.accounts),
            categories: ids(&asked.categories),
            categories_own: own(&asked.categories),
            takes: asked.takes,
        }
    }

    /// The scope as the text QML passes on.
    pub fn to_text(&self) -> String {
        json!({
            "periodOwn": self.period_own,
            "accounts": self.accounts,
            "accountsOwn": self.accounts_own,
            "categories": self.categories,
            "categoriesOwn": self.categories_own,
            "takes": {
                "period": self.takes.period,
                "accounts": self.takes.accounts,
                "categories": self.takes.categories,
            },
        })
        .to_string()
    }

    /// The scope a text says. Text that is empty or is not such a text gives
    /// the default scope: nothing is narrowed, and nothing is hidden by it.
    pub fn from_text(text: &str) -> Self {
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        let flag = |value: &Value, key: &str, otherwise: bool| {
            value.get(key).and_then(Value::as_bool).unwrap_or(otherwise)
        };
        let ids = |key: &str| -> Vec<String> {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let takes = value.get("takes").cloned().unwrap_or(Value::Null);
        Self {
            period_own: flag(&value, "periodOwn", false),
            accounts: ids("accounts"),
            accounts_own: flag(&value, "accountsOwn", false),
            categories: ids("categories"),
            categories_own: flag(&value, "categoriesOwn", false),
            takes: Takes {
                period: flag(&takes, "period", true),
                accounts: flag(&takes, "accounts", true),
                categories: flag(&takes, "categories", true),
            },
        }
    }

    /// Checks the IDs against `source` and works out the [`Filter`] the
    /// dataset is asked with, and the marks and notes of the component
    /// (`report.filter-invalid` for a list that cannot narrow anything).
    pub fn apply(&self, source: &Source) -> Result<Applied, ReportError> {
        let chosen = |ids: &[String], own: bool| {
            (!ids.is_empty()).then(|| Chosen {
                ids: ids.to_vec(),
                origin: if own { Origin::Own } else { Origin::FilterBar },
            })
        };
        Asked {
            // The period is worked out by the layout; only the mark of an
            // own period is kept here.
            period: "page".to_owned(),
            period_origin: if self.period_own {
                Origin::Own
            } else {
                Origin::Default
            },
            accounts: chosen(&self.accounts, self.accounts_own),
            categories: chosen(&self.categories, self.categories_own),
            takes: self.takes,
        }
        .apply(source)
    }

    /// What the dataset is asked with, and what the component says about it.
    pub fn resolve(
        &self,
        book: &Book,
        source: &Source,
    ) -> Result<(Filter, FilterView), ReportError> {
        let applied = self.apply(source)?;
        let view = FilterView::of(book, self, &applied);
        Ok((applied.filter, view))
    }
}

/// A note under a component about its filter, by fixed words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteView {
    /// `not-applied`, `gone` or `chooses-nothing`.
    pub kind: &'static str,
    /// `period`, `accounts` or `categories`.
    pub dimension: &'static str,
    /// For `gone`, the ID as the list writes it; else empty.
    pub id: String,
}

/// The marks and notes a component shows about its filter (D-350).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterView {
    /// The dimensions that come from the component's own setting: the *Own
    /// filter* mark names them.
    pub own: Vec<&'static str>,
    /// The dimensions a filter narrows the figures by: the *Filtered* mark.
    pub filtered: Vec<&'static str>,
    /// The notes, in the order period, accounts, categories.
    pub notes: Vec<NoteView>,
    /// The names of the accounts the figures are narrowed to, at most
    /// [`NAMES_SHOWN`].
    pub accounts: Vec<String>,
    /// How many accounts there are beyond those named.
    pub accounts_more: usize,
    /// The names of the categories, at most [`NAMES_SHOWN`].
    pub categories: Vec<String>,
    /// How many categories there are beyond those named.
    pub categories_more: usize,
    /// Whether the filter chooses nothing, so nothing is drawn.
    pub nothing: bool,
}

impl FilterView {
    /// The marks and notes of `applied`, with the names of what the lists of
    /// `scope` keep read from `book`.
    pub fn of(book: &Book, scope: &Scope, applied: &Applied) -> Self {
        let words = |dimensions: &[Dimension]| dimensions.iter().map(|d| d.word()).collect();
        let shown = |names: Vec<String>| -> (Vec<String>, usize) {
            let more = names.len().saturating_sub(NAMES_SHOWN);
            (names.into_iter().take(NAMES_SHOWN).collect(), more)
        };
        let kept = |dimension: Dimension| applied.filtered.contains(&dimension);
        // The names of what the lists in the scope keep, as the user knows
        // them: an account's name, a category's path, a group's name.
        let account_names = if kept(Dimension::Accounts) {
            let accounts = book.accounts(true).unwrap_or_default();
            scope
                .accounts
                .iter()
                .filter_map(|id| {
                    accounts
                        .iter()
                        .find(|account| account.id.to_string().eq_ignore_ascii_case(id))
                        .map(|account| account.name.clone())
                })
                .collect()
        } else {
            Vec::new()
        };
        let category_names = if kept(Dimension::Categories) {
            let categories = book.categories().unwrap_or_default();
            let groups = book.category_groups().unwrap_or_default();
            scope
                .categories
                .iter()
                .filter_map(|id| {
                    groups
                        .iter()
                        .find(|group| group.id.to_string().eq_ignore_ascii_case(id))
                        .map(|group| group.name.clone())
                        .or_else(|| {
                            categories
                                .iter()
                                .find(|entry| {
                                    entry.category.id.to_string().eq_ignore_ascii_case(id)
                                })
                                .map(|entry| entry.path.join(" › "))
                        })
                })
                .collect()
        } else {
            Vec::new()
        };
        let (accounts, accounts_more) = shown(account_names);
        let (categories, categories_more) = shown(category_names);
        Self {
            own: words(&applied.own),
            filtered: words(&applied.filtered),
            notes: applied
                .notes
                .iter()
                .map(|note| match note {
                    FilterNote::NotApplied(dimension) => NoteView {
                        kind: "not-applied",
                        dimension: dimension.word(),
                        id: String::new(),
                    },
                    FilterNote::Gone { dimension, id } => NoteView {
                        kind: "gone",
                        dimension: dimension.word(),
                        id: id.clone(),
                    },
                    FilterNote::ChoosesNothing(dimension) => NoteView {
                        kind: "chooses-nothing",
                        dimension: dimension.word(),
                        id: String::new(),
                    },
                })
                .collect(),
            accounts,
            accounts_more,
            categories,
            categories_more,
            nothing: applied.chooses_nothing(),
        }
    }

    /// The object QML reads as `filter`.
    pub fn object(&self) -> QVariant {
        let words = |words: &[&'static str]| list(words.iter().map(|word| text(word)));
        let names = |names: &[String]| list(names.iter().map(|name| text(name)));
        let count = |count: usize| QVariant::from(&i32::try_from(count).unwrap_or(i32::MAX));
        map([
            ("own", words(&self.own)),
            ("filtered", words(&self.filtered)),
            (
                "notes",
                list(self.notes.iter().map(|note| {
                    map([
                        ("kind", text(note.kind)),
                        ("dimension", text(note.dimension)),
                        ("id", text(&note.id)),
                    ])
                })),
            ),
            ("accounts", names(&self.accounts)),
            ("accountsMore", count(self.accounts_more)),
            ("categories", names(&self.categories)),
            ("categoriesMore", count(self.categories_more)),
            ("nothing", QVariant::from(&self.nothing)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{
        Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Split,
        Transaction, TransactionId,
    };
    use kassensturz_report::Filters;

    use super::*;
    use crate::budgets;
    use crate::cards::{self, Request as CardRequest};
    use crate::charts::{self, Request as ChartRequest};
    use crate::formulas::{self, Outcome, Request as FormulaRequest};
    use crate::register::tests::TestLocale;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn account(book: &mut Book, name: &str, kind: AccountKind) -> AccountId {
        let id = AccountId::generate();
        book.add_account(&Account {
            id,
            name: name.into(),
            kind,
            currency: EUR,
            account_type: (kind == AccountKind::Asset).then_some(AccountType::Checking),
            parent: None,
            closed: false,
            number: None,
            notes: None,
            institution: None,
        })
        .expect("account added");
        id
    }

    /// Two accounts of the user's: rent (800,00) is paid from the checking
    /// account and groceries (50,00) from the cash box, both in September
    /// 2026 (last month for `TestLocale`), and 2000,00 of salary arrives in
    /// the checking account.
    struct Fixture {
        book: Book,
        checking: AccountId,
        cash: AccountId,
        rent: AccountId,
    }

    fn fixture() -> Fixture {
        let mut book = Book::create_in_memory().expect("book created");
        book.use_currency(EUR).expect("currency used");
        let checking = account(&mut book, "Checking account", AccountKind::Asset);
        let cash = account(&mut book, "Cash box", AccountKind::Asset);
        let rent = account(&mut book, "Rent", AccountKind::Expense);
        let groceries = account(&mut book, "Groceries", AccountKind::Expense);
        let salary = account(&mut book, "Salary", AccountKind::Income);
        let mut add = |from: AccountId, to: AccountId, minor: i64| {
            let transaction = Transaction::new(
                TransactionId::generate(),
                Date::new(2026, 9, 3).expect("a date"),
                None,
                vec![
                    Split::new(from, Money::from_minor_units(-minor, EUR)),
                    Split::new(to, Money::from_minor_units(minor, EUR)),
                ],
            )
            .expect("balanced");
            book.add_transaction(&transaction).expect("added");
        };
        add(salary, checking, 200_000);
        add(checking, rent, 80_000);
        add(cash, groceries, 5_000);
        Fixture {
            book,
            checking,
            cash,
            rent,
        }
    }

    fn scope(accounts: &[AccountId], categories: &[AccountId]) -> Scope {
        Scope {
            accounts: accounts.iter().map(ToString::to_string).collect(),
            categories: categories.iter().map(ToString::to_string).collect(),
            ..Scope::default()
        }
    }

    fn expenses(book: &Book, scope: Scope) -> (Vec<String>, FilterView) {
        let request = CardRequest {
            dataset: String::new(),
            period: "last-month".into(),
            compare: "previous_period".into(),
            scope,
        };
        let (sets, view) = cards::cards_filtered(book, &TestLocale, &request).expect("cards");
        let shown = sets
            .iter()
            .flat_map(|set| set.cards.iter())
            .filter(|card| card.measure == "expenses")
            .map(|card| card.exact_value.clone())
            .collect();
        (shown, view)
    }

    #[test]
    fn a_scope_reads_back_what_it_was_written_as() {
        let scope = Scope {
            period_own: true,
            accounts: vec!["a-1".into(), "a-2".into()],
            accounts_own: false,
            categories: vec!["c-1".into()],
            categories_own: true,
            takes: Takes::NO_ACCOUNTS,
        };
        assert_eq!(Scope::from_text(&scope.to_text()), scope);
    }

    #[test]
    fn text_that_is_no_scope_narrows_nothing_and_hides_nothing() {
        for text in ["", "nonsense", "[]", "{\"takes\": 3}"] {
            assert_eq!(Scope::from_text(text), Scope::default(), "{text}");
        }
    }

    #[test]
    fn the_accounts_a_scope_names_narrow_the_figures_and_the_view_says_so() {
        let f = fixture();
        let (all, quiet) = expenses(&f.book, Scope::default());
        assert_eq!(all, ["850,00 €"]);
        assert!(quiet.filtered.is_empty() && quiet.notes.is_empty());
        let (cash, view) = expenses(&f.book, scope(&[f.cash], &[]));
        assert_eq!(cash, ["50,00 €"]);
        assert_eq!(view.filtered, ["accounts"]);
        assert_eq!(view.accounts, ["Cash box"]);
        // The checking account paid the rent and received the salary.
        let (checking, view) = expenses(&f.book, scope(&[f.checking], &[]));
        assert_eq!(checking, ["800,00 €"]);
        assert_eq!(view.accounts, ["Checking account"]);
    }

    #[test]
    fn the_categories_a_scope_names_narrow_the_figures_and_are_listed_by_their_names() {
        let f = fixture();
        let (rent, view) = expenses(&f.book, scope(&[], &[f.rent]));
        assert_eq!(rent, ["800,00 €"]);
        assert_eq!(view.filtered, ["categories"]);
        assert_eq!(view.categories, ["Rent"]);
        // An own list is marked as the component's own.
        let mut own = scope(&[], &[f.rent]);
        own.categories_own = true;
        assert_eq!(expenses(&f.book, own).1.own, ["categories"]);
    }

    #[test]
    fn a_filter_that_chooses_nothing_draws_no_zeros_and_names_what_is_gone() {
        let f = fixture();
        let mut gone = scope(&[], &[]);
        gone.categories = vec![AccountId::generate().to_string()];
        let (shown, view) = expenses(&f.book, gone.clone());
        assert!(shown.is_empty(), "no card, so no zero: {shown:?}");
        assert!(view.nothing);
        let kinds: Vec<&str> = view.notes.iter().map(|note| note.kind).collect();
        assert_eq!(kinds, ["gone", "chooses-nothing"]);
        // The same for a chart and for a formula.
        let request = ChartRequest {
            dataset: "spending_by_category".into(),
            kind: "pie".into(),
            period: "last-month".into(),
            grouping: "each".into(),
            scope: gone.clone(),
            ..ChartRequest::default()
        };
        let set = charts::charts(&f.book, &TestLocale, &request).expect("charts");
        assert!(set.charts.is_empty() && set.filter.nothing);
        let formula = FormulaRequest {
            period: "last-month".into(),
            text: "=Rent".into(),
            scope: gone,
            ..FormulaRequest::default()
        };
        assert!(matches!(
            formulas::show(&f.book, &TestLocale, &formula),
            Ok(Outcome::Nothing(_))
        ));
    }

    #[test]
    fn the_accounts_narrow_a_chart_and_a_formula_by_the_same_rule() {
        let f = fixture();
        let chart = |scope: Scope| {
            let request = ChartRequest {
                dataset: "spending_by_category".into(),
                kind: "bar".into(),
                period: "last-month".into(),
                grouping: "each".into(),
                scope,
                ..ChartRequest::default()
            };
            let set = charts::charts(&f.book, &TestLocale, &request).expect("charts");
            let model = &set.charts[0].0;
            format!("{model:?}")
        };
        assert!(chart(Scope::default()).contains("Rent"));
        let cash_only = chart(scope(&[f.cash], &[]));
        assert!(cash_only.contains("Groceries") && !cash_only.contains("Rent"));
        let formula = |scope: Scope| {
            let request = FormulaRequest {
                period: "last-month".into(),
                text: "=Rent".into(),
                scope,
                ..FormulaRequest::default()
            };
            match formulas::show(&f.book, &TestLocale, &request).expect("formula") {
                Outcome::Shown(shown) => shown.exact_value,
                other => panic!("not a figure: {other:?}"),
            }
        };
        assert_eq!(formula(Scope::default()), "800,00 €");
        assert_eq!(formula(scope(&[f.checking], &[])), "800,00 €");
        assert_eq!(formula(scope(&[f.cash], &[])), "0,00 €");
    }

    #[test]
    fn budgets_count_every_account_and_say_so_when_the_accounts_are_chosen() {
        let f = fixture();
        let mut chosen = scope(&[f.cash], &[]);
        chosen.takes = Takes::NO_ACCOUNTS;
        let (bars, view) =
            budgets::bars_filtered(&f.book, &TestLocale, "last-month", &[], &chosen).expect("bars");
        assert!(bars.is_empty());
        assert!(view.filtered.is_empty(), "the accounts were not applied");
        let notes: Vec<(&str, &str)> = view
            .notes
            .iter()
            .map(|note| (note.kind, note.dimension))
            .collect();
        assert_eq!(notes, [("not-applied", "accounts")]);
    }

    #[test]
    fn what_the_layout_asks_for_is_what_the_datasets_are_asked_with() {
        // The scope the layout hands QML for a component is read by the
        // models: a seam test with the real Asked of the report layer.
        let f = fixture();
        let document = kassensturz_report::Document::default_overview().expect("default");
        let bar = Filters {
            period: Some("last-year".into()),
            accounts: vec![f.cash.to_string()],
            categories: Vec::new(),
        };
        let income = document
            .asked("income", &bar)
            .expect("the income card asks");
        assert_eq!(income.period, "last-year");
        let scope = Scope::from_asked(&income);
        assert!(!scope.accounts_own && !scope.period_own);
        let (shown, view) = expenses(&f.book, Scope::from_text(&scope.to_text()));
        assert_eq!(shown, ["50,00 €"]);
        assert_eq!(view.accounts, ["Cash box"]);
    }
}
