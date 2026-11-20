// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Step 3 of the guided setup (#1031, `docs/GUIDED_SETUP.md`): the
//! transactions without a category, grouped by the name they carry, and one
//! change that gives a whole group its category.
//!
//! A group holds the simple transactions of one name, one direction (money
//! in or out) and one currency: those with exactly one part in an account of
//! the user's and nothing but parts without a category besides. A
//! transaction broken down into parts with categories, or one in two
//! currencies, is left to its own page. Sums are exact and reach the page
//! as text (D-065).
//!
//! Sorting a group is the existing change of a transaction's category, done
//! for every transaction of the group inside one user action, so one Undo
//! takes all of it back. It is recorded as the edit of a transaction; the
//! book gets no new kind of change (no format change).

use std::collections::HashMap;

use kassensturz_core::{Account, AccountId, CurrencyCode, Money, Split, Transaction};
use kassensturz_store::{Action, Book, StoreError};

use crate::format::Locale;
use crate::register::{AmountWriter, Shown};
use crate::transaction_entry::EntryError;

/// How many groups the page lists, the largest first.
pub const LISTED: usize = 12;

/// The transactions of one name that have no category yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// What the page hands back to sort this group; see [`key_of`].
    pub key: String,
    /// The name the transactions carry; empty when they carry none.
    pub name: String,
    /// Whether the money came in.
    pub money_in: bool,
    /// How many transactions it holds.
    pub count: usize,
    /// Their sum, with its sign, as the register writes amounts.
    pub total: Shown,
    /// The account of the user's the newest of them is in, as text: its
    /// categories are the ones the group can get.
    pub account: String,
    /// The newest of them, as text: what a Rule offer is filled from.
    pub transaction: String,
}

/// What the page shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Unsorted {
    /// How many transactions in the whole book have no category
    /// ([`Book::unsorted_count`]), the ones no group holds included.
    pub count: usize,
    /// The largest groups, at most [`LISTED`]: most transactions first, then
    /// by name.
    pub groups: Vec<Group>,
    /// How many more groups there are.
    pub more: usize,
}

/// A simple transaction without a category, seen from the user's account.
struct Member {
    transaction: Transaction,
    account: AccountId,
    amount: Money,
}

type GroupKey = (String, bool, CurrencyCode);

/// The key of a group as text: direction, currency, then the name, which
/// may hold any character.
fn key_of((name, money_in, currency): &GroupKey) -> String {
    let direction = if *money_in { "in" } else { "out" };
    format!("{direction}|{currency}|{name}")
}

fn read_key(key: &str) -> Option<GroupKey> {
    let mut pieces = key.splitn(3, '|');
    let money_in = match pieces.next()? {
        "in" => true,
        "out" => false,
        _ => return None,
    };
    let currency = CurrencyCode::new(pieces.next()?).ok()?;
    Some((pieces.next()?.to_owned(), money_in, currency))
}

/// The simple transactions without a category, newest first, each with the
/// group it belongs to.
fn members(book: &Book) -> Result<Vec<(GroupKey, Member)>, StoreError> {
    let own: HashMap<AccountId, Account> = book
        .all_accounts()?
        .into_iter()
        .filter(|account| account.kind.is_account())
        .map(|account| (account.id, account))
        .collect();
    let mut found = Vec::new();
    for transaction in book.newest_unsorted(usize::MAX)? {
        let mut in_account = None;
        let mut simple = true;
        for split in transaction.splits() {
            match split.account {
                None => simple &= split.foreign.is_none(),
                Some(id) => match own.get(&id) {
                    Some(account) if in_account.is_none() => {
                        in_account = Some((account, split.amount));
                    }
                    // A second part in an account, or a part that already
                    // has a category.
                    _ => simple = false,
                },
            }
        }
        let Some((account, amount)) = in_account else {
            continue;
        };
        if !simple || amount.is_zero() {
            continue;
        }
        let key = (
            transaction.payee().unwrap_or_default().to_owned(),
            amount.minor_units() > 0,
            account.currency,
        );
        found.push((
            key,
            Member {
                transaction,
                account: account.id,
                amount,
            },
        ));
    }
    Ok(found)
}

