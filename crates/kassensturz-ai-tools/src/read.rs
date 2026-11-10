// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The read functions themselves (D-170, D-186, D-187).
//!
//! Each reads the book through `kassensturz-store`'s own operations, so a
//! figure here is the figure the app shows. What each one leaves out, on
//! purpose: account numbers and notes, bank links, institutions' and
//! payees' contact details, IBANs, and the IDs of transactions, so a model
//! gets what a person reading the screen would see and no more (T-23).

use std::collections::{BTreeMap, HashMap, HashSet};

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, AmountTest, BookTo, Condition, CurrencyCode,
    Date, MatchMode, Money, Report as _, RuleId, RuleNode, Split, SplitId, TextField, TextOperator,
    Transaction,
};
use kassensturz_store::{Book, StoreError};
use serde_json::{Map, Value, json};

use crate::error::ToolError;
use crate::writer::Writer;

/// The most accounts `list_accounts` lists.
pub(crate) const MAX_ACCOUNTS: usize = 200;
/// The most categories `list_categories` lists.
pub(crate) const MAX_CATEGORIES: usize = 300;
/// The most categories of each side, per currency, `category_totals` lists.
pub(crate) const MAX_TOTALS: usize = 50;
/// The most categories of each side, per currency and month, in the
/// overview: it stays small (D-052).
pub(crate) const OVERVIEW_TOTALS: usize = 15;
/// The most rows a search returns, and how many when the call doesn't say.
pub(crate) const MAX_SEARCH_ROWS: usize = 50;
pub(crate) const DEFAULT_SEARCH_ROWS: usize = 20;
/// The most Rules `list_rules` lists, sub-Rules included, and how deep.
pub(crate) const MAX_RULES: usize = 100;
pub(crate) const MAX_RULE_DEPTH: usize = 5;

/// The accounts and categories of a book, by ID, with each category's
/// path, e.g. "Food › Groceries".
pub(crate) struct Names {
    accounts: HashMap<AccountId, Account>,
    paths: HashMap<AccountId, Vec<String>>,
    /// The categories in the order of the category tree.
    tree: Vec<AccountId>,
}

impl Names {
    pub(crate) fn read(book: &Book) -> Result<Self, StoreError> {
        let accounts = book
            .all_accounts()?
            .into_iter()
            .map(|account| (account.id, account))
            .collect();
        let mut paths = HashMap::new();
        let mut tree = Vec::new();
        for entry in book.categories()? {
            tree.push(entry.category.id);
            paths.insert(entry.category.id, entry.path);
        }
        Ok(Self {
            accounts,
            paths,
            tree,
        })
    }

    fn account(&self, id: AccountId) -> Result<&Account, StoreError> {
        self.accounts.get(&id).ok_or(StoreError::UnknownAccount(id))
    }

    /// A category's path, or an account's name.
    fn name(&self, id: AccountId) -> Result<String, StoreError> {
        Ok(match self.paths.get(&id) {
            Some(path) => path.join(" › "),
            None => self.account(id)?.name.clone(),
        })
    }

    /// The category that category `id` sits in, if any.
    fn parent_category(&self, id: AccountId) -> Result<Option<AccountId>, StoreError> {
        Ok(self
            .account(id)?
            .parent
            .filter(|parent| self.paths.contains_key(parent)))
    }

    /// The user's account `id`, if it is one.
    pub(crate) fn own_account(&self, id: AccountId) -> Result<&Account, ToolError> {
        self.accounts
            .get(&id)
            .filter(|account| account.kind.is_account())
            .ok_or(ToolError::UnknownAccount)
    }

    /// Category `id` and every category below it.
    pub(crate) fn category_and_below(
        &self,
        id: AccountId,
    ) -> Result<HashSet<AccountId>, ToolError> {
        let top = self.paths.get(&id).ok_or(ToolError::UnknownCategory)?;
        Ok(self
            .paths
            .iter()
            .filter(|(_, path)| path.starts_with(top))
            .map(|(&id, _)| id)
            .collect())
    }
}

pub(crate) fn account_type(account_type: AccountType) -> &'static str {
    match account_type {
        AccountType::Checking => "checking",
        AccountType::Savings => "savings",
        AccountType::Cash => "cash",
        AccountType::CreditCard => "credit_card",
    }
}

/// `list_accounts`: the user's accounts with their balances at the end of
/// `today`, by the day the bank booked each entry, as `account_balance`
/// counts them (M6-B168, D-601), and what they add up to per currency. An
/// account with entries dated after today also says what they add
/// (`after_today`, signed), as the accounts screen does (D-587).
pub(crate) fn list_accounts(
    book: &Book,
    w: &mut Writer<'_>,
    include_closed: bool,
    today: Date,
) -> Result<Value, ToolError> {
    let overflow = |_| StoreError::AmountOverflow;
    let accounts = book.accounts(include_closed)?;
    let mut rows = Vec::new();
    let mut together: BTreeMap<String, (Money, Money)> = BTreeMap::new();
    for account in accounts.iter().take(MAX_ACCOUNTS) {
        let parts = book.account_parts(account.id)?;
        let (until_today, later): (Vec<_>, Vec<_>) =
            parts.iter().partition(|part| part.bank_date() <= today);
        let balance = Money::checked_sum(account.currency, until_today.iter().map(|p| p.amount))
            .map_err(overflow)?;
        let after_today = Money::checked_sum(account.currency, later.iter().map(|p| p.amount))
            .map_err(overflow)?;
        let mut row = Map::new();
        row.insert("id".into(), Value::String(account.id.to_string()));
        row.insert("name".into(), w.text(&account.name));
        // Accounts without a type (e.g. from an import) say "other", so an
        // AI is never left guessing what a missing field means.
        let kind = account.account_type.map_or("other", account_type);
        row.insert("type".into(), Value::from(kind));
        if account.kind == AccountKind::Liability {
            row.insert("debt".into(), Value::Bool(true));
        }
        if account.closed {
            row.insert("closed".into(), Value::Bool(true));
        }
        row.insert("currency".into(), Value::from(account.currency.as_str()));
        row.insert("balance".into(), w.amount(balance)?);
        if !later.is_empty() {
            row.insert("after_today".into(), w.amount(after_today)?);
        }
        rows.push(Value::Object(row));
        let (sum, sum_later) = together
            .entry(account.currency.as_str().to_owned())
            .or_insert_with(|| (Money::zero(account.currency), Money::zero(account.currency)));
        *sum = sum.checked_add(balance).map_err(overflow)?;
        *sum_later = sum_later.checked_add(after_today).map_err(overflow)?;
    }
    let mut totals = Vec::new();
    for (currency, (sum, later)) in together {
        let mut total = Map::new();
        total.insert("currency".into(), Value::from(currency));
        total.insert("together".into(), w.amount(sum)?);
        if later != Money::zero(later.currency()) {
            total.insert("after_today".into(), w.amount(later)?);
        }
        totals.push(Value::Object(total));
    }
    let mut result = Map::new();
    result.insert("accounts".into(), Value::Array(rows));
    result.insert("together".into(), Value::Array(totals));
    left_out(&mut result, w, accounts.len(), MAX_ACCOUNTS);
    Ok(Value::Object(result))
}

