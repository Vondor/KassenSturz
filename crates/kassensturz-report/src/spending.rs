// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `spending_by_category`: what went out, by category.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use kassensturz_core::{AccountId, CurrencyCode, GroupId, GroupMember, Money};
use kassensturz_store::StoreError;

use crate::source::Kind;
use crate::{Filter, Period, ReportError, Shown, Source, TextFormat, TextTable};

/// The dataset's ID, as dashboards and the AI name it (D-277).
pub const SPENDING_BY_CATEGORY: &str = "spending_by_category";

/// How categories with subcategories are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Every category on its own row, with the full path.
    Each,
    /// Subcategories are added to the top-level category they sit in.
    TopLevel,
}

/// One row of spending: a category, one of My groups, or money not sorted
/// yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryRow {
    /// The category, or `None` for a group's row and for money not sorted
    /// into a category yet.
    pub category: Option<AccountId>,
    /// The group, for a group's row (D-291).
    pub group: Option<GroupId>,
    /// The names from the top-level category down, or the group's name
    /// alone; empty for money not sorted yet.
    pub path: Vec<String>,
    /// What went out, less what came back (a refund). Negative when more
    /// came back than went out.
    pub total: Money,
}

impl CategoryRow {
    /// Whether this is the row of money not sorted into a category yet.
    pub fn is_unsorted(&self) -> bool {
        self.category.is_none() && self.group.is_none() && self.path.is_empty()
    }
}

/// The spending of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendingByCategory {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// The chosen groups and the categories with spending, largest first; a
    /// tie keeps the order of the chosen groups, then of the category tree,
    /// with money not sorted yet last. Money not sorted yet is one row among
    /// them, by its size.
    pub rows: Vec<CategoryRow>,
    /// All that is shown, added up from the exact amounts, each part once:
    /// when two groups that share a category are both shown (as bars), the
    /// rows add up to more than this.
    pub total: Money,
    /// The hidden categories with spending this result leaves out, largest
    /// first, each with everything inside it that is hidden with it (D-291).
    /// A chart shows them in a line under it, never silently: "Hidden: Rent
    /// and 1 more (1,250.00 € not shown)".
    pub hidden: Vec<CategoryRow>,
    /// What `hidden` adds up to.
    pub hidden_total: Money,
}

/// What went out in `period`, by category, one result per currency in the
/// order of their codes. Only categories with spending are listed. Money
/// moved between the user's own accounts and starting balances are not
/// spending.
///
/// Every category is on its own row and nothing is hidden, as before My
/// groups ([`Shown::everything`]); [`spending_by_category_shown`] applies
/// the user's groups and hide list.
pub fn spending_by_category(
    source: &Source,
    period: Period,
    filter: &Filter,
    level: Level,
) -> Result<Vec<SpendingByCategory>, ReportError> {
    spending_by_category_shown(source, period, filter, level, &Shown::everything())
}

/// Where one part of spending is counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key {
    /// The chosen group at this place in the list.
    Group(usize),
    /// A category's row at the chart's level, or money not sorted yet.
    Category(Option<AccountId>),
    /// Left out, because this hidden category holds it.
    Hidden(AccountId),
}

