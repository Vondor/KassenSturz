// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The overview (M3.9, D-122): the accounts side by side with what is in
//! them (D-021), one month's money in and money out by category, and the
//! newest transactions. Money moved between the user's own accounts and
//! starting balances count as neither income nor spending (D-021, D-089).
//! Every total is added up here, per currency and from the exact amounts,
//! and reaches QML as text (D-065).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{Account, AccountId, AccountKind, CurrencyCode, Date, Money, TransactionId};
use kassensturz_store::{Book, StoreError};

use crate::accounts::{self, CurrencySection};
use crate::format::Locale;
use crate::register::{AmountWriter, Entry, Shown, latest_entries};

/// How many transactions the overview lists.
const LATEST: usize = 5;

/// A calendar month, named by its first day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Month(Date);

impl Month {
    /// The month `date` is in.
    pub fn of(date: Date) -> Self {
        Self(date.first_of_month())
    }

    /// Its first day.
    pub fn first_day(self) -> Date {
        self.0
    }

    /// Its last day.
    pub fn last_day(self) -> Date {
        self.0.last_of_month()
    }

    /// The month before, if the calendar has one.
    pub fn earlier(self) -> Option<Self> {
        self.0.yesterday().ok().map(Self::of)
    }

    /// The month after, if the calendar has one.
    pub fn later(self) -> Option<Self> {
        self.last_day().tomorrow().ok().map(Self::of)
    }
}

/// What a category got in the month.
#[derive(Debug, PartialEq, Eq)]
pub struct CategoryTotal {
    /// The category's name with the categories above it, e.g. "Food ›
    /// Groceries"; empty for money not sorted into a category yet.
    pub name: String,
    /// Whether this is the money not sorted into a category yet, which the
    /// UI names (D-034).
    pub unsorted: bool,
    /// The total, without a sign: for spending what went out, for income
    /// what came in. A category whose month ended the other way round, e.g.
    /// a refund larger than what was spent, has a minus sign.
    pub amount: Shown,
}

/// One month's money in and money out in one currency.
#[derive(Debug, PartialEq, Eq)]
pub struct MonthTotals {
    /// The ISO 4217 code.
    pub currency: String,
    /// Everything booked to income categories, or to no category yet and
    /// received.
    pub money_in: Shown,
    /// Everything booked to spending categories, or to no category yet and
    /// paid.
    pub money_out: Shown,
    /// Money in minus money out, without a sign.
    pub left: Shown,
    /// Whether more went out than came in, so `left` is what was spent on
    /// top of what came in.
    pub overspent: bool,
    /// Where the money came from, largest first.
    pub income: Vec<CategoryTotal>,
    /// Where the money went, largest first.
    pub spending: Vec<CategoryTotal>,
}

/// A recent transaction on the overview, as seen from one of the user's
/// accounts.
#[derive(Debug, PartialEq, Eq)]
pub struct LatestRow {
    /// The ID of the account it is seen from, as text.
    pub account_id: String,
    /// That account's name.
    pub account: String,
    /// The transaction's ID as text.
    pub id: String,
    /// The booking date.
    pub date: String,
    /// Who the money came from or went to; may be empty.
    pub other_party: String,
    /// Its category, or for a Transfer the other account's name.
    pub category: String,
    /// The amount, as seen from the account.
    pub amount: Shown,
    /// Whether money came in.
    pub incoming: bool,
    /// What kind of entry it is, as a key the UI translates
    /// (`register::EntryKind`).
    pub kind: &'static str,
}

