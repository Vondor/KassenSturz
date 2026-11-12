// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a dataset leaves out, and the shared filters of a dashboard (M6c.6,
//! D-350, D-351).
//!
//! A [`Filter`] is what a dataset function takes. The shared filters are
//! where it comes from: for each component the report layer works out, per
//! dimension (period, accounts, categories, tags), which setting applies,
//! first match wins:
//!
//! 1. the component's own setting (`props.period` other than `page`,
//!    `params.categories`, `params.accounts`, `params.tags`);
//! 2. for the period only, the period box the component lies in (D-333);
//! 3. the dashboard's filter bar ([`Filters`]: the document's `filters`, or
//!    a choice the user made for now); it has no tags (US5.2);
//! 4. nothing: all accounts, all categories, every part whatever its tags,
//!    the page's own period.
//!
//! [`Asked`] is that choice, before the book is consulted;
//! [`Asked::apply`] checks its IDs against the book and gives the [`Filter`]
//! with the marks and notes the component shows ([`Applied`]). A dimension
//! the dataset does not take is not applied, and a note says so, never
//! silently. QML maps nothing of this itself.

use std::collections::HashSet;

use kassensturz_core::{AccountId, GroupMember, Split, TagId, Transaction};

use crate::{Component, ComponentType, ReportError, Setting, Source};

/// Narrows a dataset to some accounts, some categories and some tags. `None`
/// means no narrowing; `Some` with an empty list means nothing is chosen, so
/// the dataset is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// Only transactions with a part booked to one of these accounts of the
    /// user's. A transaction counts as a whole: if it books money out of
    /// two accounts and only one is chosen, all its categories still count.
    pub accounts: Option<Vec<AccountId>>,
    /// Only parts booked to these categories or their subcategories. Money
    /// not sorted into a category yet is left out, since it is in none.
    pub categories: Option<Vec<AccountId>>,
    /// Only parts that carry one of these tags, their own or the whole
    /// transaction's (D-713). This is the one place a sum by tag is counted
    /// (D-714): only category-side parts ever count (Income, Expense or no
    /// category yet), each once, in the transaction's currency, so a tag on
    /// the whole transaction never counts its account side, and a Transfer,
    /// which has none, counts nothing. A part with two of the tags counts
    /// once.
    pub tags: Option<Vec<TagId>>,
}

/// Whether `part` of `transaction` counts for one of `tags` in a sum by tag
/// (D-713, D-714): it carries one of them itself, or the whole transaction
/// does. Whether the part counts at all (the category side only) is the
/// caller's: [`Filter::tags`] in the datasets, and the AI's totals by tag
/// (D-744) use this same check, so the two never differ.
pub fn carries_tag(transaction: &Transaction, part: &Split, tags: &[TagId]) -> bool {
    transaction
        .tags()
        .iter()
        .chain(&part.tags)
        .any(|tag| tags.contains(tag))
}

/// The most IDs in one list of a filter: the filter bar's, or a
/// component's own accounts (D-350).
pub const MAX_FILTER_IDS: usize = 200;

/// One thing a filter narrows, by the fixed word documents and notes use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dimension {
    /// `period`: the days counted.
    Period,
    /// `accounts`: the user's own accounts whose money counts.
    Accounts,
    /// `categories`: the categories (or My groups) whose money counts.
    Categories,
    /// `tags`: the tags whose parts count (US5.2, D-714).
    Tags,
}

impl Dimension {
    /// The fixed word: `period`, `accounts`, `categories` or `tags`.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Period => "period",
            Self::Accounts => "accounts",
            Self::Categories => "categories",
            Self::Tags => "tags",
        }
    }
}

/// The dimensions a dataset can honour without making a figure wrong
/// (D-350). A dataset that counts every account (a budget) does not take
/// accounts; one that counts accounts' balances does not take categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Takes {
    /// The period; every dataset takes it.
    pub period: bool,
    /// Which accounts count.
    pub accounts: bool,
    /// Which categories count.
    pub categories: bool,
    /// Which tags count (US5.2): only the datasets made of the parts that
    /// count as income or spending, never a budget's or a balance.
    pub tags: bool,
}