/// Notes in `result` how many of `found` rows were left out above `limit`.
pub(crate) fn left_out(
    result: &mut Map<String, Value>,
    w: &mut Writer<'_>,
    found: usize,
    limit: usize,
) {
    if found > limit {
        result.insert("left_out".into(), w.count(found - limit));
    }
}

/// `account_balance`: one account's balance at the end of `date`, by the
/// day the bank booked each entry ([`Book::balance_on`]).
pub(crate) fn account_balance(
    book: &Book,
    w: &mut Writer<'_>,
    names: &Names,
    id: AccountId,
    date: Date,
) -> Result<Value, ToolError> {
    let account = names.own_account(id)?;
    let balance = book.balance_on(id, date)?;
    Ok(json!({
        "account": w.text(&account.name),
        "date": date.to_string(),
        "currency": account.currency.as_str(),
        "balance": w.amount(balance)?,
    }))
}

/// The days a total counts, both included.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Period {
    pub(crate) from: Date,
    pub(crate) to: Date,
}

/// Whether `transaction` counts for a total limited to `accounts`: it has
/// a part in one of them (none chosen: every transaction). It counts as a
/// whole, as the report filters and budgets count it (D-350, D-639,
/// M6-B206).
fn in_accounts(transaction: &Transaction, accounts: &[AccountId]) -> bool {
    accounts.is_empty()
        || transaction
            .splits()
            .iter()
            .any(|split| split.account.is_some_and(|id| accounts.contains(&id)))
}

/// The names of `accounts` for a total's `accounts`, in the order given.
fn account_names(
    w: &mut Writer<'_>,
    names: &Names,
    accounts: &[AccountId],
) -> Result<Value, ToolError> {
    let mut list = Vec::with_capacity(accounts.len());
    for &id in accounts {
        list.push(w.text(&names.own_account(id)?.name));
    }
    Ok(Value::Array(list))
}

/// The transactions that may be a move between two of the user's own
/// accounts that is not joined as a Transfer (#594 (a), D-672): two parts,
/// one in one of the user's accounts and one in a category or not sorted
/// yet, and either the bank names one of the user's other accounts as the
/// other party (by its IBAN, compared here and never sent), or another
/// such transaction puts the same amount into, or takes it out of, another
/// of the user's accounts in the same currency at most
/// [`kassensturz_core::MAX_BUSINESS_DAYS`] business days apart (D-030). Only
/// a hint: a Rule or the user may have sorted one side on purpose, and the
/// user decides (D-031).
pub(crate) fn possible_transfers(
    book: &Book,
    names: &Names,
) -> Result<HashSet<kassensturz_core::TransactionId>, ToolError> {
    let squeeze_iban = |iban: &str| -> String {
        iban.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_uppercase)
            .collect()
    };
    let own_ibans: HashMap<String, AccountId> = book
        .bank_links()?
        .into_iter()
        .filter_map(|link| Some((squeeze_iban(link.iban.as_deref()?), link.account)))
        .filter(|(iban, _)| !iban.is_empty())
        .collect();
    let mut found = HashSet::new();
    // (currency, size) -> (date, account, money in?, transaction).
    type Side = (Date, AccountId, bool, kassensturz_core::TransactionId);
    let mut sides: HashMap<(CurrencyCode, u64), Vec<Side>> = HashMap::new();
    for transaction in book.transactions()? {
        let [first, second] = transaction.splits() else {
            continue;
        };
        let own = |split: &Split| -> Result<bool, StoreError> {
            Ok(match split.account {
                Some(id) => names.account(id)?.kind.is_account(),
                None => false,
            })
        };
        let flows = |split: &Split| -> Result<bool, StoreError> {
            Ok(match split.account {
                Some(id) => names.account(id)?.kind.is_category(),
                None => true,
            })
        };
        let part = if own(first)? && flows(second)? {
            first
        } else if own(second)? && flows(first)? {
            second
        } else {
            continue;
        };
        let (Some(account), false) = (part.account, part.amount.is_zero()) else {
            continue;
        };
        if part.foreign.is_some() {
            continue;
        }
        let other_iban = part
            .imported
            .as_ref()
            .and_then(|imported| imported.other_party_iban.as_deref())
            .map(squeeze_iban);
        if other_iban
            .and_then(|iban| own_ibans.get(&iban).copied())
            .is_some_and(|other| other != account)
        {
            found.insert(transaction.id());
        }
        sides
            .entry((
                part.amount.currency(),
                part.amount.minor_units().unsigned_abs(),
            ))
            .or_default()
            .push((
                transaction.date_of(part),
                account,
                !part.amount.is_negative(),
                transaction.id(),
            ));
    }
    for group in sides.values_mut() {
        group.sort_by_key(|side| side.0);
        for (index, &(date, account, money_in, id)) in group.iter().enumerate() {
            for &(later, other_account, other_in, other) in &group[index + 1..] {
                // Sorted by date, so every later side is further away.
                if kassensturz_core::business_days_apart(
                    date,
                    later,
                    kassensturz_core::MAX_BUSINESS_DAYS,
                )
                .is_none()
                {
                    break;
                }
                if account != other_account && money_in != other_in {
                    found.insert(id);
                    found.insert(other);
                }
            }
        }
    }
    Ok(found)
}

/// What money a part of a transaction counts as in the totals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Bucket {
    Category(AccountId),
    UnsortedIn,
    UnsortedOut,
}

/// One row of `category_totals`: the category (`None` for money not sorted
/// yet), what is booked to it alone, and, for a parent whose subcategories
/// hold money, the total with them (M5-B40).
type Row = (Option<AccountId>, Money, Option<Money>);

