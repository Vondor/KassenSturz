// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the datasets are made from: the transactions of a period, the
//! accounts and categories they refer to, and My groups and the hide list
//! (D-291), read from the book in one read transaction so every figure comes
//! from the same state (Q-036).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    Account, AccountId, AccountKind, CategoryGroup, Currency, CurrencyCode, Date, GroupId, Money,
    RoundingMode, Transaction,
};
use kassensturz_store::{Book, CategoryEntry, StoreError};

use crate::{Filter, Period, ReportError};

/// Whether money counts as income or as spending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Income,
    Spending,
}

/// One part of a transaction that counts as income or spending.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Part {
    /// Which transaction of the `Source` it is a part of.
    pub(crate) transaction: usize,
    pub(crate) currency: CurrencyCode,
    pub(crate) day: Date,
    pub(crate) kind: Kind,
    /// The category, or `None` for money not sorted into one yet.
    pub(crate) category: Option<AccountId>,
    /// The amount in the transaction's currency, positive when it adds to
    /// its kind: money that came in for income, money that went out for
    /// spending. A refund is a negative amount of spending.
    pub(crate) amount: Money,
}

/// The book's transactions of a period with the accounts they use, and the
/// user's groups and hidden categories.
#[derive(Clone)]
pub struct Source {
    /// The days the transactions were read for.
    covers: Period,
    accounts: HashMap<AccountId, Account>,
    /// The categories in the order of the category tree, with their paths.
    tree: Vec<CategoryEntry>,
    transactions: Vec<Transaction>,
    /// My groups, in the book's order.
    groups: Vec<CategoryGroup>,
    /// The hidden categories, as the book lists them.
    hidden: Vec<AccountId>,
    /// The decimal places the book stores for each currency (D-027).
    currencies: HashMap<CurrencyCode, u8>,
    /// The book's rounding rule (D-027).
    rounding: RoundingMode,
}

impl Source {
    /// Reads the transactions booked in `period`, the accounts and
    /// categories of the book, its groups and hidden categories, and the
    /// decimal places and rounding rule a formula's result needs.
    pub fn read(book: &Book, period: Period) -> Result<Self, ReportError> {
        book.reading(|book| {
            Ok(Self::from_parts(
                period,
                book.all_accounts()?,
                book.categories()?,
                book.transactions_between(period.first(), period.last())?,
            )
            .with_groups(book.category_groups()?, book.hidden_categories()?)
            .with_settings(&book.currencies()?, book.rounding_mode()?))
        })
    }

    pub(crate) fn from_parts(
        covers: Period,
        accounts: Vec<Account>,
        tree: Vec<CategoryEntry>,
        transactions: Vec<Transaction>,
    ) -> Self {
        Self {
            covers,
            accounts: accounts
                .into_iter()
                .map(|account| (account.id, account))
                .collect(),
            tree,
            transactions,
            groups: Vec::new(),
            hidden: Vec::new(),
            currencies: HashMap::new(),
            rounding: RoundingMode::default(),
        }
    }

    /// The same `Source` with the book's currencies and rounding rule.
    pub(crate) fn with_settings(self, currencies: &[Currency], rounding: RoundingMode) -> Self {
        Self {
            currencies: currencies
                .iter()
                .map(|currency| (currency.code(), currency.minor_digits()))
                .collect(),
            rounding,
            ..self
        }
    }

    /// The decimal places of `currency`: as the book stores them, else as
    /// ISO 4217 has them, else 2.
    pub(crate) fn minor_digits(&self, currency: CurrencyCode) -> u8 {
        self.currencies
            .get(&currency)
            .copied()
            .or_else(|| currency.iso_minor_digits())
            .unwrap_or(2)
    }

    /// The book's rounding rule.
    pub(crate) fn rounding(&self) -> RoundingMode {
        self.rounding
    }

    /// The same `Source` with these groups and hidden categories.
    pub(crate) fn with_groups(self, groups: Vec<CategoryGroup>, hidden: Vec<AccountId>) -> Self {
        Self {
            groups,
            hidden,
            ..self
        }
    }

    /// A copy that also has `groups`, after the book's own (D-341): columns
    /// of the Statistic Builder, which a formula names like a group.
    pub(crate) fn with_more_groups(&self, more: Vec<CategoryGroup>) -> Self {
        let mut copy = self.clone();
        copy.groups.extend(more);
        copy
    }

    /// My groups, in the book's order.
    pub(crate) fn groups(&self) -> &[CategoryGroup] {
        &self.groups
    }