/// What the overview shows.
#[derive(Debug, PartialEq, Eq)]
pub struct OverviewView {
    /// The month shown, e.g. "October 2026".
    pub month: String,
    /// The same month as `YYYY-MM`, which the charts and cards take as a
    /// period (M6a.8).
    pub month_key: String,
    /// Whether an earlier month has transactions.
    pub has_earlier: bool,
    /// Whether a later month has transactions, or is the current one.
    pub has_later: bool,
    /// Whether the month shown is the current one.
    pub this_month: bool,
    /// Whether nothing came in or went out in the month.
    pub empty: bool,
    /// The open accounts per currency, with their balances today.
    pub accounts: Vec<CurrencySection>,
    /// The month's money in and out, per currency: the currencies of the
    /// accounts first, in their order, then any other the month used.
    pub totals: Vec<MonthTotals>,
    /// How many transactions of the month moved money between the user's
    /// own accounts, which count as neither income nor spending.
    pub transfers: usize,
    /// The newest transactions, newest first, without starting balances.
    pub latest: Vec<LatestRow>,
}

/// What money a part of a transaction counts as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Bucket {
    /// Booked to this category.
    Category(AccountId),
    /// Not sorted into a category yet, and received.
    UnsortedIn,
    /// Not sorted into a category yet, and paid.
    UnsortedOut,
}