/// `category_totals`: money in and money out by category in `period`, per
/// currency, only of transactions with a part in `accounts` if any are
/// chosen (M6-B206), counted as the overview counts them (D-021,
/// D-089): money moved between the user's own accounts and starting
/// balances are neither, and each part counts in its transaction's
/// currency, as the money left or reached the user's accounts.
///
/// Of the income and spending, what `possible` transactions hold
/// ([`possible_transfers`]) is said per side and currency under
/// `possible_transfers`, so a model can tell the user that some of it may
/// be moves between their own accounts not joined as Transfers (#594 (a),
/// D-672). The totals stay as the app counts them.
pub(crate) fn category_totals(
    book: &Book,
    w: &mut Writer<'_>,
    names: &Names,
    period: Period,
    accounts: &[AccountId],
    limit: usize,
    possible: &HashSet<kassensturz_core::TransactionId>,
) -> Result<Value, ToolError> {
    let Period { from, to } = period;
    let overflow = |_| StoreError::AmountOverflow;
    let mut sums: HashMap<(CurrencyCode, Bucket), Money> = HashMap::new();
    // (currency, income?) -> (amount, entries) of possible Transfers.
    let mut maybe_moves: HashMap<(CurrencyCode, bool), (Money, usize)> = HashMap::new();
    let mut transfers = 0;
    for transaction in book.transactions_between(from, to)? {
        if !in_accounts(&transaction, accounts) {
            continue;
        }
        let currency = transaction.currency();
        if possible.contains(&transaction.id()) {
            for split in transaction.splits() {
                // The side the totals count the part on, as below: a
                // category by its kind, money not sorted yet by its sign.
                let income = match split.account {
                    None => split.amount.is_negative(),
                    Some(id) => match names.account(id)?.kind {
                        AccountKind::Income => true,
                        AccountKind::Expense => false,
                        _ => continue,
                    },
                };
                // What it adds to that side's figure, signed as the side's
                // rows are: an income part is negative in the book.
                let added = if income {
                    split.amount.checked_neg().map_err(overflow)?
                } else {
                    split.amount
                };
                let (sum, entries) = maybe_moves
                    .entry((currency, income))
                    .or_insert_with(|| (Money::zero(currency), 0));
                *sum = sum.checked_add(added).map_err(overflow)?;
                *entries += 1;
            }
        }
        let mut own_accounts = 0;
        for split in transaction.splits() {
            let bucket = match split.account {
                None if split.amount.is_negative() => Bucket::UnsortedIn,
                None => Bucket::UnsortedOut,
                Some(id) => {
                    let account = names.account(id)?;
                    if account.kind.is_account() {
                        own_accounts += 1;
                    }
                    if !account.kind.is_category() {
                        continue;
                    }
                    Bucket::Category(id)
                }
            };
            let sum = sums
                .entry((currency, bucket))
                .or_insert_with(|| Money::zero(currency));
            *sum = sum.checked_add(split.amount).map_err(overflow)?;
        }
        if own_accounts >= 2 {
            transfers += 1;
        }
    }

    let mut currencies: Vec<CurrencyCode> = sums.keys().map(|(currency, _)| *currency).collect();
    currencies.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
    currencies.dedup();
    let mut per_currency = Vec::new();
    for currency in currencies {
        // (name, unsorted, total), in the order of the category tree so
        // that equal totals keep it after sorting.
        let mut income: Vec<Row> = Vec::new();
        let mut spending: Vec<Row> = Vec::new();
        // What a category and everything below it holds (M5-B40): each
        // category's own total added to all of its parents.
        let mut rolled: HashMap<AccountId, Money> = HashMap::new();
        for (&(sum_currency, bucket), total) in &sums {
            let Bucket::Category(id) = bucket else {
                continue;
            };
            if sum_currency != currency {
                continue;
            }
            let mut at = Some(id);
            // A category tree is shallow; the bound only guards a broken cycle.
            for _ in 0..64 {
                let Some(current) = at else { break };
                let sum = rolled
                    .entry(current)
                    .or_insert_with(|| Money::zero(currency));
                *sum = sum.checked_add(*total).map_err(overflow)?;
                at = names.account(current)?.parent;
            }
        }
        for &id in &names.tree {
            let own = sums
                .get(&(currency, Bucket::Category(id)))
                .copied()
                .unwrap_or_else(|| Money::zero(currency));
            let with_children = rolled
                .get(&id)
                .copied()
                .unwrap_or_else(|| Money::zero(currency));
            if own.is_zero() && with_children.is_zero() {
                continue;
            }
            // Only a parent whose subcategories hold money gets the second figure.
            let below = (with_children != own).then_some(with_children);
            let (own, below) = match names.account(id)?.kind {
                // A spending category takes money: its total is what went out.
                AccountKind::Expense => {
                    spending.push((Some(id), own, below));
                    continue;
                }
                // An income category gives money: its total is negative.
                AccountKind::Income => (
                    own.checked_neg().map_err(overflow)?,
                    below
                        .map(Money::checked_neg)
                        .transpose()
                        .map_err(overflow)?,
                ),
                _ => continue,
            };
            income.push((Some(id), own, below));
        }
        if let Some(total) = sums.get(&(currency, Bucket::UnsortedOut))
            && !total.is_zero()
        {
            spending.push((None, *total, None));
        }
        if let Some(total) = sums.get(&(currency, Bucket::UnsortedIn))
            && !total.is_zero()
        {
            income.push((None, total.checked_neg().map_err(overflow)?, None));
        }
        let money_in = Money::checked_sum(currency, income.iter().map(|(_, own, _)| *own))
            .map_err(overflow)?;
        let money_out = Money::checked_sum(currency, spending.iter().map(|(_, own, _)| *own))
            .map_err(overflow)?;
        let left = money_in.checked_sub(money_out).map_err(overflow)?;
        // Largest first by what the category holds with everything below it
        // (M6-B131), so a parent with nothing booked to itself is not cut
        // off by the limit while its children are listed; a tie keeps the
        // tree order.
        let size = |row: &Row| row.2.unwrap_or(row.1).minor_units();
        income.sort_by_key(|row| core::cmp::Reverse(size(row)));
        spending.sort_by_key(|row| core::cmp::Reverse(size(row)));

        let mut entry = Map::new();
        entry.insert("currency".into(), Value::from(currency.as_str()));
        entry.insert("money_in".into(), w.amount(money_in)?);
        entry.insert("money_out".into(), w.amount(money_out)?);
        entry.insert("left".into(), w.amount(left)?);
        for (key, side) in [("income", &income), ("spending", &spending)] {
            let mut rows = Vec::new();
            for &(id, total, below) in side.iter().take(limit) {
                let mut row = Map::new();
                match id {
                    Some(id) => {
                        row.insert("category".into(), w.text(&names.name(id)?));
                        row.insert("category_id".into(), Value::String(id.to_string()));
                        // So a model sees which rows sit inside another
                        // and never adds a parent's figure to its
                        // children's (#593, D-670).
                        if let Some(parent) = names.parent_category(id)? {
                            row.insert("parent_id".into(), Value::String(parent.to_string()));
                        }
                    }
                    None => {
                        row.insert("unsorted".into(), Value::Bool(true));
                    }
                }
                row.insert("amount".into(), w.amount(total)?);
                if let Some(below) = below {
                    row.insert("amount_with_subcategories".into(), w.amount(below)?);
                }
                rows.push(Value::Object(row));
            }
            entry.insert(key.into(), Value::Array(rows));
            if side.len() > limit {
                entry.insert(format!("{key}_left_out"), w.count(side.len() - limit));
            }
        }
        let mut maybe = Map::new();
        for (key, income) in [("income", true), ("spending", false)] {
            if let Some((amount, entries)) = maybe_moves.get(&(currency, income)) {
                maybe.insert(
                    key.into(),
                    json!({ "amount": w.amount(*amount)?, "entries": w.count(*entries) }),
                );
            }
        }
        if !maybe.is_empty() {
            entry.insert("possible_transfers".into(), Value::Object(maybe));
        }
        per_currency.push(Value::Object(entry));
    }
    let mut result = Map::new();
    result.insert("from".into(), Value::String(from.to_string()));
    result.insert("to".into(), Value::String(to.to_string()));
    if !accounts.is_empty() {
        result.insert("accounts".into(), account_names(w, names, accounts)?);
    }
    result.insert("currencies".into(), Value::Array(per_currency));
    result.insert("transfers".into(), w.count(transfers));
    Ok(Value::Object(result))
}