    /// The group with this ID.
    pub(crate) fn group(&self, id: GroupId) -> Option<&CategoryGroup> {
        self.groups.iter().find(|group| group.id == id)
    }

    /// The hidden categories, as the book lists them.
    pub(crate) fn hidden(&self) -> &[AccountId] {
        &self.hidden
    }

    /// Every category that is one of `roots` or inside one, with the topmost
    /// of `roots` it is in.
    pub(crate) fn inside(&self, roots: &HashSet<AccountId>) -> HashMap<AccountId, AccountId> {
        let mut inside = HashMap::new();
        for entry in &self.tree {
            let mut current = Some(entry.category.id);
            let mut topmost = None;
            // A book's tree has no loop (`Book::categories` checks), but a
            // bound keeps a wrong `Source` from hanging.
            for _ in 0..=self.accounts.len() {
                let Some(id) = current else { break };
                if roots.contains(&id) {
                    topmost = Some(id);
                }
                current = self.accounts.get(&id).and_then(|account| account.parent);
            }
            if let Some(root) = topmost {
                inside.insert(entry.category.id, root);
            }
        }
        inside
    }

    /// The category and its path at `position` in the tree order.
    pub(crate) fn tree(&self) -> &[CategoryEntry] {
        &self.tree
    }

    /// The top-level category `category` sits in, or itself.
    pub(crate) fn top_level(&self, category: AccountId) -> AccountId {
        let mut current = category;
        // A book's tree has no loop (`Book::categories` checks), but a
        // bound keeps a wrong `Source` from hanging.
        for _ in 0..=self.accounts.len() {
            match self
                .accounts
                .get(&current)
                .and_then(|account| account.parent)
            {
                Some(parent) => current = parent,
                None => break,
            }
        }
        current
    }

    /// Every part of the transactions in `period` that counts as income or
    /// spending and passes `filter`. Zero amounts are left out.
    ///
    /// Fails with [`ReportError::PeriodNotRead`] if the `Source` was read
    /// for fewer days than `period`.
    pub(crate) fn parts(&self, period: Period, filter: &Filter) -> Result<Vec<Part>, ReportError> {
        if period.first() < self.covers.first() || period.last() > self.covers.last() {
            return Err(ReportError::PeriodNotRead);
        }
        // The chosen categories and everything below them.
        let chosen: Option<HashSet<AccountId>> = filter.categories.as_ref().map(|chosen| {
            self.inside(&chosen.iter().copied().collect())
                .into_keys()
                .collect()
        });
        let accounts: Option<HashSet<AccountId>> = filter
            .accounts
            .as_ref()
            .map(|accounts| accounts.iter().copied().collect());
        let overflow = |_| StoreError::AmountOverflow;
        let mut parts = Vec::new();
        for (index, transaction) in self.transactions.iter().enumerate() {
            // A Transfer counts by its booking date, not by the days its
            // sides show (D-108).
            let day = transaction.booking_date();
            if !period.contains(day) {
                continue;
            }
            if let Some(accounts) = &accounts
                && !transaction
                    .splits()
                    .iter()
                    .any(|split| split.account.is_some_and(|id| accounts.contains(&id)))
            {
                continue;
            }
            let currency = transaction.currency();
            for split in transaction.splits() {
                let (kind, category) = match split.account {
                    // Not sorted yet: what came in is income, what went out
                    // spending, as the overview counts it (D-122).
                    None if split.amount.is_negative() => (Kind::Income, None),
                    None => (Kind::Spending, None),
                    Some(id) => {
                        let account = self
                            .accounts
                            .get(&id)
                            .ok_or(StoreError::UnknownAccount(id))?;
                        let kind = match account.kind {
                            AccountKind::Income => Kind::Income,
                            AccountKind::Expense => Kind::Spending,
                            // The user's own accounts and starting
                            // balances are neither (D-021, D-089).
                            _ => continue,
                        };
                        (kind, Some(id))
                    }
                };
                if let Some(chosen) = &chosen
                    && !category.is_some_and(|id| chosen.contains(&id))
                {
                    continue;
                }
                if split.amount.is_zero() {
                    continue;
                }
                // Income categories and money in are negative on the
                // category side: they give money.
                let amount = match kind {
                    Kind::Spending => split.amount,
                    Kind::Income => split.amount.checked_neg().map_err(overflow)?,
                };
                parts.push(Part {
                    transaction: index,
                    currency,
                    day,
                    kind,
                    category,
                    amount,
                });
            }
        }
        Ok(parts)
    }
}
