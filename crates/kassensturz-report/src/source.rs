// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the datasets are made from: the transactions of a period and the
//! accounts and categories they refer to, read from the book in one read
//! transaction so every figure comes from the same state (Q-036).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{Account, AccountId, AccountKind, CurrencyCode, Date, Money, Transaction};
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

/// The book's transactions of a period with the accounts they use.
pub struct Source {
    /// The days the transactions were read for.
    covers: Period,
    accounts: HashMap<AccountId, Account>,
    /// The categories in the order of the category tree, with their paths.
    tree: Vec<CategoryEntry>,
    transactions: Vec<Transaction>,
}

impl Source {
    /// Reads the transactions booked in `period` and the accounts and
    /// categories of the book.
    pub fn read(book: &Book, period: Period) -> Result<Self, ReportError> {
        book.reading(|book| {
            Ok(Self::from_parts(
                period,
                book.all_accounts()?,
                book.categories()?,
                book.transactions_between(period.first(), period.last())?,
            ))
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
        }
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
        let chosen = filter.categories.as_ref().map(|chosen| {
            let chosen: HashSet<AccountId> = chosen.iter().copied().collect();
            // The chosen categories and everything below them.
            self.tree
                .iter()
                .map(|entry| &entry.category)
                .filter(|category| {
                    let mut current = Some(category.id);
                    let mut steps = 0;
                    while let Some(id) = current {
                        if chosen.contains(&id) {
                            return true;
                        }
                        steps += 1;
                        if steps > self.accounts.len() {
                            return false;
                        }
                        current = self.accounts.get(&id).and_then(|account| account.parent);
                    }
                    false
                })
                .map(|category| category.id)
                .collect::<HashSet<AccountId>>()
        });
        let accounts: Option<HashSet<AccountId>> = filter
            .accounts
            .as_ref()
            .map(|accounts| accounts.iter().copied().collect());
        let overflow = |_| StoreError::AmountOverflow;
        let mut parts = Vec::new();
        for transaction in &self.transactions {
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