/// The most payees each side of `payee_totals` lists, and how many it
/// lists when the model doesn't say.
pub(crate) const MAX_PAYEES: usize = 50;
pub(crate) const DEFAULT_PAYEES: usize = 10;

/// What `payee_totals` groups by (#593, D-670).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PayeeGroup {
    /// The payee the user or a Rule set, else the bank's other party name.
    #[default]
    Payee,
    /// The bank's other party name, else the payee: a Rule's display payee
    /// such as "Supermarket" doesn't join different shops into one.
    OtherPartyName,
}

impl PayeeGroup {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Payee => "payee",
            Self::OtherPartyName => "other_party_name",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        [Self::Payee, Self::OtherPartyName]
            .into_iter()
            .find(|group| group.name() == name)
    }
}

/// One row of `payee_totals`: the payee (`None` for entries without one),
/// the amount and the number of transactions.
type PayeeRow = (Option<String>, Money, usize);

/// `payee_totals`: per currency, who the user paid and who paid the user
/// in `period`, net per payee, only what went out of or into `accounts` if
/// any are chosen (M6-B206), largest first (Q-414, D-597), so a
/// question such as "whom did I pay most" is answered by code, not by the
/// model's own arithmetic. Counted as `category_totals` counts: money moved
/// between the user's own accounts and starting balances are left out, and
/// each part counts in its transaction's currency. An entry without a payee
/// counts under `no_payee`. `group` says whether the payee shown or the
/// bank's other party name comes first; each falls back to the other.
pub(crate) fn payee_totals(
    book: &Book,
    w: &mut Writer<'_>,
    names: &Names,
    period: Period,
    accounts: &[AccountId],
    limit: usize,
    group: PayeeGroup,
) -> Result<Value, ToolError> {
    let Period { from, to } = period;
    let overflow = |_| StoreError::AmountOverflow;
    // (currency, payee) -> (money into the user's accounts, net; entries).
    let mut sums: BTreeMap<(String, Option<String>), (Money, usize)> = BTreeMap::new();
    for transaction in book.transactions_between(from, to)? {
        let currency = transaction.currency();
        let mut own = Vec::new();
        let mut flows = false;
        for split in transaction.splits() {
            match split.account {
                None => flows = true,
                Some(id) => {
                    let kind = names.account(id)?.kind;
                    if kind.is_account() {
                        own.push(split);
                    } else if kind.is_category() {
                        flows = true;
                    }
                }
            }
        }
        // A move between own accounts, or a starting balance.
        if own.len() != 1 || !flows {
            continue;
        }
        let part = own[0];
        // The one own account of the entry decides (M6-B206).
        if !accounts.is_empty() && !part.account.is_some_and(|id| accounts.contains(&id)) {
            continue;
        }
        let shown = transaction.payee_of(part);
        let other_party = part
            .imported
            .as_ref()
            .and_then(|imported| imported.other_party_name.as_deref());
        let payee = match group {
            PayeeGroup::Payee => shown.or(other_party),
            PayeeGroup::OtherPartyName => other_party.or(shown),
        }
        .map(str::trim)
        .filter(|payee| !payee.is_empty())
        .map(str::to_owned);
        let (sum, count) = sums
            .entry((currency.as_str().to_owned(), payee))
            .or_insert_with(|| (Money::zero(currency), 0));
        *sum = sum.checked_add(part.amount).map_err(overflow)?;
        *count += 1;
    }
    let mut per_currency: BTreeMap<String, (Vec<PayeeRow>, Vec<PayeeRow>)> = BTreeMap::new();
    for ((currency, payee), (net, count)) in sums {
        let (paid, received) = per_currency.entry(currency).or_default();
        if net.is_negative() {
            paid.push((payee, net.checked_neg().map_err(overflow)?, count));
        } else if !net.is_zero() {
            received.push((payee, net, count));
        }
    }
    let mut currencies = Vec::new();
    for (currency, (mut paid, mut received)) in per_currency {
        let mut entry = Map::new();
        entry.insert("currency".into(), Value::from(currency));
        for (key, side) in [("paid", &mut paid), ("received", &mut received)] {
            // Largest first; a tie keeps the order of the names.
            side.sort_by_key(|(_, total, _)| core::cmp::Reverse(total.minor_units()));
            let mut rows = Vec::new();
            for (payee, total, count) in side.iter().take(limit) {
                let mut row = Map::new();
                match payee {
                    Some(payee) => {
                        row.insert(group.name().into(), w.text(payee));
                    }
                    None => {
                        row.insert("no_payee".into(), Value::Bool(true));
                    }
                }
                row.insert("amount".into(), w.amount(*total)?);
                row.insert("transactions".into(), w.count(*count));
                rows.push(Value::Object(row));
            }
            entry.insert(key.into(), Value::Array(rows));
            if side.len() > limit {
                entry.insert(format!("{key}_left_out"), w.count(side.len() - limit));
            }
        }
        currencies.push(Value::Object(entry));
    }
    let mut result = Map::new();
    result.insert("from".into(), Value::String(from.to_string()));
    result.insert("to".into(), Value::String(to.to_string()));
    if !accounts.is_empty() {
        result.insert("accounts".into(), account_names(w, names, accounts)?);
    }
    if group != PayeeGroup::Payee {
        result.insert("group_by".into(), Value::from(group.name()));
    }
    result.insert("currencies".into(), Value::Array(currencies));
    Ok(Value::Object(result))
}

