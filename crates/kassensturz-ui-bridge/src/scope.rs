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
    /// The IDs of the tags only whose parts count (US5.2, D-714); empty for
    /// every part. Always the component's own: the filter bar has none.
    tags: Vec<String>,
    /// The dimensions its dataset takes.
    takes: Takes,
    /// The series with a filter of their own (US1.16, D-646).
    series: Vec<SeriesScope>,
    /// What spending is averaged per (US1.13, D-648): a `params.average`
    /// word, empty for the total.
    pub average: String,
    /// What a spending chart compares each category with (US3.1a, D-687): a
    /// `params.compare` word, empty for no comparison.
    pub compare: String,
    /// How many days a candle chart's candle covers (Q-498): a
    /// `params.candle` word, empty for the length that follows the period.
    pub candle: String,
}

/// The own lists of one series of a chart (D-646): empty for a dimension it
/// takes from the component.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeriesScope {
    /// The series' fixed name: `income` or `expenses`.
    series: String,
    /// Its own accounts.
    accounts: Vec<String>,
    /// Its own categories and groups.
    categories: Vec<String>,
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            period_own: false,
            accounts: Vec::new(),
            accounts_own: false,
            categories: Vec::new(),
            categories_own: false,
            tags: Vec::new(),
            takes: Takes::ALL,
            series: Vec::new(),
            average: String::new(),
            compare: String::new(),
            candle: String::new(),
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
            tags: ids(&asked.tags),
            takes: asked.takes,
            series: asked
                .series
                .iter()
                .map(|series| SeriesScope {
                    series: series.series.to_owned(),
                    accounts: ids(&series.accounts),
                    categories: ids(&series.categories),
                })
                .collect(),
            average: asked
                .average
                .map(|per| per.word().to_owned())
                .unwrap_or_default(),
            compare: asked
                .compare
                .map(|compare| compare.word().to_owned())
                .unwrap_or_default(),
            candle: asked
                .candle
                .map(|length| length.key().to_owned())
                .unwrap_or_default(),
        }
    }

    /// Whether a series has a filter of its own (D-646).
    pub fn has_series(&self) -> bool {
        !self.series.is_empty()
    }

    /// The scope the series `series` is counted with (D-646): this, with
    /// the series' own lists in place of the component's; its own lists
    /// are marked as the component's own.
    pub fn for_series(&self, series: &str) -> Self {
        let mut scope = Self {
            series: Vec::new(),
            ..self.clone()
        };
        if let Some(own) = self.series.iter().find(|own| own.series == series) {
            if !own.accounts.is_empty() {
                scope.accounts.clone_from(&own.accounts);
                scope.accounts_own = true;
            }
            if !own.categories.is_empty() {
                scope.categories.clone_from(&own.categories);
                scope.categories_own = true;
            }
        }
        scope
    }

    /// The scope as the text QML passes on.
    pub fn to_text(&self) -> String {
        json!({
            "periodOwn": self.period_own,
            "accounts": self.accounts,
            "accountsOwn": self.accounts_own,
            "categories": self.categories,
            "categoriesOwn": self.categories_own,
            "tags": self.tags,
            "takes": {
                "period": self.takes.period,
                "accounts": self.takes.accounts,
                "categories": self.takes.categories,
                "tags": self.takes.tags,
            },
            "series": self
                .series
                .iter()
                .map(|series| json!({
                    "series": series.series,
                    "accounts": series.accounts,
                    "categories": series.categories,
                }))
                .collect::<Vec<_>>(),
            "average": self.average,
            "compare": self.compare,
            "candle": self.candle,
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
        let ids_in = |value: &Value, key: &str| -> Vec<String> {
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
        let ids = |key: &str| ids_in(&value, key);
        let series = value
            .get("series")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|entry| {
                        Some(SeriesScope {
                            series: entry.get("series")?.as_str()?.to_owned(),
                            accounts: ids_in(entry, "accounts"),
                            categories: ids_in(entry, "categories"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let takes = value.get("takes").cloned().unwrap_or(Value::Null);
        Self {
            period_own: flag(&value, "periodOwn", false),
            accounts: ids("accounts"),
            accounts_own: flag(&value, "accountsOwn", false),
            categories: ids("categories"),
            categories_own: flag(&value, "categoriesOwn", false),
            tags: ids("tags"),
            takes: Takes {
                period: flag(&takes, "period", true),
                accounts: flag(&takes, "accounts", true),
                categories: flag(&takes, "categories", true),
                tags: flag(&takes, "tags", true),
            },
            series,
            average: value
                .get("average")
                .and_then(Value::as_str)
                .filter(|word| kassensturz_report::Per::read(word).is_some())
                .unwrap_or_default()
                .to_owned(),
            compare: value
                .get("compare")
                .and_then(Value::as_str)
                .filter(|word| kassensturz_report::CompareWith::read(word).is_some())
                .unwrap_or_default()
                .to_owned(),
            candle: value
                .get("candle")
                .and_then(Value::as_str)
                .filter(|word| kassensturz_report::CandleLength::read(word).is_some())
                .unwrap_or_default()
                .to_owned(),
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
            tags: chosen(&self.tags, true),
            takes: self.takes,
            // Each series is applied on its own (`for_series`).
            series: Vec::new(),
            // An average, a comparison and a candle length change no filter.
            average: None,
            compare: None,
            candle: None,
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
    /// `not-applied`, `gone`, `chooses-nothing` or `average-early`.
    pub kind: &'static str,
    /// `period`, `accounts` or `categories`; `period` for `average-early`.
    pub dimension: &'static str,
    /// For `gone`, the ID as the list writes it; for `average-early`, the
    /// `params.average` word (`per-month`, `per-quarter`, `per-year`); else
    /// empty.
    pub id: String,
    /// For `average-early` (M6-B207, #590, D-668): how many months
    /// (quarters, years) the average divides by; else zero.
    pub counted: u32,
    /// For `average-early`: how many of them lie before the first
    /// transaction of the accounts and categories it counts; else zero.
    pub before: u32,
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
    /// The names of the tags only whose parts count, at most
    /// [`NAMES_SHOWN`] (US5.2).
    pub tags: Vec<String>,
    /// How many tags there are beyond those named.
    pub tags_more: usize,
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
        let tag_names = if kept(Dimension::Tags) {
            let tags = book.tags().unwrap_or_default();
            scope
                .tags
                .iter()
                .filter_map(|id| {
                    tags.iter()
                        .find(|tag| tag.id.to_string().eq_ignore_ascii_case(id))
                        .map(|tag| tag.name.clone())
                })
                .collect()
        } else {
            Vec::new()
        };
        let (accounts, accounts_more) = shown(account_names);
        let (categories, categories_more) = shown(category_names);
        let (tags, tags_more) = shown(tag_names);
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
                        counted: 0,
                        before: 0,
                    },
                    FilterNote::Gone { dimension, id } => NoteView {
                        kind: "gone",
                        dimension: dimension.word(),
                        id: id.clone(),
                        counted: 0,
                        before: 0,
                    },
                    FilterNote::ChoosesNothing(dimension) => NoteView {
                        kind: "chooses-nothing",
                        dimension: dimension.word(),
                        id: String::new(),
                        counted: 0,
                        before: 0,
                    },
                })
                .collect(),
            accounts,
            accounts_more,
            categories,
            categories_more,
            tags,
            tags_more,
            nothing: applied.chooses_nothing(),
        }
    }

    /// The view of a chart whose two series count with filters of their
    /// own (D-646): the marks of both, each dimension once in the order
    /// period, accounts, categories; the notes of both, each once; the names
    /// of both lists. Nothing is drawn only when both choose nothing; a
    /// series whose own lists choose nothing counts zero.
    pub fn merge(self, other: Self) -> Self {
        let order = ["period", "accounts", "categories", "tags"];
        let union = |a: &[&'static str], b: &[&'static str]| -> Vec<&'static str> {
            order
                .into_iter()
                .filter(|word| a.contains(word) || b.contains(word))
                .collect()
        };
        let names = |a: Vec<String>, a_more: usize, b: Vec<String>, b_more: usize| {
            let mut all = a;
            for name in b {
                if !all.contains(&name) {
                    all.push(name);
                }
            }
            let more = a_more + b_more + all.len().saturating_sub(NAMES_SHOWN);
            all.truncate(NAMES_SHOWN);
            (all, more)
        };
        let mut notes = self.notes;
        for note in other.notes {
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
        let (accounts, accounts_more) = names(
            self.accounts,
            self.accounts_more,
            other.accounts,
            other.accounts_more,
        );
        let (categories, categories_more) = names(
            self.categories,
            self.categories_more,
            other.categories,
            other.categories_more,
        );
        let (tags, tags_more) = names(self.tags, self.tags_more, other.tags, other.tags_more);
        Self {
            own: union(&self.own, &other.own),
            filtered: union(&self.filtered, &other.filtered),
            notes,
            accounts,
            accounts_more,
            categories,
            categories_more,
            tags,
            tags_more,
            nothing: self.nothing && other.nothing,
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
                        (
                            "counted",
                            count(usize::try_from(note.counted).unwrap_or(usize::MAX)),
                        ),
                        (
                            "before",
                            count(usize::try_from(note.before).unwrap_or(usize::MAX)),
                        ),
                    ])
                })),
            ),
            ("accounts", names(&self.accounts)),
            ("accountsMore", count(self.accounts_more)),
            ("categories", names(&self.categories)),
            ("categoriesMore", count(self.categories_more)),
            ("tags", names(&self.tags)),
            ("tagsMore", count(self.tags_more)),
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
    use crate::variant::read_back;

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
        salary: AccountId,
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
            salary,
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
            tags: vec!["t-1".into()],
            takes: Takes::NO_ACCOUNTS,
            series: vec![
                SeriesScope {
                    series: "income".into(),
                    accounts: Vec::new(),
                    categories: vec!["c-2".into()],
                },
                SeriesScope {
                    series: "expenses".into(),
                    accounts: vec!["a-3".into()],
                    categories: Vec::new(),
                },
            ],
            average: "per-month".into(),
            compare: "same_last_year".into(),
            candle: "week".into(),
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
    fn the_tags_a_scope_names_count_only_the_tagged_parts_and_are_listed_by_their_names() {
        let mut f = fixture();
        let holiday = kassensturz_core::TagId::generate();
        f.book
            .add_tag(&kassensturz_core::Tag {
                id: holiday,
                name: "Holiday".into(),
            })
            .expect("tag added");
        // 30,00 € of a 50,00 € bill tagged on its part, from the cash box.
        let food = account(&mut f.book, "Food", AccountKind::Expense);
        let transaction = Transaction::new(
            TransactionId::generate(),
            Date::new(2026, 9, 4).expect("a date"),
            None,
            vec![
                Split::new(f.cash, Money::from_minor_units(-5_000, EUR)),
                Split::new(food, Money::from_minor_units(2_000, EUR)),
                Split {
                    tags: vec![holiday],
                    ..Split::new(f.rent, Money::from_minor_units(3_000, EUR))
                },
            ],
        )
        .expect("balanced");
        f.book.add_transaction(&transaction).expect("added");
        let tagged = Scope {
            tags: vec![holiday.to_string()],
            ..Scope::default()
        };
        // The text QML passes on keeps the tags.
        let tagged = Scope::from_text(&tagged.to_text());
        let (shown, view) = expenses(&f.book, tagged);
        assert_eq!(shown, ["30,00 €"]);
        assert_eq!(view.own, ["tags"]);
        assert_eq!(view.filtered, ["tags"]);
        assert_eq!(view.tags, ["Holiday"]);
        // The object QML reads, read back the way QML reads it.
        let object = view.object();
        let names: Vec<String> = read_back::list_of(&object, "tags")
            .iter()
            .filter_map(|name| name.value::<cxx_qt_lib::QString>())
            .map(|name| name.to_string())
            .collect();
        assert_eq!(names, ["Holiday"]);
        assert_eq!(read_back::number_of(&object, "tagsMore"), 0);
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
            budgets::bars_filtered(&f.book, &TestLocale, "last-month", &[], &[], &chosen)
                .expect("bars");
        assert!(bars.is_empty());
        assert!(view.filtered.is_empty(), "the accounts were not applied");
        let notes: Vec<(&str, &str)> = view
            .notes
            .iter()
            .map(|note| (note.kind, note.dimension))
            .collect();
        assert_eq!(notes, [("not-applied", "accounts")]);
    }

    /// Money in and money out of the chart `component` (a JSON component of
    /// `income_expense_by_month`) for September 2026, with the filter bar's
    /// accounts `bar`, read through the scope QML passes on; and the view.
    fn money_in_and_out(
        f: &Fixture,
        component: Value,
        bar: &[AccountId],
    ) -> (i64, i64, FilterView) {
        let component = kassensturz_report::Component::from_value(&component).expect("component");
        let bar = Filters {
            period: None,
            accounts: bar.iter().map(ToString::to_string).collect(),
            categories: Vec::new(),
        };
        let asked = Asked::new(&component, None, &bar).expect("asks");
        let request = ChartRequest {
            dataset: "income_expense_by_month".into(),
            kind: "bar".into(),
            period: "last-month".into(),
            grouping: "each".into(),
            scope: Scope::from_text(&Scope::from_asked(&asked).to_text()),
            ..ChartRequest::default()
        };
        let set = charts::charts(&f.book, &TestLocale, &request).expect("charts");
        let model = &set.charts[0].0;
        let total = |key: &str| -> i64 {
            model
                .series
                .iter()
                .filter(|series| series.key == key)
                .flat_map(|series| series.points.iter())
                .map(|point| point.minor_units)
                .sum()
        };
        (total("income"), total("expenses"), set.filter)
    }

    #[test]
    fn each_series_of_money_in_and_out_counts_with_its_own_lists() {
        // A seam test: the component's own lists go through the real Asked,
        // the scope text QML passes on, and the chart (US1.16, D-646).
        let f = fixture();
        let plain = json!({"type": "chart.bar", "dataset": "income_expense_by_month"});
        let (income, expenses, view) = money_in_and_out(&f, plain, &[]);
        assert_eq!((income, expenses), (200_000, 85_000));
        assert!(view.filtered.is_empty());
        // Money out from the cash box only; money in as before.
        let own = json!({
            "type": "chart.bar", "dataset": "income_expense_by_month",
            "params": {"expenses_accounts": [f.cash.to_string()]}
        });
        let (income, expenses, view) = money_in_and_out(&f, own.clone(), &[]);
        assert_eq!((income, expenses), (200_000, 5_000));
        assert_eq!(view.own, ["accounts"]);
        assert_eq!(view.accounts, ["Cash box"]);
        // The filter bar's accounts still narrow money in, which has no
        // list of its own; money out keeps its own list.
        let (income, expenses, _) = money_in_and_out(&f, own, &[f.cash]);
        assert_eq!((income, expenses), (0, 5_000));
        // Money in from the salary only and money out from the checking
        // account only: the two lists of the goal of US1.16.
        let both = json!({
            "type": "chart.bar", "dataset": "income_expense_by_month",
            "params": {
                "income_categories": [f.salary.to_string()],
                "expenses_accounts": [f.checking.to_string()]
            }
        });
        let (income, expenses, view) = money_in_and_out(&f, both, &[]);
        assert_eq!((income, expenses), (200_000, 80_000));
        assert_eq!(view.own, ["accounts", "categories"]);
        assert_eq!(view.categories, ["Salary"]);
    }

    #[test]
    fn a_series_whose_list_chooses_nothing_counts_zero_and_the_other_is_drawn() {
        let f = fixture();
        let gone = json!({
            "type": "chart.bar", "dataset": "income_expense_by_month",
            "params": {"income_categories": [AccountId::generate().to_string()]}
        });
        let (income, expenses, view) = money_in_and_out(&f, gone, &[]);
        assert_eq!((income, expenses), (0, 85_000));
        assert!(!view.nothing);
        let kinds: Vec<&str> = view.notes.iter().map(|note| note.kind).collect();
        assert!(kinds.contains(&"gone"), "{kinds:?}");
    }

    #[test]
    fn spending_shown_as_an_average_is_divided_by_the_months_up_to_today() {
        // A seam test (US1.13, D-648): the component's `average` goes
        // through the real Asked and the scope text to the chart. This
        // year up to 19 October 2026 (`TestLocale`) is 292 days: 10 months.
        let f = fixture();
        let component = kassensturz_report::Component::from_value(&json!({
            "type": "chart.donut", "dataset": "spending_by_category",
            "params": {"average": "per-month"}
        }))
        .expect("component");
        assert_eq!(component.to_value()["params"]["average"], "per-month");
        let asked = Asked::new(&component, None, &Filters::default()).expect("asks");
        let scope = Scope::from_text(&Scope::from_asked(&asked).to_text());
        assert_eq!(scope.average, "per-month");
        let request = ChartRequest {
            dataset: "spending_by_category".into(),
            kind: "donut".into(),
            period: "this-year".into(),
            grouping: "each".into(),
            scope,
            ..ChartRequest::default()
        };
        let set = charts::charts(&f.book, &TestLocale, &request).expect("charts");
        let model = &set.charts[0].0;
        assert_eq!(model.total, "85,00 €");
        assert_eq!(model.table.rows[0], ["Rent", "80,00 €"]);
        assert_eq!(model.table.columns, ["category", "per_month"]);
        // The total is the default and is not written.
        let total = kassensturz_report::Component::from_value(&json!({
            "type": "chart.donut", "dataset": "spending_by_category",
            "params": {"average": "total"}
        }))
        .expect("component");
        assert!(total.to_value()["params"].get("average").is_none());
    }

    #[test]
    fn a_comparison_goes_from_the_component_through_the_scope_text() {
        // A seam test (US3.1a, D-687): the component's `compare` goes
        // through the real Asked and the scope text to the chart.
        let component = kassensturz_report::Component::from_value(&json!({
            "type": "chart.bar", "dataset": "spending_by_category",
            "params": {"compare": "average-3"}
        }))
        .expect("component");
        let asked = Asked::new(&component, None, &Filters::default()).expect("asks");
        let scope = Scope::from_text(&Scope::from_asked(&asked).to_text());
        assert_eq!(scope.compare, "average-3");
        // A word that is not a comparison is not passed on.
        assert_eq!(Scope::from_text(r#"{"compare": "average-5"}"#).compare, "");
        // No comparison is the default and is not written.
        let none = kassensturz_report::Component::from_value(&json!({
            "type": "chart.bar", "dataset": "spending_by_category",
            "params": {"compare": "none"}
        }))
        .expect("component");
        assert!(none.to_value()["params"].get("compare").is_none());
        let asked = Asked::new(&none, None, &Filters::default()).expect("asks");
        assert_eq!(Scope::from_asked(&asked).compare, "");
    }

    #[test]
    fn a_candle_length_goes_from_the_component_through_the_scope_text() {
        // A seam test (Q-498): the candle chart's `candle` goes through the
        // real Asked and the scope text to the chart.
        let component = kassensturz_report::Component::from_value(&json!({
            "type": "chart.candlestick", "dataset": "cash_flow_by_month",
            "params": {"candle": "month"}
        }))
        .expect("component");
        let asked = Asked::new(&component, None, &Filters::default()).expect("asks");
        let scope = Scope::from_text(&Scope::from_asked(&asked).to_text());
        assert_eq!(scope.candle, "month");
        // The automatic length is the default, not written and not passed on.
        let auto = kassensturz_report::Component::from_value(&json!({
            "type": "chart.candlestick", "dataset": "cash_flow_by_month",
            "params": {"candle": "auto"}
        }))
        .expect("component");
        assert!(auto.to_value()["params"].get("candle").is_none());
        let asked = Asked::new(&auto, None, &Filters::default()).expect("asks");
        assert_eq!(Scope::from_asked(&asked).candle, "");
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