/// The transactions without a category, grouped by name.
pub fn unsorted(book: &Book, locale: &impl Locale) -> Result<Unsorted, StoreError> {
    let mut order: Vec<GroupKey> = Vec::new();
    let mut grouped: HashMap<GroupKey, Vec<Member>> = HashMap::new();
    for (key, member) in members(book)? {
        if !grouped.contains_key(&key) {
            order.push(key.clone());
        }
        grouped.entry(key).or_default().push(member);
    }
    let mut groups = Vec::new();
    for key in order {
        let Some(held) = grouped.remove(&key) else {
            continue;
        };
        let Some(newest) = held.first() else {
            continue;
        };
        let total = Money::checked_sum(key.2, held.iter().map(|member| member.amount))
            .map_err(|_| StoreError::AmountOverflow)?;
        groups.push(Group {
            key: key_of(&key),
            name: key.0.clone(),
            money_in: key.1,
            count: held.len(),
            total: AmountWriter::new(book, key.2, locale)?.text(total),
            account: newest.account.to_string(),
            transaction: newest.transaction.id().to_string(),
        });
    }
    groups.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    let more = groups.len().saturating_sub(LISTED);
    groups.truncate(LISTED);
    Ok(Unsorted {
        count: book.unsorted_count()?,
        groups,
        more,
    })
}

/// Gives every transaction of the group with `key` the category with the ID
/// `category`, as one user action, and returns how many it sorted. The
/// category must be an open one in the group's currency, as the transaction
/// page asks (`input.no-such-entry` otherwise, also for a group that is no
/// longer there).
pub fn sort(book: &mut Book, key: &str, category: &str) -> Result<usize, EntryError> {
    let wanted = read_key(key).ok_or(EntryError::NoSuchEntry)?;
    let category = book
        .all_accounts()?
        .into_iter()
        .find(|account| {
            account.kind.is_category()
                && !account.closed
                && account.currency == wanted.2
                && account.id.to_string() == category
        })
        .ok_or(EntryError::NoSuchEntry)?;
    let mut sorted = Vec::new();
    for (key, member) in members(book)? {
        if key != wanted {
            continue;
        }
        let stored = member.transaction;
        let splits = stored
            .splits()
            .iter()
            .map(|split| Split {
                account: split.account.or(Some(category.id)),
                ..split.clone()
            })
            .collect();
        let transaction = Transaction::new(
            stored.id(),
            stored.booking_date(),
            stored.payee().map(str::to_owned),
            splits,
        )
        .map_err(EntryError::Transaction)?
        .with_details_of(&stored);
        sorted.push(transaction);
    }
    if sorted.is_empty() {
        return Err(EntryError::NoSuchEntry);
    }
    book.as_one_action(Action::EditTransaction, |book| {
        for transaction in &sorted {
            book.update_transaction(transaction)?;
        }
        Ok::<(), StoreError>(())
    })?;
    Ok(sorted.len())
}

#[cfg(test)]
mod tests {
    use kassensturz_core::AccountType;

    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::first_start::tests::started;
    use crate::register::tests::TestLocale;
    use crate::transaction_entry::{NewEntry, add, category_choices};

    fn account(book: &mut Book, name: &str) -> String {
        add_account(
            book,
            &NewAccount {
                name,
                account_type: AccountType::Checking,
                currency: "EUR",
                balance: "0,00",
                date: "2026-01-01",
            },
            &TestLocale,
        )
        .expect("account added")
        .to_string()
    }

    fn category(book: &Book, account: &str, money_in: bool) -> String {
        category_choices(book, account, money_in, "")
            .expect("choices")
            .into_iter()
            .next()
            .expect("a category")
            .id
    }

    fn entered(
        book: &mut Book,
        account: &str,
        money_in: bool,
        amount: &str,
        other_party: &str,
        category: &str,
        date: &str,
    ) -> String {
        add(
            book,
            &NewEntry {
                account,
                money_in,
                amount,
                other_party,
                category,
                date,
                note: None,
                tags: None,
            },
            &TestLocale,
        )
        .expect("transaction added")
        .to_string()
    }

    /// A book with three "Corner Shop" purchases, one "Bakery" purchase, a
    /// refund from "Corner Shop" and a sorted purchase, all made the way the
    /// transaction page makes them.
    fn book() -> (Book, String) {
        let mut book = started();
        let checking = account(&mut book, "Checking");
        let food = category(&book, &checking, false);
        entered(
            &mut book,
            &checking,
            false,
            "10,00",
            "Corner Shop",
            "",
            "2026-03-01",
        );
        entered(
            &mut book,
            &checking,
            false,
            "20,50",
            "Corner Shop",
            "",
            "2026-03-05",
        );
        entered(
            &mut book,
            &checking,
            false,
            "5,25",
            "Corner Shop",
            "",
            "2026-03-09",
        );
        entered(
            &mut book,
            &checking,
            false,
            "3,00",
            "Bakery",
            "",
            "2026-03-02",
        );
        entered(
            &mut book,
            &checking,
            true,
            "4,00",
            "Corner Shop",
            "",
            "2026-03-03",
        );
        entered(
            &mut book,
            &checking,
            false,
            "99,00",
            "Landlord",
            &food,
            "2026-03-01",
        );
        (book, checking)
    }