/// What `search_transactions` looks for.
pub(crate) struct Search {
    /// Lowercase text to find in a payee, note or bank text.
    pub(crate) text: Option<String>,
    pub(crate) account: Option<AccountId>,
    pub(crate) category: Option<AccountId>,
    pub(crate) from: Option<Date>,
    pub(crate) to: Option<Date>,
    pub(crate) limit: usize,
    /// How many matching rows to skip first, for the next page (#593).
    pub(crate) offset: usize,
    /// The order of the rows (#187).
    pub(crate) sort: Sort,
    /// Only entries of at least / at most this size (#187).
    pub(crate) min_amount: Option<Size>,
    pub(crate) max_amount: Option<Size>,
    /// Leave out moves between the user's own accounts and starting
    /// balances, so "biggest expenses" are spending (#187).
    pub(crate) skip_transfers: bool,
    /// Only transactions with a part that has no account yet, i.e. no
    /// category (the row's `unsorted`).
    pub(crate) unsorted_only: bool,
    /// Whether rows say who sorted them (`sorted_by`, #569, D-672): only
    /// when the user lets the model read the Rules too.
    pub(crate) sorted_by: bool,
}

/// The parts a Rule booked and the user didn't change since, with that Rule
/// (`None`: removed since), as the store keeps them ([`Book::rule_marks`]).
type Marks = HashMap<SplitId, Option<RuleId>>;

/// How `search_transactions` orders its rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Sort {
    /// By day, newest first.
    #[default]
    Newest,
    /// Most money out first, then less, then money in.
    LargestOut,
    /// Most money in first, then less, then money out.
    LargestIn,
}

impl Sort {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Newest => "newest",
            Self::LargestOut => "largest_out",
            Self::LargestIn => "largest_in",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        [Self::Newest, Self::LargestOut, Self::LargestIn]
            .into_iter()
            .find(|sort| sort.name() == name)
    }
}

/// An amount's size as the model wrote it, e.g. `49.99`, compared exactly
/// with the minor units of an entry (no floating point, D-065).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Size {
    /// The digits without the point.
    digits: i128,
    /// How many of them are after the point.
    places: u32,
}

/// The most digits before and after the point of a [`Size`].
const SIZE_WHOLE_DIGITS: usize = 12;
const SIZE_PLACES: usize = 6;

impl Size {
    /// Reads `[0-9]+` or `[0-9]+.[0-9]+`: no sign (money in and out both
    /// count by size), no grouping, no exponent.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
        let plain = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        if whole.is_empty()
            || whole.len() > SIZE_WHOLE_DIGITS
            || fraction.len() > SIZE_PLACES
            || (text.contains('.') && fraction.is_empty())
            || !plain(whole)
            || !plain(fraction)
        {
            return None;
        }
        Some(Self {
            digits: format!("{whole}{fraction}").parse().ok()?,
            places: u32::try_from(fraction.len()).ok()?,
        })
    }

    /// How the size of `minor_units` (of a currency with `decimals` places)
    /// compares with this size.
    fn compare(self, minor_units: i64, decimals: u8) -> core::cmp::Ordering {
        // |minor| / 10^decimals against digits / 10^places, cross-multiplied.
        let entry = i128::from(minor_units).abs() * 10_i128.pow(self.places);
        let wanted = self.digits * 10_i128.pow(u32::from(decimals));
        entry.cmp(&wanted)
    }
}

/// Whether `part` of `transaction` is money moved to or from another account
/// of the user, or a starting balance, rather than spending or income: some
/// other part is neither a category nor unsorted. The row says the same with
/// `transfer` or `starting_balance`.
fn is_move(names: &Names, transaction: &Transaction, part: &Split) -> Result<bool, ToolError> {
    for split in transaction.splits() {
        if core::ptr::eq(split, part) {
            continue;
        }
        if let Some(id) = split.account
            && !names.account(id)?.kind.is_category()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether `transaction` or its part `part` holds `text`, ignoring case.
fn holds(transaction: &Transaction, part: &Split, text: &str) -> bool {
    let found =
        |value: Option<&str>| value.is_some_and(|value| value.to_lowercase().contains(text));
    found(transaction.payee_of(part))
        || found(transaction.note())
        || transaction.splits().iter().any(|split| {
            found(split.note.as_deref())
                || split.imported.as_ref().is_some_and(|imported| {
                    found(imported.other_party_name.as_deref())
                        || found(imported.remittance_information.as_deref())
                })
        })
}

/// `search_transactions`: the entries of the user's accounts that match,
/// in the order `search.sort` names (newest first by default), one row per
/// account an entry is in, at most `search.limit` of them, with how many
/// matched in all and how many rows were left out.
pub(crate) fn search_transactions(
    book: &Book,
    w: &mut Writer<'_>,
    names: &Names,
    search: &Search,
) -> Result<Value, ToolError> {
    let possible = possible_transfers(book, names)?;
    if let Some(account) = search.account {
        names.own_account(account)?;
    }
    let categories = search
        .category
        .map(|category| names.category_and_below(category))
        .transpose()?;
    let transactions = match search.account {
        Some(account) => book.transactions_of(account)?,
        None => book.transactions()?,
    };
    // (date, transaction, part): every entry of an account that matches.
    let mut found: Vec<(Date, &Transaction, &Split, &Account)> = Vec::new();
    for transaction in &transactions {
        if search.unsorted_only
            && !transaction
                .splits()
                .iter()
                .any(|split| split.account.is_none())
        {
            continue;
        }
        if let Some(categories) = &categories
            && !transaction
                .splits()
                .iter()
                .any(|split| split.account.is_some_and(|id| categories.contains(&id)))
        {
            continue;
        }
        for part in transaction.splits() {
            let Some(id) = part.account else { continue };
            let account = names.account(id)?;
            // The day the account shows it with, as in its register (D-108).
            let date = transaction.date_of(part);
            if !account.kind.is_account()
                || search.account.is_some_and(|wanted| wanted != id)
                || search.from.is_some_and(|from| date < from)
                || search.to.is_some_and(|to| date > to)
            {
                continue;
            }
            if search
                .text
                .as_deref()
                .is_some_and(|text| !holds(transaction, part, text))
            {
                continue;
            }
            if search.skip_transfers && is_move(names, transaction, part)? {
                continue;
            }
            if search.min_amount.is_some() || search.max_amount.is_some() {
                let minor = part.account_amount().minor_units();
                let decimals = w.digits(account.currency)?;
                if search
                    .min_amount
                    .is_some_and(|min| min.compare(minor, decimals).is_lt())
                    || search
                        .max_amount
                        .is_some_and(|max| max.compare(minor, decimals).is_gt())
                {
                    continue;
                }
            }
            found.push((date, transaction, part, account));
        }
    }
    let marks: Marks = if search.sorted_by {
        book.rule_marks()?
            .into_iter()
            .map(|mark| (mark.split, mark.rule))
            .collect()
    } else {
        Marks::new()
    };
    let marks = search.sorted_by.then_some(&marks);
    // Newest first; the store's order (by booking date, then ID) breaks
    // ties, reversed.
    found.reverse();
    found.sort_by_key(|&(date, ..)| core::cmp::Reverse(date));
    // Amounts of different currencies can't be compared, so they stay in
    // groups by currency. The sort is stable: equal amounts stay newest
    // first.
    fn amount<'a>(entry: &(Date, &Transaction, &Split, &'a Account)) -> (&'a str, i64) {
        let (_, _, part, account) = *entry;
        (
            account.currency.as_str(),
            part.account_amount().minor_units(),
        )
    }
    match search.sort {
        Sort::Newest => {}
        Sort::LargestOut => found.sort_by(|a, b| amount(a).cmp(&amount(b))),
        Sort::LargestIn => found.sort_by(|a, b| {
            let (a, b) = (amount(a), amount(b));
            a.0.cmp(b.0).then(b.1.cmp(&a.1))
        }),
    }

    let mut rows = Vec::new();
    for &(date, transaction, part, account) in found.iter().skip(search.offset).take(search.limit) {
        rows.push(search_row(
            w,
            names,
            (date, transaction, part, account),
            marks,
            possible.contains(&transaction.id()),
        )?);
    }
    let shown = rows.len();
    let mut result = json!({
        "matches": w.count(found.len()),
        "order": search.sort.name(),
        "rows": rows,
    });
    if search.offset > 0 {
        result["offset"] = w.count(search.offset);
    }
    // So a model never takes the rows shown for all of the matches (#187):
    // how many come after these, and where the next page starts (#593).
    let after = search.offset.saturating_add(shown);
    if found.len() > after {
        result["rows_left_out"] = w.count(found.len() - after);
        result["next_offset"] = w.count(after);
    }
    Ok(result)
}