impl Takes {
    /// Period, accounts, categories and tags.
    pub const ALL: Self = Self {
        period: true,
        accounts: true,
        categories: true,
        tags: true,
    };
    /// Period and accounts: the datasets of balances, which count accounts
    /// and no categories.
    pub const NO_CATEGORIES: Self = Self {
        period: true,
        accounts: true,
        categories: false,
        tags: false,
    };
    /// Period and categories: budgets, which count their category's money
    /// from every account, tagged or not.
    pub const NO_ACCOUNTS: Self = Self {
        period: true,
        accounts: false,
        categories: true,
        tags: false,
    };
    /// Nothing: a component that shows no figures.
    pub const NOTHING: Self = Self {
        period: false,
        accounts: false,
        categories: false,
        tags: false,
    };

    /// Whether `dimension` is one of them.
    pub const fn takes(self, dimension: Dimension) -> bool {
        match dimension {
            Dimension::Period => self.period,
            Dimension::Accounts => self.accounts,
            Dimension::Categories => self.categories,
            Dimension::Tags => self.tags,
        }
    }
}

/// What a dashboard's filter bar holds: the document's `filters` (D-351),
/// or what the user chose there for now. An empty list narrows nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    /// A period word ([`crate::is_period_word`]): a fixed name, a rolling
    /// period (D-644) or a typed range (D-332), if chosen.
    pub period: Option<String>,
    /// The IDs of own accounts, written as text.
    pub accounts: Vec<String>,
    /// The IDs of categories or My groups, written as text.
    pub categories: Vec<String>,
}

/// Where the value of one dimension comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The component's own setting.
    Own,
    /// The period box the component lies in (period only).
    PeriodBox,
    /// The dashboard's filter bar.
    FilterBar,
    /// Nothing chose it: the page's period, all accounts, all categories,
    /// every tag.
    Default,
}

/// A list of IDs a dimension is narrowed to, and where it comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chosen {
    /// The IDs, written as text; never empty.
    pub ids: Vec<String>,
    /// [`Origin::Own`] or [`Origin::FilterBar`].
    pub origin: Origin,
}

/// A series of a chart that can count with a filter of its own (US1.16,
/// D-646): its fixed name and the keys of its two lists in `params`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeriesFilter {
    /// The series' fixed name, as the chart names it: `income` or
    /// `expenses`.
    pub series: &'static str,
    /// The key of its own accounts.
    pub accounts: &'static str,
    /// The key of its own categories and groups.
    pub categories: &'static str,
}

/// The series that can have a filter of their own: money in and money out
/// of [`crate::INCOME_EXPENSE_BY_MONTH`], the only dataset that takes them.
pub const SERIES_FILTERS: [SeriesFilter; 2] = [
    SeriesFilter {
        series: "income",
        accounts: "income_accounts",
        categories: "income_categories",
    },
    SeriesFilter {
        series: "expenses",
        accounts: "expenses_accounts",
        categories: "expenses_categories",
    },
];

/// Whether `key` is one of the lists of [`SERIES_FILTERS`].
pub fn is_series_filter(key: &str) -> bool {
    SERIES_FILTERS
        .iter()
        .any(|series| series.accounts == key || series.categories == key)
}

/// What one series asks for on its own (D-646): a list it has replaces the
/// component's (or the filter bar's) list of that dimension for this series
/// only; a list it lacks follows the component as before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeriesAsked {
    /// The series' fixed name ([`SeriesFilter::series`]).
    pub series: &'static str,
    /// Its own accounts, if it has some.
    pub accounts: Option<Chosen>,
    /// Its own categories and groups, if it has some.
    pub categories: Option<Chosen>,
}

/// What one component asks for, before the book is consulted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    /// A period word for [`crate::Period::for_component`]: `page` means the
    /// page's own period, which the caller knows.
    pub period: String,
    /// Where the period comes from.
    pub period_origin: Origin,
    /// The accounts, `None` for all of them.
    pub accounts: Option<Chosen>,
    /// The categories and groups, `None` for all of them.
    pub categories: Option<Chosen>,
    /// The tags, `None` for every part whatever its tags. Only the
    /// component's own (`params.tags`): the filter bar has none (US5.2).
    pub tags: Option<Chosen>,
    /// The dimensions the component's dataset takes.
    pub takes: Takes,
    /// The series with a filter of their own (D-646), in the order of
    /// [`SERIES_FILTERS`]; empty when none has one.
    pub series: Vec<SeriesAsked>,
    /// What spending is averaged per (D-648); `None` for its total.
    pub average: Option<crate::Per>,
    /// What a spending chart compares each category with (US3.1a, D-687);
    /// `None` for no comparison and for a key figure's own `compare`.
    pub compare: Option<crate::CompareWith>,
    /// The length of a candle chart's candles the user chose (Q-498);
    /// `None` for the automatic length and for every other component.
    pub candle: Option<crate::CandleLength>,
}

