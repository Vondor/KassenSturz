// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `spending_by_category`: what went out, by category.

use std::cmp::Reverse;
use std::collections::HashMap;

use kassensturz_core::{AccountId, CurrencyCode, Money};
use kassensturz_store::StoreError;

use crate::source::Kind;
use crate::{Filter, Period, ReportError, Source, TextFormat, TextTable};

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

/// One category's spending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryRow {
    /// The category, or `None` for money not sorted into a category yet.
    pub category: Option<AccountId>,
    /// The names from the top-level category down, empty when `category` is
    /// `None`.
    pub path: Vec<String>,
    /// What went out, less what came back (a refund). Negative when more
    /// came back than went out.
    pub total: Money,
}

/// The spending of one currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendingByCategory {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// The categories with spending, largest first; a tie keeps the order
    /// of the category tree, with money not sorted yet after the categories.
    /// Money not sorted yet is one row among them, by its size.
    pub rows: Vec<CategoryRow>,
    /// All of it, added up from the exact amounts.
    pub total: Money,
}

/// What went out in `period`, by category, one result per currency in the
/// order of their codes. Only categories with spending are listed. Money
/// moved between the user's own accounts and starting balances are not
/// spending.
pub fn spending_by_category(
    source: &Source,
    period: Period,
    filter: &Filter,
    level: Level,
) -> Result<Vec<SpendingByCategory>, ReportError> {
    let overflow = |_| ReportError::Store(StoreError::AmountOverflow);
    let mut sums: HashMap<(CurrencyCode, Option<AccountId>), Money> = HashMap::new();
    for part in source.parts(period, filter)? {
        if part.kind != Kind::Spending {
            continue;
        }
        let key = match (level, part.category) {
            (Level::TopLevel, Some(category)) => Some(source.top_level(category)),
            (_, category) => category,
        };
        let sum = sums
            .entry((part.currency, key))
            .or_insert_with(|| Money::zero(part.currency));
        *sum = sum.checked_add(part.amount).map_err(overflow)?;
    }
    let mut currencies: Vec<CurrencyCode> = sums.keys().map(|(currency, _)| *currency).collect();
    currencies.sort_unstable();
    currencies.dedup();

    let mut results = Vec::new();
    for currency in currencies {
        let mut rows: Vec<(usize, CategoryRow)> = Vec::new();
        for (position, entry) in source.tree().iter().enumerate() {
            let id = Some(entry.category.id);
            if let Some(&total) = sums.get(&(currency, id))
                && !total.is_zero()
            {
                rows.push((
                    position,
                    CategoryRow {
                        category: id,
                        path: entry.path.clone(),
                        total,
                    },
                ));
            }
        }
        if let Some(&total) = sums.get(&(currency, None))
            && !total.is_zero()
        {
            rows.push((
                usize::MAX,
                CategoryRow {
                    category: None,
                    path: Vec::new(),
                    total,
                },
            ));
        }
        // Largest first, from the exact amounts (D-065); a tie keeps the
        // tree order.
        rows.sort_by_key(|(position, row)| (Reverse(row.total.minor_units()), *position));
        let rows: Vec<CategoryRow> = rows.into_iter().map(|(_, row)| row).collect();
        if rows.is_empty() {
            continue;
        }
        let total =
            Money::checked_sum(currency, rows.iter().map(|row| row.total)).map_err(overflow)?;
        results.push(SpendingByCategory {
            currency,
            rows,
            total,
        });
    }
    Ok(results)
}

impl SpendingByCategory {
    /// The result as a table of text with the columns `category` and
    /// `total`. A category's path is joined with " › ".
    pub fn to_text(&self, format: &impl TextFormat) -> TextTable {
        TextTable {
            currency: self.currency.to_string(),
            columns: vec!["category", "total"],
            rows: self
                .rows
                .iter()
                .map(|row| {
                    let name = if row.category.is_some() {
                        row.path.join(" › ")
                    } else {
                        format.unsorted()
                    };
                    vec![name, format.amount(row.total)]
                })
                .collect(),
        }
    }
}