    #[test]
    fn groups_are_by_name_and_direction_the_largest_first_with_exact_sums() {
        let (book, checking) = book();
        let shown = unsorted(&book, &TestLocale).expect("groups");
        assert_eq!(shown.count, 5, "the sorted one doesn't count");
        assert_eq!(shown.more, 0);
        let rows: Vec<_> = shown
            .groups
            .iter()
            .map(|group| {
                (
                    group.name.as_str(),
                    group.money_in,
                    group.count,
                    group.total.exact.as_str(),
                )
            })
            .collect();
        let first = &shown.groups[0];
        assert_eq!((first.name.as_str(), first.count), ("Corner Shop", 3));
        assert_eq!(first.account, checking);
        // The other two hold one each and come by name.
        assert_eq!(rows.len(), 3);
        assert_eq!((rows[1].0, rows[1].1, rows[1].2), ("Bakery", false, 1));
        assert_eq!((rows[2].0, rows[2].1, rows[2].2), ("Corner Shop", true, 1));
        // 10,00 + 20,50 + 5,25, written by the same writer as the register.
        let writer = AmountWriter::new(&book, first_currency(&book), &TestLocale).expect("writer");
        let eur = first_currency(&book);
        assert_eq!(
            first.total,
            writer.text(Money::from_minor_units(-3575, eur))
        );
        assert_eq!(
            rows[2].3,
            writer.text(Money::from_minor_units(400, eur)).exact
        );
    }

    fn first_currency(book: &Book) -> CurrencyCode {
        book.all_accounts()
            .expect("accounts")
            .into_iter()
            .find(|account| account.kind.is_account())
            .expect("an account")
            .currency
    }

    #[test]
    fn sorting_a_group_is_one_change_that_one_undo_takes_back() {
        let (mut book, checking) = book();
        let food = category(&book, &checking, false);
        let before = unsorted(&book, &TestLocale).expect("groups");
        let key = before.groups[0].key.clone();

        assert_eq!(sort(&mut book, &key, &food).expect("sorted"), 3);
        let after = unsorted(&book, &TestLocale).expect("groups");
        assert_eq!(after.count, 2);
        assert!(after.groups.iter().all(|group| group.key != key));
        // The refund of the same name is its own group and stays.
        assert!(
            after
                .groups
                .iter()
                .any(|group| group.name == "Corner Shop" && group.money_in)
        );

        book.undo().expect("undone");
        assert_eq!(unsorted(&book, &TestLocale).expect("groups"), before);
    }

    #[test]
    fn a_group_or_category_that_is_not_there_is_refused_and_nothing_changes() {
        let (mut book, checking) = book();
        let food = category(&book, &checking, false);
        let before = unsorted(&book, &TestLocale).expect("groups");
        for (key, category) in [
            ("out|EUR|Nobody", food.as_str()),
            ("sideways|EUR|Corner Shop", food.as_str()),
            ("out|EUR", food.as_str()),
            ("out|EUR|Corner Shop", "no-such-category"),
            // An account is not a category.
            ("out|EUR|Corner Shop", checking.as_str()),
        ] {
            assert!(
                matches!(sort(&mut book, key, category), Err(EntryError::NoSuchEntry)),
                "{key} / {category}"
            );
        }
        assert_eq!(unsorted(&book, &TestLocale).expect("groups"), before);
    }

    #[test]
    fn a_name_with_the_separator_keeps_its_group() {
        let mut book = started();
        let checking = account(&mut book, "Checking");
        let food = category(&book, &checking, false);
        entered(
            &mut book,
            &checking,
            false,
            "1,00",
            "A|B | C",
            "",
            "2026-03-01",
        );
        let shown = unsorted(&book, &TestLocale).expect("groups");
        assert_eq!(shown.groups[0].name, "A|B | C");
        assert_eq!(
            sort(&mut book, &shown.groups[0].key, &food).expect("sorted"),
            1
        );
        assert_eq!(unsorted(&book, &TestLocale).expect("groups").count, 0);
    }
}