/// `text` with every run of whitespace as one space, trimmed.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One entry of a search: the day, the account, the amount as seen from it
/// (money in is positive), who and what it was, and where the money came
/// from or went.
fn search_row(
    w: &mut Writer<'_>,
    names: &Names,
    (date, transaction, part, account): (Date, &Transaction, &Split, &Account),
    marks: Option<&Marks>,
    possible_transfer: bool,
) -> Result<Value, ToolError> {
    let mut row = Map::new();
    row.insert("date".into(), Value::String(date.to_string()));
    // For `propose_changes`, which gives a transaction a category.
    row.insert(
        "transaction_id".into(),
        Value::String(transaction.id().to_string()),
    );
    row.insert("account".into(), w.text(&account.name));
    // Every name comes with its ID, so two of the same name are never
    // mixed up (#593, D-670).
    row.insert("account_id".into(), Value::String(account.id.to_string()));
    row.insert("amount".into(), w.amount(part.account_amount())?);
    row.insert("currency".into(), Value::from(account.currency.as_str()));
    let imported = part.imported.as_ref();
    let other_party = imported.and_then(|imported| imported.other_party_name.as_deref());
    let payee = transaction.payee_of(part);
    if let Some(payee) = payee.or(other_party) {
        row.insert("payee".into(), w.text(payee));
    }
    // What the bank named the other party, which Rules test, when the payee
    // shown says something else (A1 of the US1 tool review).
    if let (Some(payee), Some(other_party)) = (payee, other_party)
        && payee.trim() != other_party.trim()
    {
        row.insert("other_party_name".into(), w.text(other_party));
    }
    let memo = part.note.as_deref().or(transaction.note());
    let bank_text = imported.and_then(|imported| imported.remittance_information.as_deref());
    // Each text once (#593, D-670): a memo the bank text holds is left out,
    // and so is a bank text the memo holds, whitespace aside. The bank text
    // is what Rules test, so it stays when the two are the same.
    let (memo, bank_text) = match (memo, bank_text) {
        (Some(memo), Some(bank)) => {
            let (memo_words, bank_words) = (squeeze(memo), squeeze(bank));
            if bank_words.contains(&memo_words) {
                (None, Some(bank))
            } else if memo_words.contains(&bank_words) {
                (Some(memo), None)
            } else {
                (Some(memo), Some(bank))
            }
        }
        other => other,
    };
    if let Some(memo) = memo {
        row.insert("note".into(), w.text(memo));
    }
    if let Some(bank_text) = bank_text {
        row.insert("bank_text".into(), w.text(bank_text));
    }

    let mut categories: Vec<(String, AccountId)> = Vec::new();
    let mut transfer = None;
    let mut unsorted = false;
    let mut starting = false;
    for split in transaction.splits() {
        if core::ptr::eq(split, part) {
            continue;
        }
        match split.account {
            None => unsorted = true,
            Some(id) => {
                let other = names.account(id)?;
                if other.kind.is_category() {
                    if !categories.iter().any(|(_, known)| *known == id) {
                        categories.push((names.name(id)?, id));
                    }
                } else if other.kind.is_account() {
                    transfer.get_or_insert(other);
                } else {
                    starting = true;
                }
            }
        }
    }
    match categories.as_slice() {
        [] => {}
        [(name, id)] => {
            row.insert("category".into(), w.text(name));
            row.insert("category_id".into(), Value::String(id.to_string()));
        }
        several => {
            let names = several.iter().map(|(name, _)| w.text(name)).collect();
            let ids = several
                .iter()
                .map(|(_, id)| Value::String(id.to_string()))
                .collect();
            row.insert("categories".into(), Value::Array(names));
            row.insert("category_ids".into(), Value::Array(ids));
        }
    }
    if let Some(other) = transfer {
        row.insert("transfer".into(), w.text(&other.name));
        row.insert(
            "transfer_account_id".into(),
            Value::String(other.id.to_string()),
        );
    }
    if unsorted {
        row.insert("unsorted".into(), Value::Bool(true));
    }
    if starting {
        row.insert("starting_balance".into(), Value::Bool(true));
    }
    // It may be a move between the user's own accounts that isn't joined
    // as a Transfer, so it counts as income or spending (#594 (a), D-672).
    if possible_transfer {
        row.insert("possible_transfer".into(), Value::Bool(true));
    }
    // Who sorted it (#569, D-672), for an entry with a category or a
    // Transfer: a Rule while the store still marks the part it booked, by
    // hand without such a mark (the user's own choice, or a change after
    // the Rule). A Rule that only set the payee sorted nothing.
    if let Some(marks) = marks
        && (!categories.is_empty() || transfer.is_some())
    {
        let mark = transaction
            .splits()
            .iter()
            .find_map(|split| marks.get(&split.id));
        match mark {
            Some(rule) => {
                row.insert("sorted_by".into(), Value::from("rule"));
                if let Some(rule) = rule {
                    row.insert("rule_id".into(), Value::String(rule.to_string()));
                }
            }
            None => {
                row.insert("sorted_by".into(), Value::from("by_hand"));
            }
        }
    }
    Ok(Value::Object(row))
}