/// [`spending_by_category`] with My groups and the hide list (D-291): each
/// group in `shown` is one row that counts all its members, hidden ones
/// too (Q-225); every other category is listed at `level` unless it is
/// hidden, in which case the result names it under `hidden` with its amount.
/// At the top level, a hidden subcategory is taken out of the row of the
/// category it sits in. Fails with `report.groups-overlap` when two chosen
/// groups share a category and `shown.overlap` refuses it.
pub fn spending_by_category_shown(
    source: &Source,
    period: Period,
    filter: &Filter,
    level: Level,
    shown: &Shown,
) -> Result<Vec<SpendingByCategory>, ReportError> {
    let overflow = |_| ReportError::Store(StoreError::AmountOverflow);
    let chosen: Vec<GroupMember> = shown
        .groups
        .iter()
        .copied()
        .map(GroupMember::Group)
        .collect();
    let groups = source.resolve(&chosen, shown.overlap)?;
    // The categories the user picked, with everything inside them (D-331).
    let only: Option<HashSet<AccountId>> = (!shown.only.is_empty()).then(|| {
        shown
            .only
            .iter()
            .flat_map(|&of| source.counted(of))
            .collect()
    });
    let hidden_by = if shown.show_hidden || only.is_some() {
        HashMap::new()
    } else {
        source.hidden_by()
    };
    let mut sums: HashMap<(CurrencyCode, Key), Money> = HashMap::new();
    let mut shown_totals: HashMap<CurrencyCode, Money> = HashMap::new();
    let mut add = |currency: CurrencyCode, key: Key, amount: Money| -> Result<(), ReportError> {
        let sum = sums
            .entry((currency, key))
            .or_insert_with(|| Money::zero(currency));
        *sum = sum.checked_add(amount).map_err(overflow)?;
        Ok(())
    };
    for part in source.parts(period, filter)? {
        if part.kind != Kind::Spending {
            continue;
        }
        if only.as_ref().is_some_and(|only| {
            !part
                .category
                .is_some_and(|category| only.contains(&category))
        }) {
            continue;
        }
        let mut in_group = false;
        if let Some(category) = part.category {
            for (at, group) in groups.iter().enumerate() {
                if group.categories.contains(&category) {
                    add(part.currency, Key::Group(at), part.amount)?;
                    in_group = true;
                }
            }
        }
        if !in_group {
            if let Some(&by) = part.category.and_then(|id| hidden_by.get(&id)) {
                add(part.currency, Key::Hidden(by), part.amount)?;
                continue;
            }
            let key = match (level, part.category) {
                (Level::TopLevel, Some(category)) => Some(source.top_level(category)),
                (_, category) => category,
            };
            add(part.currency, Key::Category(key), part.amount)?;
        }
        // Shown, and counted once however many groups hold it.
        let total = shown_totals
            .entry(part.currency)
            .or_insert_with(|| Money::zero(part.currency));
        *total = total.checked_add(part.amount).map_err(overflow)?;
    }
    let mut currencies: Vec<CurrencyCode> = sums.keys().map(|(currency, _)| *currency).collect();
    currencies.sort_unstable();
    currencies.dedup();

    let row = |category, group, path: &[String], total| CategoryRow {
        category,
        group,
        path: path.to_vec(),
        total,
    };
    let mut results = Vec::new();
    for currency in currencies {
        let sum = |key| {
            sums.get(&(currency, key))
                .copied()
                .filter(|total| !total.is_zero())
        };
        let mut rows: Vec<(usize, CategoryRow)> = Vec::new();
        let mut hidden: Vec<(usize, CategoryRow)> = Vec::new();
        for (at, group) in groups.iter().enumerate() {
            if let (Some(total), GroupMember::Group(id)) = (sum(Key::Group(at)), group.of) {
                rows.push((at, row(None, Some(id), &group.path, total)));
            }
        }
        for (position, entry) in source.tree().iter().enumerate() {
            let id = entry.category.id;
            let place = groups.len() + position;
            if let Some(total) = sum(Key::Category(Some(id))) {
                rows.push((place, row(Some(id), None, &entry.path, total)));
            }
            if let Some(total) = sum(Key::Hidden(id)) {
                hidden.push((place, row(Some(id), None, &entry.path, total)));
            }
        }
        if let Some(total) = sum(Key::Category(None)) {
            rows.push((usize::MAX, row(None, None, &[], total)));
        }
        // Largest first, from the exact amounts (D-065); a tie keeps the
        // order of the groups and the tree.
        for list in [&mut rows, &mut hidden] {
            list.sort_by_key(|(place, row)| (Reverse(row.total.minor_units()), *place));
        }
        let rows: Vec<CategoryRow> = rows.into_iter().map(|(_, row)| row).collect();
        let hidden: Vec<CategoryRow> = hidden.into_iter().map(|(_, row)| row).collect();
        if rows.is_empty() && hidden.is_empty() {
            continue;
        }
        let hidden_total =
            Money::checked_sum(currency, hidden.iter().map(|row| row.total)).map_err(overflow)?;
        results.push(SpendingByCategory {
            currency,
            rows,
            total: shown_totals
                .get(&currency)
                .copied()
                .unwrap_or_else(|| Money::zero(currency)),
            hidden,
            hidden_total,
        });
    }
    Ok(results)
}

impl SpendingByCategory {
    /// The result as a table of text with the columns `category` and
    /// `total`. A category's path is joined with " › "; a group's row has
    /// the group's name. The hidden categories are not rows: the note about
    /// them is written from `hidden` and `hidden_total` (D-291).
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec!["category", "total"],
            rows: self
                .rows
                .iter()
                .map(|row| {
                    let name = if row.is_unsorted() {
                        format.unsorted()
                    } else {
                        row.path.join(" › ")
                    };
                    vec![name, format.amount(row.total)]
                })
                .collect(),
        }
    }
}