/// A note under a component about its filter (D-350). The component writes
/// its words; the parameters are data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterNote {
    /// A filter asked for this dimension but the dataset does not take it,
    /// so it was not applied: *Not filtered by account: a budget counts
    /// every account*.
    NotApplied(Dimension),
    /// An ID the filter names is no longer in the book (a removed category),
    /// so it was left out: *something that no longer exists*.
    Gone {
        /// Which list named it.
        dimension: Dimension,
        /// The ID as the list writes it.
        id: String,
    },
    /// Every ID of this dimension's list is gone, so the filter chooses
    /// nothing: the component says so instead of drawing zeros.
    ChoosesNothing(Dimension),
}

/// What a component is drawn with: the [`Filter`] for its dataset, and the
/// marks and notes it shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// What the dataset is asked with.
    pub filter: Filter,
    /// The dimensions that come from the component's own setting: the
    /// *Own filter* mark names them.
    pub own: Vec<Dimension>,
    /// The dimensions a filter narrows the figures by: the *Filtered* mark
    /// lists what `filter` keeps of them.
    pub filtered: Vec<Dimension>,
    /// The notes, in the order period, accounts, categories, tags.
    pub notes: Vec<FilterNote>,
}

impl Applied {
    /// Whether a filter chooses nothing, so the component draws nothing.
    pub fn chooses_nothing(&self) -> bool {
        self.notes
            .iter()
            .any(|note| matches!(note, FilterNote::ChoosesNothing(_)))
    }
}

fn refused(reason: &'static str, dimension: Dimension) -> ReportError {
    ReportError::FilterInvalid {
        reason,
        dimension: dimension.word(),
    }
}

/// Refuses a list of more than [`MAX_FILTER_IDS`] IDs or with an ID twice
/// (`report.filter-invalid`). IDs are compared ignoring capitals, as the
/// book compares them.
pub(crate) fn check_list(dimension: Dimension, ids: &[String]) -> Result<(), ReportError> {
    if ids.len() > MAX_FILTER_IDS {
        return Err(refused("too-many", dimension));
    }
    let mut seen = HashSet::new();
    if ids.iter().any(|id| !seen.insert(id.to_ascii_lowercase())) {
        return Err(refused("repeated", dimension));
    }
    Ok(())
}

/// The IDs of a component's list property, `None` when it has none or the
/// list is empty (all of them).
fn own_ids(component: &Component, key: &str) -> Option<Chosen> {
    match component.get(key) {
        Some(Setting::Ids(ids)) if !ids.is_empty() => Some(Chosen {
            ids: ids.clone(),
            origin: Origin::Own,
        }),
        _ => None,
    }
}

fn bar_ids(ids: &[String]) -> Option<Chosen> {
    (!ids.is_empty()).then(|| Chosen {
        ids: ids.to_vec(),
        origin: Origin::FilterBar,
    })
}

/// The dimensions a component takes: its dataset's, or for the cards the
/// page fills, those of the dataset they are filled from (the net worth
/// card from `account_balances`, D-372; where the money came from from the
/// spending). A component that shows no figures takes nothing.
pub fn takes(component: &Component) -> Takes {
    match component.kind() {
        ComponentType::NetWorth => Takes::NO_CATEGORIES,
        ComponentType::IncomeSources => Takes::ALL,
        // Its list keeps whole transactions and has no tags to choose
        // (D-716), so a tag never narrows it.
        ComponentType::Transactions => Takes {
            tags: false,
            ..Takes::ALL
        },
        kind if kind.shows_no_data() => Takes::NOTHING,
        _ => component.dataset().takes,
    }
}