/// `list_categories`: the income and spending categories, as a tree in
/// the app's order.
pub(crate) fn list_categories(book: &Book, w: &mut Writer<'_>) -> Result<Value, ToolError> {
    let entries = book.categories()?;
    let mut rows = Vec::new();
    for entry in entries.iter().take(MAX_CATEGORIES) {
        let mut row = Map::new();
        row.insert("id".into(), Value::String(entry.category.id.to_string()));
        row.insert("name".into(), w.text(&entry.path.join(" › ")));
        let kind = if entry.category.kind == AccountKind::Income {
            "income"
        } else {
            "spending"
        };
        row.insert("kind".into(), Value::from(kind));
        if entry.category.closed {
            row.insert("closed".into(), Value::Bool(true));
        }
        rows.push(Value::Object(row));
    }
    let mut result = Map::new();
    result.insert("categories".into(), Value::Array(rows));
    left_out(&mut result, w, entries.len(), MAX_CATEGORIES);
    Ok(Value::Object(result))
}

/// `list_rules`: the Rules that sort new transactions (D-015), each with
/// its sub-Rules.
pub(crate) fn list_rules(
    book: &Book,
    w: &mut Writer<'_>,
    names: &Names,
) -> Result<Value, ToolError> {
    let tree = book.rules()?;
    let mut budget = MAX_RULES;
    let mut total = 0;
    count_rules(&tree, &mut total);
    let rules = rule_rows(w, names, &tree, 1, &mut budget)?;
    let mut result = Map::new();
    result.insert("rules".into(), Value::Array(rules));
    // Rules past the limit, or deeper than the depth limit.
    let listed = MAX_RULES - budget;
    if total > listed {
        result.insert("left_out".into(), w.count(total - listed));
    }
    Ok(Value::Object(result))
}

fn count_rules(nodes: &[RuleNode], total: &mut usize) {
    for node in nodes {
        *total += 1;
        count_rules(&node.sub_rules, total);
    }
}

fn rule_rows(
    w: &mut Writer<'_>,
    names: &Names,
    nodes: &[RuleNode],
    depth: usize,
    budget: &mut usize,
) -> Result<Vec<Value>, ToolError> {
    let mut rows = Vec::new();
    for node in nodes {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let rule = &node.rule;
        let mut row = Map::new();
        row.insert("id".into(), Value::String(rule.id.to_string()));
        row.insert("name".into(), w.text(&rule.name));
        let mode = match rule.match_mode {
            MatchMode::All => "all",
            MatchMode::Any => "any",
        };
        row.insert("match".into(), Value::from(mode));
        let mut conditions = Vec::new();
        for condition in &rule.conditions {
            conditions.push(condition_row(w, names, condition)?);
        }
        row.insert("conditions".into(), Value::Array(conditions));
        // Whether the conditions above are all of them, exactly as stored,
        // so edit_rules may replace them (M6-B200).
        row.insert(
            "conditions_complete".into(),
            Value::Bool(rule.conditions.iter().all(crate::changes::writable)),
        );
        match rule.actions.book_to {
            Some(BookTo::Category(id)) => {
                row.insert("category".into(), w.text(&names.name(id)?));
                row.insert("category_id".into(), Value::String(id.to_string()));
            }
            Some(BookTo::Transfer(id)) => {
                row.insert("transfer".into(), w.text(&names.name(id)?));
                row.insert("transfer_account_id".into(), Value::String(id.to_string()));
            }
            None => {}
        }
        if let Some(payee) = &rule.actions.payee {
            row.insert("payee".into(), w.text(payee));
        }
        if !node.sub_rules.is_empty() && depth < MAX_RULE_DEPTH {
            let sub = rule_rows(w, names, &node.sub_rules, depth + 1, budget)?;
            row.insert("sub_rules".into(), Value::Array(sub));
        }
        rows.push(Value::Object(row));
    }
    Ok(rows)
}

fn condition_row(
    w: &mut Writer<'_>,
    names: &Names,
    condition: &Condition,
) -> Result<Value, ToolError> {
    Ok(match condition {
        Condition::Text {
            field,
            operator,
            value,
            case_sensitive,
        } => {
            let field_name = match field {
                TextField::OtherPartyName => "other_party_name",
                TextField::OtherPartyIban => "other_party_iban",
                TextField::RemittanceInformation => "bank_text",
                TextField::Payee => "payee",
            };
            let test = match operator {
                TextOperator::Contains => "contains",
                TextOperator::Equals => "equals",
                TextOperator::StartsWith => "starts_with",
                TextOperator::EndsWith => "ends_with",
                TextOperator::Matches => "matches_pattern",
            };
            let mut row = Map::new();
            row.insert("field".into(), Value::from(field_name));
            row.insert("test".into(), Value::from(test));
            // No IBAN reaches a model, not even part of one (D-186).
            if *field == TextField::OtherPartyIban {
                row.insert("text_left_out".into(), Value::Bool(true));
            } else {
                row.insert("text".into(), w.text(value));
            }
            if *case_sensitive {
                row.insert("case_sensitive".into(), Value::Bool(true));
            }
            Value::Object(row)
        }
        Condition::Amount(AmountTest::Equals(amount)) => json!({
            "amount_equals": w.amount(*amount)?,
            "currency": amount.currency().as_str(),
        }),
        Condition::Amount(AmountTest::Between { min, max }) => json!({
            "amount_from": w.amount(*min)?,
            "amount_to": w.amount(*max)?,
            "currency": min.currency().as_str(),
        }),
        Condition::Amount(AmountTest::MoneyIn) => json!({ "money_in": true }),
        Condition::Amount(AmountTest::MoneyOut) => json!({ "money_out": true }),
        Condition::BookingDate { first, last } => json!({
            "booked_from": first.to_string(),
            "booked_to": last.to_string(),
        }),
        Condition::Currency(currency) => json!({ "currency": currency.as_str() }),
        Condition::Account(id) => json!({
            "account": w.text(&names.name(*id)?),
            "account_id": id.to_string(),
        }),
    })
}

/// The most budgets `list_budgets` lists.
pub(crate) const MAX_BUDGETS: usize = 100;

