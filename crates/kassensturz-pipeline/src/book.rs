// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The complete export: gathering a whole book for it, and filling a new
//! book with one read back (D-029, D-046).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{Account, AccountId, AccountKind, RuleId, RuleNode};
use kassensturz_export::{BookData, BookedByRule, ExportData, ViewData};
use kassensturz_store::{Book, HistoryRetention, StoreError};

/// Everything `book` holds, for the complete export (`BookExport`): its
/// settings, currencies, banks, accounts and categories, payees, tags, rates, Rules,
/// transactions and which parts the Rules booked. Only the change history
/// stays behind (D-066). Nothing in the book changes.
///
/// The order is fixed, so the same book always gives the same file:
/// accounts by name, then the internal account for starting balances, then
/// the categories as a tree, each after the one it sits inside (income
/// before spending); rates by their currencies and date; transactions by
/// booking date and ID.
pub fn export_book(book: &Book) -> Result<ExportData, StoreError> {
    let mut equity: Vec<Account> = book
        .all_accounts()?
        .into_iter()
        .filter(|account| account.kind == AccountKind::Equity)
        .collect();
    equity.sort_by_cached_key(|account| (account.name.to_lowercase(), account.id));
    let accounts = book
        .accounts(true)?
        .into_iter()
        .chain(equity)
        .chain(book.categories()?.into_iter().map(|entry| entry.category))
        .collect();
    Ok(ExportData::Book(BookData {
        rounding_mode: book.rounding_mode()?,
        history_keep_days: match book.history_retention()? {
            HistoryRetention::Days(days) => Some(days),
            HistoryRetention::Forever => None,
        },
        currencies: book.currencies()?,
        accounts,
        rates: book.all_rates()?,
        rules: book.rules()?,
        transactions: book.transactions()?,
        rule_marks: book
            .rule_marks()?
            .into_iter()
            .map(|mark| BookedByRule {
                split: mark.split,
                rule: mark.rule,
            })
            .collect(),
        institutions: book.institutions()?,
        bank_links: book.bank_links()?,
        views: book
            .views(None)?
            .into_iter()
            .map(|view| ViewData {
                id: view.id,
                document: view.document,
            })
            .collect(),
        groups: book.category_groups()?,
        hidden_categories: book.hidden_categories()?,
        budgets: book.budgets()?,
        payees: book.payees()?,
        tags: book.tags()?,
    }))
}

/// Fills the new, empty `book` with `data`, e.g. a complete export read
/// back (`read_book`), so it holds what the book the export was made from
/// held, with the same IDs.
///
/// Everything goes in through the book's usual functions, so every check
/// they make applies to data from a file too: parts must add up, names must
/// be unique, and a closed account's balance must be zero. As with creating
/// a book, nothing is recorded in the change history (`Book::fill_new`). If
/// anything is refused, the book stays empty, so the front end can remove
/// it again.
pub fn fill_book(book: &mut Book, data: &BookData) -> Result<(), StoreError> {
    let keep = data
        .history_keep_days
        .map_or(HistoryRetention::Forever, HistoryRetention::Days);
    book.fill_new(keep, |book| {
        book.set_rounding_mode(data.rounding_mode)?;
        for &currency in &data.currencies {
            book.restore_currency(currency)?;
        }
        for institution in &data.institutions {
            book.add_institution(institution)?;
        }
        // Open at first, so their transactions can be added; closed at the
        // end, which checks that their balance is zero.
        for account in parents_first(&data.accounts)? {
            book.add_account(&Account {
                closed: false,
                ..account.clone()
            })?;
        }
        for link in &data.bank_links {
            book.link_bank_account(link)?;
        }
        for view in &data.views {
            book.restore_view(view.id, &view.document)?;
        }
        book.restore_category_groups(&data.groups)?;
        book.set_hidden_categories(&data.hidden_categories)?;
        book.restore_budgets(&data.budgets)?;
        for rate in &data.rates {
            book.add_rate(rate)?;
        }
        add_rules(book, &data.rules, None)?;
        for payee in &data.payees {
            book.add_payee(payee)?;
        }
        for tag in &data.tags {
            book.add_tag(tag)?;
        }
        for transaction in &data.transactions {
            book.add_transaction(transaction)?;
        }
        for &BookedByRule { split, rule } in &data.rule_marks {
            book.restore_rule_mark(split, rule)?;
        }
        for account in data.accounts.iter().filter(|account| account.closed) {
            book.close_account(account.id)?;
        }
        Ok(())
    })
}

/// `accounts` in an order the book can add them in: every category after
/// the one it sits inside. One whose parent isn't in the list comes early,
/// so the book refuses it ([`StoreError::UnknownAccount`]); categories that
/// sit inside each other in a loop are refused here.
fn parents_first(accounts: &[Account]) -> Result<Vec<&Account>, StoreError> {
    let ids: HashSet<AccountId> = accounts.iter().map(|account| account.id).collect();
    let mut children: HashMap<AccountId, Vec<&Account>> = HashMap::new();
    let mut order = Vec::with_capacity(accounts.len());
    for account in accounts {
        match account.parent {
            Some(parent) if ids.contains(&parent) && parent != account.id => {
                children.entry(parent).or_default().push(account);
            }
            _ => order.push(account),
        }
    }
    let mut next = 0;
    while let Some(account) = order.get(next) {
        if let Some(inside) = children.remove(&account.id) {
            order.extend(inside);
        }
        next += 1;
    }
    match children.values().flatten().map(|account| account.id).min() {
        Some(in_a_loop) => Err(StoreError::CategoryInsideItself(in_a_loop)),
        None => Ok(order),
    }
}

/// Adds `rules` with their sub-Rules, inside `parent`, in their order.
fn add_rules(
    book: &mut Book,
    rules: &[RuleNode],
    parent: Option<RuleId>,
) -> Result<(), StoreError> {
    for node in rules {
        book.add_rule(&node.rule, parent)?;
        add_rules(book, &node.sub_rules, Some(node.rule.id))?;
    }
    Ok(())
}