impl Asked {
    /// What `component` asks for when it lies in a period box with period
    /// `in_box` (or none) on a page whose filter bar holds `bar`. `None`
    /// for a component that shows no figures (a heading, a period box).
    pub fn new(component: &Component, in_box: Option<&str>, bar: &Filters) -> Option<Self> {
        let takes = takes(component);
        if takes == Takes::NOTHING {
            return None;
        }
        let own_period = match component.get("period") {
            Some(Setting::Word(word)) if word != "page" => Some(word.clone()),
            _ => None,
        };
        let (period, period_origin) = if let Some(word) = own_period {
            (word, Origin::Own)
        } else if let Some(word) = in_box.filter(|word| *word != "page") {
            (word.to_owned(), Origin::PeriodBox)
        } else if let Some(word) = &bar.period {
            (word.clone(), Origin::FilterBar)
        } else {
            ("page".to_owned(), Origin::Default)
        };
        Some(Self {
            period,
            period_origin,
            accounts: own_ids(component, "accounts").or_else(|| bar_ids(&bar.accounts)),
            categories: own_ids(component, "categories").or_else(|| bar_ids(&bar.categories)),
            tags: own_ids(component, crate::schema::TAGS_KEY),
            takes,
            series: series_asked(component),
            average: match component.get(crate::schema::AVERAGE_KEY) {
                Some(Setting::Word(word)) => crate::Per::read(word),
                _ => None,
            },
            compare: match component.get(crate::schema::COMPARE_KEY) {
                Some(Setting::Word(word))
                    if component.dataset().id == crate::SPENDING_BY_CATEGORY =>
                {
                    crate::CompareWith::read(word)
                }
                _ => None,
            },
            candle: match component.get(crate::schema::CANDLE_KEY) {
                Some(Setting::Word(word)) => crate::CandleLength::read(word),
                _ => None,
            },
        })
    }

    /// What the series `series` is counted with (D-646): this, with the
    /// series' own lists in place of the component's. The same as this for
    /// a series without a filter of its own.
    pub fn for_series(&self, series: &str) -> Self {
        let own = self.series.iter().find(|asked| asked.series == series);
        Self {
            period: self.period.clone(),
            period_origin: self.period_origin,
            accounts: own
                .and_then(|own| own.accounts.clone())
                .or_else(|| self.accounts.clone()),
            categories: own
                .and_then(|own| own.categories.clone())
                .or_else(|| self.categories.clone()),
            tags: self.tags.clone(),
            takes: self.takes,
            series: Vec::new(),
            average: self.average,
            compare: self.compare,
            candle: self.candle,
        }
    }

    /// Checks the IDs against the book `source` was read from and gives the
    /// [`Filter`] the component's dataset is asked with, with its marks and
    /// notes.
    ///
    /// Refused (`report.filter-invalid`), whether the dataset takes the
    /// dimension or not: a list of more than [`MAX_FILTER_IDS`] IDs or with
    /// one twice, an account list naming something that is not one of the
    /// user's own accounts (a category, the starting balances), a category
    /// list naming an account, a tag list naming anything but a tag. An ID
    /// the book doesn't know is left out with a [`FilterNote::Gone`]; a list
    /// left with nothing chooses nothing. A group counts all its members
    /// (D-291).
    pub fn apply(&self, source: &Source) -> Result<Applied, ReportError> {
        let mut applied = Applied::default();
        if self.period_origin == Origin::Own {
            applied.own.push(Dimension::Period);
        }
        if let Some(chosen) = &self.accounts {
            let kept = accounts_of(source, &chosen.ids, &mut applied.notes)?;
            narrow(
                &mut applied,
                Dimension::Accounts,
                chosen,
                kept,
                self.takes,
                |filter, kept| filter.accounts = Some(kept),
            );
        }
        if let Some(chosen) = &self.categories {
            let kept = categories_of(source, &chosen.ids, &mut applied.notes)?;
            narrow(
                &mut applied,
                Dimension::Categories,
                chosen,
                kept,
                self.takes,
                |filter, kept| filter.categories = Some(kept),
            );
        }
        if let Some(chosen) = &self.tags {
            let kept = tags_of(source, &chosen.ids, &mut applied.notes)?;
            narrow(
                &mut applied,
                Dimension::Tags,
                chosen,
                kept,
                self.takes,
                |filter, kept| filter.tags = Some(kept),
            );
        }
        Ok(applied)
    }
}

/// The series of `component` that have a list of their own (D-646); only
/// the dataset of [`SERIES_FILTERS`] takes them.
fn series_asked(component: &Component) -> Vec<SeriesAsked> {
    if component.dataset().id != crate::INCOME_EXPENSE_BY_MONTH {
        return Vec::new();
    }
    SERIES_FILTERS
        .iter()
        .map(|filter| SeriesAsked {
            series: filter.series,
            accounts: own_ids(component, filter.accounts),
            categories: own_ids(component, filter.categories),
        })
        .filter(|asked| asked.accounts.is_some() || asked.categories.is_some())
        .collect()
}