/// `list_budgets`: the budgets that run on `today` or start later, each
/// with its categories, the IDs of the accounts it counts from if not all,
/// how often
/// it repeats, what it plans for its current
/// (or first) period and whether it carries over (D-358, D-396). With
/// `spending`, which needs the reading of Transactions as well, the
/// current period's spent and left come with it, as `budget_utilization`
/// counts them. Ended budgets are left out: they can't be changed.
pub(crate) fn list_budgets(
    book: &Book,
    w: &mut Writer<'_>,
    today: Date,
    spending: bool,
) -> Result<Value, ToolError> {
    use kassensturz_report::{BudgetGrouping, Filter, Period, budget_utilization};

    let paths: HashMap<AccountId, Vec<String>> = book
        .categories()?
        .into_iter()
        .map(|entry| (entry.category.id, entry.path))
        .collect();
    let budgets: Vec<_> = book
        .budgets()?
        .into_iter()
        .filter(|budget| budget.last_day.is_none_or(|last| last >= today))
        .collect();
    let mut failed = HashMap::new();
    let figures = if spending && !budgets.is_empty() {
        let period = Period::new(today, today).map_err(|_| ToolError::BadPeriod)?;
        let count = |chosen: Vec<kassensturz_core::Budget>| {
            let source = kassensturz_report::Source::read_for_budgets(book, chosen, period, today)?;
            budget_utilization(
                &source,
                period,
                today,
                BudgetGrouping::Budget,
                &Filter::default(),
                None,
            )
        };
        let rows = match count(budgets.clone()) {
            Ok(rows) => rows,
            // One budget whose figures can't be counted doesn't hide the
            // others': each is counted on its own, and the one that fails
            // says so in its row (M6-B198).
            Err(_) => {
                let mut rows = Vec::new();
                for budget in budgets.iter().take(MAX_BUDGETS) {
                    match count(vec![budget.clone()]) {
                        Ok(own) => rows.extend(own),
                        Err(error) => {
                            failed.insert(budget.id, ToolError::BudgetFigures(error).report());
                        }
                    }
                }
                rows
            }
        };
        rows.into_iter()
            .filter_map(|row| row.periods.first().map(|shown| (row.budget, *shown)))
            .collect()
    } else {
        HashMap::new()
    };
    let mut rows = Vec::new();
    for budget in budgets.iter().take(MAX_BUDGETS) {
        let mut row = Map::new();
        row.insert("budget_id".into(), Value::String(budget.id.to_string()));
        // The name the user gave it, if any (D-664): text of the book, as
        // the category paths are.
        if let Some(name) = &budget.name {
            row.insert("name".into(), w.text(name));
        }
        // Every category with everything inside it, and the accounts it
        // counts from if it doesn't count them all (D-641): by ID only, as
        // their names come with list_accounts and its own switch.
        row.insert(
            "category_ids".into(),
            Value::Array(
                budget
                    .categories
                    .iter()
                    .map(|id| Value::String(id.to_string()))
                    .collect(),
            ),
        );
        let mut categories = Vec::with_capacity(budget.categories.len());
        for id in &budget.categories {
            let path = paths
                .get(id)
                .map(|path| path.join(" › "))
                .unwrap_or_default();
            categories.push(w.text(&path));
        }
        row.insert("categories".into(), Value::Array(categories));
        if !budget.accounts.is_empty() {
            row.insert(
                "account_ids".into(),
                Value::Array(
                    budget
                        .accounts
                        .iter()
                        .map(|id| Value::String(id.to_string()))
                        .collect(),
                ),
            );
        }
        row.insert(
            "every".into(),
            Value::from(crate::changes::every_word(budget.every)),
        );
        if let Some(day) = budget.every.salary_day() {
            row.insert("salary_day".into(), Value::from(day));
        }
        let day = today.max(budget.first_day);
        if budget.first_day > today {
            row.insert("starts".into(), Value::String(budget.first_day.to_string()));
        }
        if let Some(last) = budget.last_day {
            row.insert("ends".into(), Value::String(last.to_string()));
        }
        if let Some((first, _)) = budget.period_containing(day)
            && let Some(amount) = budget.amount_for(first)
        {
            row.insert("amount".into(), w.amount(amount.amount)?);
            row.insert("carry_over".into(), Value::Bool(amount.carry_over));
        }
        if let Some(shown) = figures.get(&budget.id) {
            let mut current = Map::new();
            current.insert("from".into(), Value::String(shown.first_day.to_string()));
            current.insert("to".into(), Value::String(shown.last_day.to_string()));
            current.insert("available".into(), w.amount(shown.available)?);
            current.insert("spent".into(), w.amount(shown.spent)?);
            current.insert("left".into(), w.amount(shown.left)?);
            row.insert("current_period".into(), Value::Object(current));
        } else if let Some(report) = failed.get(&budget.id) {
            // Only the codes, never a text the report may hold.
            let mut error = Map::new();
            error.insert("error".into(), Value::from(report.code));
            if let Some(cause) = &report.cause {
                error.insert("cause".into(), Value::from(cause.code));
            }
            row.insert("current_period".into(), Value::Object(error));
        }
        rows.push(Value::Object(row));
    }
    let mut result = Map::new();
    result.insert("budgets".into(), Value::Array(rows));
    // What this period spent and has left needs the reading of
    // Transactions: say which field is missing and where the user turns it
    // on (D-676).
    if !spending && !budgets.is_empty() {
        result.insert(
            "hidden".into(),
            json!({
                "fields": ["current_period"],
                "setting": crate::error::AI_PERMISSIONS.json(),
            }),
        );
    }
    left_out(&mut result, w, budgets.len(), MAX_BUDGETS);
    Ok(Value::Object(result))
}

#[cfg(test)]
mod tests {
    use core::cmp::Ordering;

    use super::*;

    #[test]
    fn a_size_is_compared_exactly_with_the_minor_units_of_its_currency() {
        let size = |text: &str| Size::parse(text).expect(text);
        assert_eq!(size("49.99").compare(4999, 2), Ordering::Equal);
        assert_eq!(size("49.99").compare(-4999, 2), Ordering::Equal);
        assert_eq!(size("49.99").compare(-5000, 2), Ordering::Greater);
        assert_eq!(size("49.99").compare(4998, 2), Ordering::Less);
        assert_eq!(size("50").compare(5000, 2), Ordering::Equal);
        // More places than the currency has: 0.005 is below one cent.
        assert_eq!(size("0.005").compare(1, 2), Ordering::Greater);
        assert_eq!(size("0.010").compare(1, 2), Ordering::Equal);
        // A currency without decimals (yen).
        assert_eq!(size("500").compare(-500, 0), Ordering::Equal);
        assert_eq!(size("500.5").compare(501, 0), Ordering::Greater);
        assert_eq!(size("0").compare(i64::MIN, 2), Ordering::Greater);
    }

    #[test]
    fn only_plain_decimal_text_is_a_size() {
        for good in ["0", "7", "100", "49.99", "0.000001", "123456789012"] {
            assert!(Size::parse(good).is_some(), "{good}");
        }
        for bad in [
            "",
            "-1",
            "+1",
            "1e3",
            "5.",
            ".5",
            "1,5",
            " 1",
            "1 ",
            "1.2.3",
            "1234567890123",
            "0.1234567",
            "０",
            "1_0",
        ] {
            assert!(Size::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn the_sort_names_round_trip() {
        for sort in [Sort::Newest, Sort::LargestOut, Sort::LargestIn] {
            assert_eq!(Sort::from_name(sort.name()), Some(sort));
        }
        assert_eq!(Sort::from_name("biggest"), None);
        assert_eq!(Sort::default(), Sort::Newest);
    }
}
