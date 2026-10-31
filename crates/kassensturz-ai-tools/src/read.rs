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
    Date, MatchMode, Money, RuleNode, Split, TextField, TextOperator, Transaction,
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

fn account_type(account_type: AccountType) -> &'static str {
    match account_type {
        AccountType::Checking => "checking",
        AccountType::Savings => "savings",
        AccountType::Cash => "cash",
        AccountType::CreditCard => "credit_card",
    }
}

/// `list_accounts`: the user's accounts with their balances, and what they
/// add up to per currency.
pub(crate) fn list_accounts(
    book: &Book,
    w: &mut Writer<'_>,
    include_closed: bool,
) -> Result<Value, ToolError> {
    let accounts = book.accounts(include_closed)?;
    let mut rows = Vec::new();
    let mut together: BTreeMap<String, Money> = BTreeMap::new();
    for account in accounts.iter().take(MAX_ACCOUNTS) {
        let balance = book.balance(account.id)?;
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
        rows.push(Value::Object(row));
        let sum = together
            .entry(account.currency.as_str().to_owned())
            .or_insert_with(|| Money::zero(account.currency));
        *sum = sum
            .checked_add(balance)
            .map_err(|_| StoreError::AmountOverflow)?;
    }
    let mut totals = Vec::new();
    for (currency, sum) in together {
        totals.push(json!({ "currency": currency, "together": w.amount(sum)? }));
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

/// What money a part of a transaction counts as in the totals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Bucket {
    Category(AccountId),
    UnsortedIn,
    UnsortedOut,
}

/// `category_totals`: money in and money out by category from `from` to
/// `last`, per currency, counted as the overview counts them (D-021,
/// D-089): money moved between the user's own accounts and starting
/// balances are neither, and each part counts in its transaction's
/// currency, as the money left or reached the user's accounts.
pub(crate) fn category_totals(
    book: &Book,
    w: &mut Writer<'_>,
    names: &Names,
    from: Date,
    to: Date,
    limit: usize,
) -> Result<Value, ToolError> {
    let overflow = |_| StoreError::AmountOverflow;
    let mut sums: HashMap<(CurrencyCode, Bucket), Money> = HashMap::new();
    let mut transfers = 0;
    for transaction in book.transactions_between(from, to)? {
        let currency = transaction.currency();
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
        let mut income: Vec<(Option<AccountId>, Money)> = Vec::new();
        let mut spending: Vec<(Option<AccountId>, Money)> = Vec::new();
        for &id in &names.tree {
            let Some(total) = sums.get(&(currency, Bucket::Category(id))) else {
                continue;
            };
            if total.is_zero() {
                continue;
            }
            match names.account(id)?.kind {
                // A spending category takes money: its total is what went out.
                AccountKind::Expense => spending.push((Some(id), *total)),
                // An income category gives money: its total is negative.
                AccountKind::Income => {
                    income.push((Some(id), total.checked_neg().map_err(overflow)?));
                }
                _ => {}
            }
        }
        if let Some(total) = sums.get(&(currency, Bucket::UnsortedOut))
            && !total.is_zero()
        {
            spending.push((None, *total));
        }
        if let Some(total) = sums.get(&(currency, Bucket::UnsortedIn))
            && !total.is_zero()
        {
            income.push((None, total.checked_neg().map_err(overflow)?));
        }
        let money_in = Money::checked_sum(currency, income.iter().map(|(_, total)| *total))
            .map_err(overflow)?;
        let money_out = Money::checked_sum(currency, spending.iter().map(|(_, total)| *total))
            .map_err(overflow)?;
        let left = money_in.checked_sub(money_out).map_err(overflow)?;
        // Largest first, from the exact amounts; a tie keeps the tree order.
        income.sort_by_key(|(_, total)| core::cmp::Reverse(total.minor_units()));
        spending.sort_by_key(|(_, total)| core::cmp::Reverse(total.minor_units()));

        let mut entry = Map::new();
        entry.insert("currency".into(), Value::from(currency.as_str()));
        entry.insert("money_in".into(), w.amount(money_in)?);
        entry.insert("money_out".into(), w.amount(money_out)?);
        entry.insert("left".into(), w.amount(left)?);
        for (key, side) in [("income", &income), ("spending", &spending)] {
            let mut rows = Vec::new();
            for &(id, total) in side.iter().take(limit) {
                let mut row = Map::new();
                match id {
                    Some(id) => {
                        row.insert("category".into(), w.text(&names.name(id)?));
                        row.insert("category_id".into(), Value::String(id.to_string()));
                    }
                    None => {
                        row.insert("unsorted".into(), Value::Bool(true));
                    }
                }
                row.insert("amount".into(), w.amount(total)?);
                rows.push(Value::Object(row));
            }
            entry.insert(key.into(), Value::Array(rows));
            if side.len() > limit {
                entry.insert(format!("{key}_left_out"), w.count(side.len() - limit));
            }
        }
        per_currency.push(Value::Object(entry));
    }
    let mut result = Map::new();
    result.insert("from".into(), Value::String(from.to_string()));
    result.insert("to".into(), Value::String(to.to_string()));
    result.insert("currencies".into(), Value::Array(per_currency));
    result.insert("transfers".into(), w.count(transfers));
    Ok(Value::Object(result))
}

/// What `search_transactions` looks for.
pub(crate) struct Search {
    /// Lowercase text to find in a payee, memo or bank text.
    pub(crate) text: Option<String>,
    pub(crate) account: Option<AccountId>,
    pub(crate) category: Option<AccountId>,
    pub(crate) from: Option<Date>,
    pub(crate) to: Option<Date>,
    pub(crate) limit: usize,
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
}

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
    for &(date, transaction, part, account) in found.iter().take(search.limit) {
        rows.push(search_row(w, names, (date, transaction, part, account))?);
    }
    let mut result = json!({
        "matches": w.count(found.len()),
        "order": search.sort.name(),
        "rows": rows,
    });
    // So a model never takes the rows shown for all of the matches (#187).
    if found.len() > search.limit {
        result["rows_left_out"] = w.count(found.len() - search.limit);
    }
    Ok(result)
}