/// Reads the overview for `month` from `book`, for `locale`.
pub fn view(book: &Book, month: Month, locale: &impl Locale) -> Result<OverviewView, StoreError> {
    let all = book.all_accounts()?;
    // The accounts' currencies first, in the order the accounts were added
    // (as `accounts::view` lists them), then any other the month used.
    let mut currencies: Vec<CurrencyCode> = Vec::new();
    for account in &all {
        if account.kind.is_account() && !account.closed && !currencies.contains(&account.currency) {
            currencies.push(account.currency);
        }
    }
    let accounts: HashMap<AccountId, Account> = all
        .into_iter()
        .map(|account| (account.id, account))
        .collect();

    // Every part booked to a category, or to none yet, in the currency the
    // transaction counts in: the money as it left or reached the user's
    // accounts. A part's amount in its category's own currency (D-043)
    // doesn't change what was paid.
    let mut sums: HashMap<(CurrencyCode, Bucket), Money> = HashMap::new();
    let mut transfers = 0;
    for transaction in book.transactions_between(month.first_day(), month.last_day())? {
        let currency = transaction.currency();
        let mut own_accounts = 0;
        for split in transaction.splits() {
            let bucket = match split.account {
                None if split.amount.is_negative() => Bucket::UnsortedIn,
                None => Bucket::UnsortedOut,
                Some(id) => {
                    let account = accounts.get(&id).ok_or(StoreError::UnknownAccount(id))?;
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
            *sum = sum
                .checked_add(split.amount)
                .map_err(|_| StoreError::AmountOverflow)?;
        }
        if own_accounts >= 2 {
            transfers += 1;
        }
    }

    let mut used: Vec<CurrencyCode> = sums.keys().map(|(currency, _)| *currency).collect();
    used.sort_unstable();
    used.dedup();
    used.retain(|used| !currencies.contains(used));
    currencies.extend(used);

    let paths: Vec<(AccountId, String)> = book
        .categories()?
        .into_iter()
        .map(|entry| (entry.category.id, entry.path.join(" › ")))
        .collect();
    let mut totals = Vec::new();
    for currency in currencies {
        totals.push(month_totals(
            book, currency, &sums, &paths, &accounts, locale,
        )?);
    }
    let empty = totals
        .iter()
        .all(|totals| totals.income.is_empty() && totals.spending.is_empty());

    let today = Month::of(locale.today());
    let range = book.booking_date_range()?;
    let has_earlier = range.is_some_and(|(first, _)| first < month.first_day());
    let has_later = month < today || range.is_some_and(|(_, last)| last > month.last_day());

    Ok(OverviewView {
        month: locale.month_and_year(month.first_day()),
        month_key: format!(
            "{:04}-{:02}",
            month.first_day().year(),
            month.first_day().month()
        ),
        has_earlier,
        has_later,
        this_month: month == today,
        empty,
        accounts: accounts::view(book, locale)?.sections,
        totals,
        transfers,
        latest: latest(book, &accounts, locale)?,
    })
}

/// The month's totals in `currency`, from `sums`.
fn month_totals(
    book: &Book,
    currency: CurrencyCode,
    sums: &HashMap<(CurrencyCode, Bucket), Money>,
    paths: &[(AccountId, String)],
    accounts: &HashMap<AccountId, Account>,
    locale: &impl Locale,
) -> Result<MonthTotals, StoreError> {
    let writer = AmountWriter::new(book, currency, locale)?;
    let overflow = |_| StoreError::AmountOverflow;
    let mut income: Vec<(String, bool, Money)> = Vec::new();
    let mut spending: Vec<(String, bool, Money)> = Vec::new();
    // In the order of the category tree, so categories with the same total
    // keep it after sorting.
    for (id, path) in paths {
        let Some(total) = sums.get(&(currency, Bucket::Category(*id))) else {
            continue;
        };
        if total.is_zero() {
            continue;
        }
        match accounts.get(id).map(|account| account.kind) {
            // A spending category takes money, so its total is what went out.
            Some(AccountKind::Expense) => spending.push((path.clone(), false, *total)),
            // An income category gives money, so its total is negative.
            Some(AccountKind::Income) => {
                income.push((path.clone(), false, total.checked_neg().map_err(overflow)?));
            }
            _ => {}
        }
    }
    if let Some(total) = sums.get(&(currency, Bucket::UnsortedOut))
        && !total.is_zero()
    {
        spending.push((String::new(), true, *total));
    }
    if let Some(total) = sums.get(&(currency, Bucket::UnsortedIn)) {
        income.push((String::new(), true, total.checked_neg().map_err(overflow)?));
    }
    let money_in =
        Money::checked_sum(currency, income.iter().map(|(.., total)| *total)).map_err(overflow)?;
    let money_out = Money::checked_sum(currency, spending.iter().map(|(.., total)| *total))
        .map_err(overflow)?;
    let left = money_in.checked_sub(money_out).map_err(overflow)?;
    let overspent = left.is_negative();
    let left = if overspent {
        left.checked_neg().map_err(overflow)?
    } else {
        left
    };
    // Largest first, from the exact amounts (D-065); a tie keeps the tree
    // order.
    let largest_first = |totals: &mut Vec<(String, bool, Money)>| {
        totals.sort_by_key(|(.., total)| core::cmp::Reverse(total.minor_units()));
    };
    largest_first(&mut income);
    largest_first(&mut spending);
    let written = |totals: Vec<(String, bool, Money)>| -> Vec<CategoryTotal> {
        totals
            .into_iter()
            .map(|(name, unsorted, total)| CategoryTotal {
                name,
                unsorted,
                amount: writer.text(total),
            })
            .collect()
    };
    Ok(MonthTotals {
        currency: currency.to_string(),
        money_in: writer.text(money_in),
        money_out: writer.text(money_out),
        left: writer.text(left),
        overspent,
        income: written(income),
        spending: written(spending),
    })
}

/// The newest transactions of the open accounts, newest first, each once:
/// money moved between two accounts is listed from the side with the later
/// date (D-108), or from the account it reached if both show the same.
fn latest(
    book: &Book,
    accounts: &HashMap<AccountId, Account>,
    locale: &impl Locale,
) -> Result<Vec<LatestRow>, StoreError> {
    let mut found: Vec<(&Account, Entry)> = Vec::new();
    for account in accounts.values() {
        if !account.kind.is_account() || account.closed {
            continue;
        }
        // Only an account's newest few can be among the newest of all, so
        // the rest of its transactions are never read (M6-B30).
        for entry in latest_entries(book, account.id, accounts, LATEST)? {
            found.push((account, entry));
        }
    }
    // Newest first, on the same day the one added last, as in a register
    // (D-067); the two sides of a Transfer on one day stay together, the
    // one money reached first.
    found.sort_by_key(|(_, entry)| {
        core::cmp::Reverse((entry.date, entry.transaction, entry.amount.minor_units()))
    });
    let mut listed: HashSet<TransactionId> = HashSet::new();
    let mut latest = Vec::new();
    for (account, entry) in found {
        if latest.len() == LATEST {
            break;
        }
        if !listed.insert(entry.transaction) {
            continue;
        }
        let writer = AmountWriter::new(book, account.currency, locale)?;
        let row = entry.row(&writer, account.kind);
        latest.push(LatestRow {
            account_id: account.id.to_string(),
            account: account.name.clone(),
            id: row.id,
            date: row.date,
            other_party: row.other_party,
            category: row.category,
            amount: row.amount,
            incoming: row.incoming,
            kind: row.kind,
        });
    }
    Ok(latest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{NewAccount, add_account};
    use crate::categories::{NewCategory, add_category};
    use crate::first_start::tests::started;
    use crate::number_format::{DecimalPlaces, NumberFormat, UserLocale};
    use crate::parts_entry::{PartInput, PartsInput, save};
    use crate::register::tests::TestLocale;
    use crate::transaction_entry::{NewEntry, add, category_choices};
    use crate::transfer_prompt::join;
    use kassensturz_core::AccountType;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    /// October 2026, the month of `TestLocale::today`.
    fn october() -> Month {
        Month::of(TestLocale.today())
    }

    fn date(year: i16, month: i8, day: i8) -> Date {
        Date::new(year, month, day).expect("valid date")
    }

    /// Adds an account the way the accounts screen does (M3.3), with
    /// `balance` on `date`, and returns its ID as text.
    fn account(
        book: &mut Book,
        name: &str,
        account_type: AccountType,
        currency: &str,
        balance: &str,
        date: &str,
    ) -> String {
        add_account(
            book,
            &NewAccount {
                name,
                account_type,
                currency,
                balance,
                date,
            },
            &TestLocale,
        )
        .expect("account added")
        .to_string()
    }

    /// The ID (as text) of the category at `path` as the transaction page
    /// offers it (M3.5).
    fn category(book: &Book, account: &str, money_in: bool, path: &str) -> String {
        category_choices(book, account, money_in, "")
            .expect("choices")
            .into_iter()
            .find(|choice| choice.path == path)
            .unwrap_or_else(|| panic!("no category {path}"))
            .id
    }

    /// Adds a transaction the way the transaction page does (M3.5) and
    /// returns its ID as text.
    fn entered(
        book: &mut Book,
        account: &str,
        money_in: bool,
        amount: &str,
        other_party: &str,
        category_path: &str,
        date: &str,
    ) -> String {
        let category = if category_path.is_empty() {
            String::new()
        } else {
            category(book, account, money_in, category_path)
        };
        add(
            book,
            &NewEntry {
                account,
                money_in,
                amount,
                other_party,
                category: &category,
                date,
            },
            &TestLocale,
        )
        .expect("transaction added")
        .to_string()
    }

    fn overview(book: &Book, month: Month) -> OverviewView {
        view(book, month, &TestLocale).expect("overview")
    }

    fn names_and_amounts(totals: &[CategoryTotal]) -> Vec<(&str, &str)> {
        totals
            .iter()
            .map(|total| (total.name.as_str(), total.amount.exact.as_str()))
            .collect()
    }

    #[test]
    fn shows_one_months_money_in_and_out_by_category() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "EUR",
            "1.000,00",
            "2026-09-28",
        );
        entered(
            &mut book,
            &checking,
            true,
            "2.500,00",
            "Example Employer Ltd.",
            "Salary",
            "2026-10-01",
        );
        entered(
            &mut book,
            &checking,
            false,
            "800,00",
            "Example Housing",
            "Rent",
            "2026-10-03",
        );
        entered(
            &mut book,
            &checking,
            false,
            "53,20",
            "Corner Grocery",
            "Food › Groceries",
            "2026-10-05",
        );
        entered(
            &mut book,
            &checking,
            false,
            "40,00",
            "Corner Grocery",
            "Food › Groceries",
            "2026-10-12",
        );
        entered(&mut book, &checking, false, "12,50", "", "", "2026-10-13");
        // Last month, so not in October's totals.
        entered(
            &mut book,
            &checking,
            false,
            "30,00",
            "Corner Grocery",
            "Food › Groceries",
            "2026-09-30",
        );

        let shown = overview(&book, october());
        assert_eq!(shown.month, "October 2026");
        assert!(shown.this_month);
        assert!(shown.has_earlier);
        assert!(!shown.has_later);
        assert!(!shown.empty);
        assert_eq!(shown.transfers, 0);

        // The accounts with what they hold today, not only this month.
        assert_eq!(shown.accounts.len(), 1);
        assert_eq!(shown.accounts[0].accounts[0].name, "Checking account");
        assert_eq!(shown.accounts[0].accounts[0].balance.exact, "2564,30 €");

        let [totals] = shown.totals.as_slice() else {
            panic!("one currency");
        };
        assert_eq!(totals.currency, "EUR");
        assert_eq!(totals.money_in.exact, "2500,00 €");
        assert_eq!(totals.money_out.exact, "905,70 €");
        assert_eq!(totals.left.exact, "1594,30 €");
        assert!(!totals.overspent);
        // Largest first; the money without a category is named by the UI.
        assert_eq!(
            names_and_amounts(&totals.spending),
            [
                ("Rent", "800,00 €"),
                ("Food › Groceries", "93,20 €"),
                ("", "12,50 €")
            ]
        );
        assert!(totals.spending[2].unsorted);
        assert_eq!(names_and_amounts(&totals.income), [("Salary", "2500,00 €")]);

        // The newest five, newest first, without the starting balance.
        let latest: Vec<(&str, &str)> = shown
            .latest
            .iter()
            .map(|row| (row.date.as_str(), row.category.as_str()))
            .collect();
        assert_eq!(
            latest,
            [
                ("2026-10-13", ""),
                ("2026-10-12", "Groceries"),
                ("2026-10-05", "Groceries"),
                ("2026-10-03", "Rent"),
                ("2026-10-01", "Salary"),
            ]
        );
        assert_eq!(shown.latest[0].account, "Checking account");
        assert_eq!(shown.latest[0].account_id, checking);
        assert!(!shown.latest[0].incoming);
        assert_eq!(shown.latest[0].kind, "simple");
        assert_eq!(shown.latest[0].amount.exact, "-12,50 €");

        // September has only the one purchase, and the current month after it.
        let september = overview(&book, october().earlier().expect("september"));
        assert_eq!(september.month, "September 2026");
        assert!(!september.this_month);
        assert!(!september.has_earlier);
        assert!(september.has_later);
        assert_eq!(september.totals[0].money_out.exact, "30,00 €");
        assert_eq!(september.totals[0].money_in.exact, "0,00 €");
        assert_eq!(september.totals[0].left.exact, "30,00 €");
        assert!(september.totals[0].overspent);
        // The accounts and the latest transactions don't change with the month.
        assert_eq!(september.accounts, shown.accounts);
        assert_eq!(september.latest, shown.latest);
    }

    #[test]
    fn money_moved_between_accounts_is_left_out() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "EUR",
            "500,00",
            "2026-10-01",
        );
        let savings = account(
            &mut book,
            "Savings",
            AccountType::Savings,
            "EUR",
            "",
            "2026-10-01",
        );
        let out = entered(
            &mut book,
            &checking,
            false,
            "100,00",
            "Own transfer",
            "",
            "2026-10-05",
        );
        let into = entered(
            &mut book,
            &savings,
            true,
            "100,00",
            "Own transfer",
            "",
            "2026-10-06",
        );
        entered(
            &mut book,
            &checking,
            false,
            "20,00",
            "Corner Grocery",
            "Food › Groceries",
            "2026-10-07",
        );

        // Typed as two transactions, the move counts as money not sorted yet.
        let before = overview(&book, october());
        assert_eq!(before.transfers, 0);
        assert_eq!(before.totals[0].money_out.exact, "120,00 €");
        assert_eq!(before.totals[0].money_in.exact, "100,00 €");

        // Joined through the Transfer prompt (M3.7), it is neither.
        join(&mut book, &out, &into).expect("joined");
        let after = overview(&book, october());
        assert_eq!(after.transfers, 1);
        assert_eq!(after.totals[0].money_out.exact, "20,00 €");
        assert_eq!(after.totals[0].money_in.exact, "0,00 €");
        assert_eq!(
            names_and_amounts(&after.totals[0].spending),
            [("Food › Groceries", "20,00 €")]
        );
        assert!(after.totals[0].income.is_empty());
        assert_eq!(after.totals[0].left.exact, "20,00 €");
        assert!(after.totals[0].overspent);

        // The accounts show the move, and the latest list shows it once,
        // from the side dated later (D-108).
        assert_eq!(after.accounts[0].accounts[0].balance.exact, "380,00 €");
        assert_eq!(after.accounts[0].accounts[1].balance.exact, "100,00 €");
        assert_eq!(after.accounts[0].together.exact, "480,00 €");
        let latest: Vec<(&str, &str, &str)> = after
            .latest
            .iter()
            .map(|row| (row.account.as_str(), row.kind, row.category.as_str()))
            .collect();
        assert_eq!(
            latest,
            [
                ("Checking account", "simple", "Groceries"),
                ("Savings", "transfer", "Checking account"),
            ]
        );
        assert_eq!(after.latest[1].amount.exact, "100,00 €");
        assert!(after.latest[1].incoming);
    }

    /// The newest transactions the way they were found before M6-B30: every
    /// transaction of every open account read, then the newest five of them.
    fn latest_from_everything(book: &Book) -> Vec<(String, String, String, String)> {
        let accounts: HashMap<AccountId, Account> = book
            .all_accounts()
            .expect("accounts")
            .into_iter()
            .map(|account| (account.id, account))
            .collect();
        let mut found: Vec<(&Account, Entry)> = Vec::new();
        for account in accounts.values() {
            if !account.kind.is_account() || account.closed {
                continue;
            }
            for entry in crate::register::entries(book, account.id).expect("entries") {
                if !entry.starting_balance() {
                    found.push((account, entry));
                }
            }
        }
        found.sort_by_key(|(_, entry)| {
            core::cmp::Reverse((entry.date, entry.transaction, entry.amount.minor_units()))
        });
        let mut listed = HashSet::new();
        found
            .into_iter()
            .filter(|(_, entry)| listed.insert(entry.transaction))
            .take(LATEST)
            .map(|(account, entry)| {
                (
                    account.name.clone(),
                    entry.transaction.to_string(),
                    entry.date.to_string(),
                    entry.amount.minor_units().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn the_latest_list_is_what_reading_every_transaction_would_give() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "EUR",
            "500,00",
            "2026-01-01",
        );
        let savings = account(
            &mut book,
            "Savings",
            AccountType::Savings,
            "EUR",
            "50,00",
            "2026-01-01",
        );
        // Many more than the list shows, in both accounts, some on the same
        // day, and one move between them that is joined into a Transfer.
        for day in 1..=28 {
            entered(
                &mut book,
                &checking,
                day % 5 == 0,
                &format!("{day},50"),
                &format!("Shop {day}"),
                if day % 5 == 0 {
                    ""
                } else {
                    "Food › Groceries"
                },
                &format!("2026-09-{day:02}"),
            );
        }
        for day in [3, 3, 9, 20] {
            entered(
                &mut book,
                &savings,
                true,
                "5,00",
                "Interest",
                "",
                &format!("2026-10-{day:02}"),
            );
        }
        let out = entered(
            &mut book,
            &checking,
            false,
            "100,00",
            "Own transfer",
            "",
            "2026-10-05",
        );
        let into = entered(
            &mut book,
            &savings,
            true,
            "100,00",
            "Own transfer",
            "",
            "2026-10-06",
        );
        join(&mut book, &out, &into).expect("joined");

        let shown = overview(&book, october());
        let shown: Vec<_> = shown
            .latest
            .iter()
            .map(|row| {
                (
                    row.account.clone(),
                    row.id.clone(),
                    row.date.clone(),
                    row.amount.exact.clone(),
                )
            })
            .collect();
        let expected = latest_from_everything(&book);
        assert_eq!(shown.len(), LATEST);
        assert_eq!(
            shown.iter().map(|row| &row.1).collect::<Vec<_>>(),
            expected.iter().map(|row| &row.1).collect::<Vec<_>>()
        );
        assert_eq!(
            shown.iter().map(|row| &row.0).collect::<Vec<_>>(),
            expected.iter().map(|row| &row.0).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_part_in_another_currency_counts_what_was_paid() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "EUR",
            "500,00",
            "2026-10-01",
        );
        add_category(
            &mut book,
            &NewCategory {
                name: "Travel",
                income: false,
                parent: "",
                currency: "USD",
            },
            EUR,
        )
        .expect("category added");
        let travel = book
            .categories()
            .expect("categories")
            .into_iter()
            .find(|entry| entry.category.name == "Travel")
            .expect("travel")
            .category
            .id
            .to_string();
        let hotel = entered(
            &mut book,
            &checking,
            false,
            "92,50",
            "Example Hotel",
            "",
            "2026-10-10",
        );
        // Broken down on the Parts page (M3.6): the whole bill is Travel,
        // 100 dollars on the receipt (D-043).
        save(
            &mut book,
            &checking,
            &hotel,
            &PartsInput {
                money_in: false,
                total: "92,50",
                other_party: "Example Hotel",
                date: "2026-10-10",
                parts: &[PartInput {
                    id: "",
                    category: &travel,
                    amount: "",
                    foreign_amount: "100",
                }],
            },
            &TestLocale,
        )
        .expect("parts saved");

        // What went out is the 92,50 € the account paid; no dollar section
        // appears for a category that counts in dollars.
        let shown = overview(&book, october());
        assert_eq!(shown.totals.len(), 1);
        assert_eq!(shown.totals[0].currency, "EUR");
        assert_eq!(shown.totals[0].money_out.exact, "92,50 €");
        assert_eq!(
            names_and_amounts(&shown.totals[0].spending),
            [("Travel", "92,50 €")]
        );
        // The register shows a Part in another currency as Parts (D-118).
        assert_eq!(shown.latest[0].kind, "parts");
    }

    #[test]
    fn totals_add_up_the_exact_amounts_before_rounding() {
        let mut book = started();
        let cash = account(
            &mut book,
            "Cash",
            AccountType::Cash,
            "EUR",
            "10,00",
            "2026-10-01",
        );
        for _ in 0..3 {
            entered(
                &mut book,
                &cash,
                false,
                "0,40",
                "Kiosk",
                "Food › Groceries",
                "2026-10-05",
            );
        }
        let locale = UserLocale::new(
            TestLocale,
            NumberFormat {
                decimal_places: DecimalPlaces::None,
                ..NumberFormat::default()
            },
        );
        // Each 0,40 € shows as 0 €; their total of 1,20 € as 1 €, not 0 €.
        let shown = view(&book, october(), &locale).expect("overview");
        let groceries = &shown.totals[0].spending[0].amount;
        assert_eq!(
            (groceries.text.as_str(), groceries.exact.as_str()),
            ("1 €", "1,20 €")
        );
        assert_eq!(shown.totals[0].money_out.text, "1 €");
        assert_eq!(
            shown.latest[0].amount,
            Shown {
                text: "0 €".into(),
                exact: "-0,40 €".into()
            }
        );
        let balance = &shown.accounts[0].accounts[0].balance;
        assert_eq!(
            (balance.text.as_str(), balance.exact.as_str()),
            ("9 €", "8,80 €")
        );
    }

    #[test]
    fn a_debt_larger_than_everything_reads_you_owe() {
        let mut book = started();
        account(
            &mut book,
            "Cash",
            AccountType::Cash,
            "EUR",
            "50,00",
            "2026-10-01",
        );
        account(
            &mut book,
            "Credit card",
            AccountType::CreditCard,
            "EUR",
            "300,00",
            "2026-10-01",
        );
        let shown = overview(&book, october());
        assert_eq!(shown.accounts[0].accounts[1].balance.exact, "300,00 €");
        assert!(shown.accounts[0].accounts[1].owes);
        assert_eq!(shown.accounts[0].together.exact, "250,00 €");
        assert!(shown.accounts[0].together_owes);
        // Starting balances are neither money in nor money out.
        assert!(shown.empty);
        assert_eq!(shown.totals[0].money_in.exact, "0,00 €");
        assert_eq!(shown.totals[0].money_out.exact, "0,00 €");
        assert!(shown.latest.is_empty());
    }

    #[test]
    fn each_currency_has_its_own_totals() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "EUR",
            "100,00",
            "2026-10-01",
        );
        let dollars = account(
            &mut book,
            "Dollar account",
            AccountType::Checking,
            "USD",
            "100,00",
            "2026-10-01",
        );
        entered(
            &mut book,
            &checking,
            false,
            "20,00",
            "Corner Grocery",
            "Food › Groceries",
            "2026-10-05",
        );
        entered(
            &mut book,
            &dollars,
            false,
            "30,00",
            "Example Diner",
            "",
            "2026-10-06",
        );
        let shown = overview(&book, october());
        let currencies: Vec<&str> = shown
            .totals
            .iter()
            .map(|totals| totals.currency.as_str())
            .collect();
        assert_eq!(currencies, ["EUR", "USD"]);
        assert_eq!(shown.totals[0].money_out.exact, "20,00 €");
        assert_eq!(shown.totals[1].money_out.exact, "30,00 €");
        assert_eq!(shown.accounts.len(), 2);
    }

    #[test]
    fn an_undone_transaction_leaves_the_overview() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "EUR",
            "100,00",
            "2026-10-01",
        );
        entered(
            &mut book,
            &checking,
            false,
            "20,00",
            "Corner Grocery",
            "Food › Groceries",
            "2026-10-05",
        );
        assert_eq!(
            overview(&book, october()).totals[0].money_out.exact,
            "20,00 €"
        );
        // Taken back the way the side menu does (M3.8).
        book.undo().expect("undone").expect("a change");
        let shown = overview(&book, october());
        assert!(shown.empty);
        assert_eq!(shown.totals[0].money_out.exact, "0,00 €");
        assert_eq!(shown.accounts[0].accounts[0].balance.exact, "100,00 €");
        assert!(shown.latest.is_empty());
    }

    #[test]
    fn a_later_month_is_reachable_when_it_has_transactions() {
        let mut book = started();
        let checking = account(
            &mut book,
            "Checking account",
            AccountType::Checking,
            "EUR",
            "100,00",
            "2026-10-01",
        );
        assert!(!overview(&book, october()).has_later);
        entered(
            &mut book,
            &checking,
            true,
            "10,00",
            "",
            "Salary",
            "2026-12-24",
        );
        let shown = overview(&book, october());
        assert!(shown.has_later);
        let november = overview(&book, october().later().expect("november"));
        assert!(november.has_later);
        assert!(november.has_earlier);
        assert!(november.empty);
        let december = overview(&book, Month::of(date(2026, 12, 1)));
        assert!(!december.has_later);
        assert_eq!(december.totals[0].money_in.exact, "10,00 €");
    }

    #[test]
    fn months_follow_the_calendar() {
        let december = Month::of(date(2026, 12, 31));
        assert_eq!(december.first_day(), date(2026, 12, 1));
        assert_eq!(december.last_day(), date(2026, 12, 31));
        assert_eq!(december.later(), Some(Month::of(date(2027, 1, 1))));
        assert_eq!(Month::of(date(2027, 1, 15)).earlier(), Some(december));
        assert_eq!(Month::of(date(2026, 2, 3)).last_day(), date(2026, 2, 28));
        assert!(Month::of(date(2026, 10, 1)) < Month::of(date(2026, 11, 30)));
    }
}