/// Puts what a list kept into `applied`: the filter, the marks and the
/// notes, or the note that the dataset does not take the dimension.
fn narrow<T>(
    applied: &mut Applied,
    dimension: Dimension,
    chosen: &Chosen,
    kept: Vec<T>,
    takes: Takes,
    set: impl FnOnce(&mut Filter, Vec<T>),
) {
    if !takes.takes(dimension) {
        // Nothing of this list counts, so neither do the notes about it.
        applied.notes.retain(
            |note| !matches!(note, FilterNote::Gone { dimension: gone, .. } if *gone == dimension),
        );
        applied.notes.push(FilterNote::NotApplied(dimension));
        return;
    }
    if chosen.origin == Origin::Own {
        applied.own.push(dimension);
    }
    applied.filtered.push(dimension);
    if kept.is_empty() {
        applied.notes.push(FilterNote::ChoosesNothing(dimension));
    }
    set(&mut applied.filter, kept);
}

/// The account with this ID written as text, ignoring capitals.
fn account_by_id<'a>(source: &'a Source, text: &str) -> Option<&'a kassensturz_core::Account> {
    source
        .accounts()
        .values()
        .find(|account| account.id.to_string().eq_ignore_ascii_case(text))
}

/// The own accounts `ids` name, in their order; a gone one is noted.
fn accounts_of(
    source: &Source,
    ids: &[String],
    notes: &mut Vec<FilterNote>,
) -> Result<Vec<AccountId>, ReportError> {
    check_list(Dimension::Accounts, ids)?;
    let mut kept = Vec::new();
    for id in ids {
        match account_by_id(source, id) {
            Some(account) if account.kind.is_account() => kept.push(account.id),
            Some(_) => return Err(refused("not-an-account", Dimension::Accounts)),
            None if source.member_by_id(id).is_some() => {
                return Err(refused("not-an-account", Dimension::Accounts));
            }
            None => notes.push(FilterNote::Gone {
                dimension: Dimension::Accounts,
                id: id.clone(),
            }),
        }
    }
    Ok(kept)
}

/// The categories `ids` name, a group as all its members, each once in the
/// order they are first named; a gone one is noted.
fn categories_of(
    source: &Source,
    ids: &[String],
    notes: &mut Vec<FilterNote>,
) -> Result<Vec<AccountId>, ReportError> {
    check_list(Dimension::Categories, ids)?;
    let mut kept: Vec<AccountId> = Vec::new();
    for id in ids {
        match source.member_by_id(id) {
            Some(member) => {
                let counted = match member {
                    GroupMember::Category(category) => vec![category],
                    GroupMember::Group(_) => source.counted(member),
                };
                for category in counted {
                    if !kept.contains(&category) {
                        kept.push(category);
                    }
                }
            }
            None if account_by_id(source, id).is_some() => {
                return Err(refused("not-a-category", Dimension::Categories));
            }
            None => notes.push(FilterNote::Gone {
                dimension: Dimension::Categories,
                id: id.clone(),
            }),
        }
    }
    Ok(kept)
}

/// The tags `ids` name, each once in the order they are first named; a gone
/// one is noted. An ID of something else of the book (an account, a
/// category, a group) is refused, so a list meant for another dimension
/// never counts as a list of tags that are all gone.
fn tags_of(
    source: &Source,
    ids: &[String],
    notes: &mut Vec<FilterNote>,
) -> Result<Vec<TagId>, ReportError> {
    check_list(Dimension::Tags, ids)?;
    let mut kept: Vec<TagId> = Vec::new();
    for id in ids {
        match source
            .tags()
            .iter()
            .find(|tag| tag.id.to_string().eq_ignore_ascii_case(id))
        {
            Some(tag) => {
                if !kept.contains(&tag.id) {
                    kept.push(tag.id);
                }
            }
            None if account_by_id(source, id).is_some() || source.member_by_id(id).is_some() => {
                return Err(refused("not-a-tag", Dimension::Tags));
            }
            None => notes.push(FilterNote::Gone {
                dimension: Dimension::Tags,
                id: id.clone(),
            }),
        }
    }
    Ok(kept)
}