/// One entry of a search: the day, the account, the amount as seen from it
/// (money in is positive), who and what it was, and where the money came
/// from or went.
fn search_row(
    w: &mut Writer<'_>,
    names: &Names,
    (date, transaction, part, account): (Date, &Transaction, &Split, &Account),
) -> Result<Value, ToolError> {
    let mut row = Map::new();
    row.insert("date".into(), Value::String(date.to_string()));
    // For `propose_changes`, which gives a transaction a category.
    row.insert(
        "transaction_id".into(),
        Value::String(transaction.id().to_string()),
    );
    row.insert("account".into(), w.text(&account.name));
    row.insert("amount".into(), w.amount(part.account_amount())?);
    row.insert("currency".into(), Value::from(account.currency.as_str()));
    let imported = part.imported.as_ref();
    if let Some(payee) = transaction
        .payee_of(part)
        .or_else(|| imported.and_then(|imported| imported.other_party_name.as_deref()))
    {
        row.insert("payee".into(), w.text(payee));
    }
    let memo = part.note.as_deref().or(transaction.note());
    if let Some(memo) = memo {
        row.insert("memo".into(), w.text(memo));
    }
    // The bank's text, unless the memo says the same.
    if let Some(bank_text) = imported
        .and_then(|imported| imported.remittance_information.as_deref())
        .filter(|&bank_text| Some(bank_text) != memo)
    {
        row.insert("bank_text".into(), w.text(bank_text));
    }

    let mut categories: Vec<String> = Vec::new();
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
                    let name = names.name(id)?;
                    if !categories.contains(&name) {
                        categories.push(name);
                    }
                } else if other.kind.is_account() {
                    transfer.get_or_insert(other.name.as_str());
                } else {
                    starting = true;
                }
            }
        }
    }
    match categories.as_slice() {
        [] => {}
        [one] => {
            row.insert("category".into(), w.text(one));
        }
        several => {
            let several = several.iter().map(|name| w.text(name)).collect();
            row.insert("categories".into(), Value::Array(several));
        }
    }
    if let Some(other) = transfer {
        row.insert("transfer".into(), w.text(other));
    }
    if unsorted {
        row.insert("unsorted".into(), Value::Bool(true));
    }
    if starting {
        row.insert("starting_balance".into(), Value::Bool(true));
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
        match rule.actions.book_to {
            Some(BookTo::Category(id)) => {
                row.insert("category".into(), w.text(&names.name(id)?));
            }
            Some(BookTo::Transfer(id)) => {
                row.insert("transfer".into(), w.text(&names.name(id)?));
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
        Condition::Account(id) => json!({ "account": w.text(&names.name(*id)?) }),
    })
}

/// The most budgets `list_budgets` lists.
pub(crate) const MAX_BUDGETS: usize = 100;

/// `list_budgets`: the budgets that run on `today` or start later, each
/// with its category, how often it repeats, what it plans for its current
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
    let figures = if spending && !budgets.is_empty() {
        let period = Period::new(today, today).map_err(|_| ToolError::BadPeriod)?;
        let source = kassensturz_report::Source::read_with_budgets(book, period, today)
            .map_err(read_error)?;
        budget_utilization(
            &source,
            period,
            today,
            BudgetGrouping::Budget,
            &Filter::default(),
        )
        .map_err(read_error)?
        .into_iter()
        .filter_map(|row| row.periods.first().map(|shown| (row.budget, *shown)))
        .collect()
    } else {
        HashMap::new()
    };
    let mut rows = Vec::new();
    for budget in budgets.iter().take(MAX_BUDGETS) {
        let mut row = Map::new();
        row.insert("budget_id".into(), Value::String(budget.id.to_string()));
        row.insert(
            "category_id".into(),
            Value::String(budget.category.to_string()),
        );
        let path = paths
            .get(&budget.category)
            .map(|path| path.join(" › "))
            .unwrap_or_default();
        row.insert("category".into(), w.text(&path));
        row.insert(
            "every".into(),
            Value::from(crate::changes::every_word(budget.every)),
        );
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
        }
        rows.push(Value::Object(row));
    }
    let mut result = Map::new();
    result.insert("budgets".into(), Value::Array(rows));
    left_out(&mut result, w, budgets.len(), MAX_BUDGETS);
    Ok(Value::Object(result))
}

/// A report error of `list_budgets`: the book's own as it is.
fn read_error(error: kassensturz_report::ReportError) -> ToolError {
    match error {
        kassensturz_report::ReportError::Store(error) => ToolError::Store(error),
        _ => ToolError::BadPeriod,
    }
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
