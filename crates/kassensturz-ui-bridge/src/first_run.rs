// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The first-run mock (M1.M, D-068): add an account, add a transaction, see
//! the overview. It tests whether the first steps are simple enough before
//! the real first-run flow comes in M3.
//!
//! What the user enters goes into the book the app has open (M3.1, D-033),
//! through the public functions of core and store, and is there at the next
//! start. The mock keeps to one currency and the accounts and categories in
//! it, until the real screens replace it (M3.3, M3.4, M3.5, M3.9).
//! Everything the window shows is text formatted here (D-065); what the user
//! types comes back as text and is read here too.

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Money, Report, Split, Transaction,
    TransactionError, TransactionId,
};
use kassensturz_store::{Action, Book, StoreError};

use crate::errors::{ErrorText, amount_example, report_text};
use crate::format::Locale;
use crate::parse::{AmountInputError, AmountReader};
use crate::register::{AmountWriter, Entry, RegisterView, Shown, entries, register_view};

/// How many transactions the overview lists.
const LATEST: usize = 5;

/// What the mock knows about the open book.
pub struct FirstRun {
    currency: CurrencyCode,
    /// The user's accounts, in the order they were added.
    accounts: Vec<AccountId>,
    /// The spending categories, in the order the UI offers them.
    spending: Vec<AccountId>,
    /// The income categories, in the order the UI offers them.
    income: Vec<AccountId>,
    /// The names of the spending categories, as the UI offers them.
    spending_names: Vec<String>,
    /// The names of the income categories, as the UI offers them.
    income_names: Vec<String>,
    /// The internal account that starting balances are booked against,
    /// once there is one (`docs/GLOSSARY.md`).
    starting_balances: Option<AccountId>,
}

/// Why something the user entered wasn't added. The UI shows a message for
/// each code (D-034).
#[derive(Debug)]
pub enum FirstRunError {
    /// What the user typed as an amount isn't one.
    Amount(AmountInputError),
    /// An amount of a transaction has to be more than zero: whether money
    /// came in or went out is chosen separately.
    NotPositive,
    /// What a credit card owes can't be below zero here.
    NegativeDebt,
    /// What the user typed as a date isn't one.
    Date,
    /// There is no account or category at that place in the list.
    NoSuchEntry,
    /// The book refused it, e.g. a name that is taken.
    Store(StoreError),
    /// The transaction doesn't add up; only an amount too large can get here.
    Transaction(TransactionError),
}

impl From<StoreError> for FirstRunError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// One account on the overview.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountCard {
    /// Its name.
    pub name: String,
    /// Its type, as a key the UI translates: `checking`, `savings`, `cash`
    /// or `credit-card`.
    pub account_type: &'static str,
    /// Its balance; for a debt, what the user owes.
    pub balance: Shown,
    /// Whether the balance is shown as "You owe" (D-067).
    pub owes: bool,
}

/// What a category got in total.
#[derive(Debug, PartialEq, Eq)]
pub struct CategoryTotal {
    /// The category's name.
    pub name: String,
    /// Its total.
    pub amount: Shown,
}

/// A recent transaction on the overview.
#[derive(Debug, PartialEq, Eq)]
pub struct LatestRow {
    /// The account it belongs to.
    pub account: String,
    /// The booking date.
    pub date: String,
    /// Who the money came from or went to; may be empty.
    pub other_party: String,
    /// Its category.
    pub category: String,
    /// The amount, as seen from the account.
    pub amount: Shown,
    /// Whether money came in.
    pub incoming: bool,
}

/// The overview after the first steps: the accounts side by side (D-021),
/// what came in and went out, and the latest transactions.
#[derive(Debug, PartialEq, Eq)]
pub struct Overview {
    /// The accounts, in the order they were added.
    pub accounts: Vec<AccountCard>,
    /// All accounts together: what the user has minus what they owe.
    pub together: Shown,
    /// Whether, all together, the user owes money.
    pub together_owes: bool,
    /// Everything booked to income categories.
    pub money_in: Shown,
    /// Everything booked to spending categories.
    pub money_out: Shown,
    /// The spending categories that have something, largest first.
    pub spending: Vec<CategoryTotal>,
    /// The newest transactions, newest first, without starting balances.
    pub latest: Vec<LatestRow>,
}

/// A transaction as the user entered it.
pub struct NewTransaction<'a> {
    /// The account's place in the overview's list.
    pub account: usize,
    /// Whether money came in, rather than went out.
    pub money_in: bool,
    /// The amount, as typed.
    pub amount: &'a str,
    /// Who the money came from or went to, as typed.
    pub other_party: &'a str,
    /// The category's place in the list of income or spending categories.
    pub category: usize,
    /// The date, as typed.
    pub date: &'a str,
}

impl FirstRun {
    /// What the mock needs from `book`: its accounts in one currency, the
    /// categories in that currency, and the internal account for starting
    /// balances. The currency is that of the book's first account, or
    /// `currency` in a book without accounts. A book without spending or
    /// income categories gets the ones named in `spending` and `income`; the
    /// UI passes their names in the user's language, as the real default
    /// categories will be stored (D-067).
    pub fn load(
        book: &mut Book,
        currency: CurrencyCode,
        spending: &[String],
        income: &[String],
    ) -> Result<Self, StoreError> {
        // Sorted by ID, so in the order they were added (D-067).
        let all = book.all_accounts()?;
        let currency = all
            .iter()
            .find(|account| account.kind.is_account() && !account.closed)
            .map_or(currency, |account| account.currency);
        let accounts = all
            .iter()
            .filter(|account| {
                account.kind.is_account() && !account.closed && account.currency == currency
            })
            .map(|account| account.id)
            .collect();
        let starting_balances = all
            .iter()
            .find(|account| account.kind == AccountKind::Equity && account.currency == currency)
            .map(|account| account.id);
        book.use_currency(currency)?;
        let (spending, spending_names) =
            categories(book, AccountKind::Expense, currency, spending)?;
        let (income, income_names) = categories(book, AccountKind::Income, currency, income)?;
        Ok(Self {
            currency,
            accounts,
            spending,
            income,
            spending_names,
            income_names,
            starting_balances,
        })
    }

    /// The names of the categories for money in or money out, in the order
    /// the UI offers them.
    pub fn category_names(&self, money_in: bool) -> &[String] {
        if money_in {
            &self.income_names
        } else {
            &self.spending_names
        }
    }

    /// The names of the user's accounts, in the order they were added.
    pub fn account_names(&self, book: &Book) -> Result<Vec<String>, StoreError> {
        self.accounts
            .iter()
            .map(|&id| Ok(book.account(id)?.name))
            .collect()
    }

    /// Adds an account of `account_type` named `name`, with the starting
    /// balance typed as `starting_balance`: for a credit card what the user
    /// owes on it, otherwise what is in it. Nothing typed means zero.
    pub fn add_account(
        &mut self,
        book: &mut Book,
        name: &str,
        account_type: AccountType,
        starting_balance: &str,
        locale: &impl Locale,
    ) -> Result<(), FirstRunError> {
        let amount = if starting_balance.trim().is_empty() {
            Money::zero(self.currency)
        } else {
            self.read_amount(book, starting_balance, locale)?
        };
        let amount = match account_type.kind() {
            AccountKind::Liability if amount.is_negative() => {
                return Err(FirstRunError::NegativeDebt);
            }
            // What is owed is money that went out of the account.
            AccountKind::Liability => amount
                .checked_neg()
                .map_err(|_| FirstRunError::Amount(AmountInputError::TooLarge))?,
            _ => amount,
        };

        let id = AccountId::generate();
        let currency = self.currency;
        let starting_balances = self.starting_balances;
        // One user action, so the account and its starting balance are added
        // together or not at all (D-032).
        let equity =
            book.as_one_action(Action::AddAccount, |book| -> Result<_, FirstRunError> {
                book.add_account(&Account {
                    id,
                    name: name.trim().to_owned(),
                    kind: account_type.kind(),
                    currency,
                    account_type: Some(account_type),
                    closed: false,
                    parent: None,
                })?;
                if amount.is_zero() {
                    return Ok(starting_balances);
                }
                let equity = match starting_balances {
                    Some(equity) => equity,
                    None => {
                        let equity = AccountId::generate();
                        book.add_account(&Account {
                            id: equity,
                            // Never shown (`AccountKind::Equity`).
                            name: "Starting balances".into(),
                            kind: AccountKind::Equity,
                            currency,
                            account_type: None,
                            closed: false,
                            parent: None,
                        })?;
                        equity
                    }
                };
                let balancing = amount
                    .checked_neg()
                    .map_err(|_| FirstRunError::Amount(AmountInputError::TooLarge))?;
                let transaction = Transaction::new(
                    TransactionId::generate(),
                    locale.today(),
                    None,
                    vec![Split::new(id, amount), Split::new(equity, balancing)],
                )
                .map_err(FirstRunError::Transaction)?;
                book.add_transaction(&transaction)?;
                Ok(Some(equity))
            })?;
        self.starting_balances = equity;
        self.accounts.push(id);
        Ok(())
    }

    /// Adds a transaction the user entered.
    pub fn add_transaction(
        &mut self,
        book: &mut Book,
        entered: &NewTransaction<'_>,
        locale: &impl Locale,
    ) -> Result<(), FirstRunError> {
        let account = *self
            .accounts
            .get(entered.account)
            .ok_or(FirstRunError::NoSuchEntry)?;
        let categories = if entered.money_in {
            &self.income
        } else {
            &self.spending
        };
        let category = *categories
            .get(entered.category)
            .ok_or(FirstRunError::NoSuchEntry)?;
        let amount = self.read_amount(book, entered.amount, locale)?;
        if amount.is_zero() || amount.is_negative() {
            return Err(FirstRunError::NotPositive);
        }
        let date = locale.read_date(entered.date).ok_or(FirstRunError::Date)?;
        let into_account = if entered.money_in {
            amount
        } else {
            amount
                .checked_neg()
                .map_err(|_| FirstRunError::Amount(AmountInputError::TooLarge))?
        };
        let into_category = into_account
            .checked_neg()
            .map_err(|_| FirstRunError::Amount(AmountInputError::TooLarge))?;
        let other_party = entered.other_party.trim();
        let transaction = Transaction::new(
            TransactionId::generate(),
            date,
            (!other_party.is_empty()).then(|| other_party.to_owned()),
            vec![
                Split::new(account, into_account),
                Split::new(category, into_category),
            ],
        )
        .map_err(FirstRunError::Transaction)?;
        book.add_transaction(&transaction)?;
        Ok(())
    }

    /// The overview of everything entered so far.
    pub fn overview(&self, book: &Book, locale: &impl Locale) -> Result<Overview, StoreError> {
        let writer = AmountWriter::new(book, self.currency, locale)?;
        let mut accounts = Vec::new();
        let mut balances = Vec::new();
        let mut latest: Vec<(String, Entry)> = Vec::new();
        for &id in &self.accounts {
            let account = book.account(id)?;
            let balance = book.balance(id)?;
            balances.push(balance);
            let (text, owes) = writer.balance(balance, account.kind);
            accounts.push(AccountCard {
                name: account.name.clone(),
                account_type: account.account_type.map_or("", type_key),
                balance: text,
                owes,
            });
            for entry in entries(book, id)? {
                if !entry.starting_balance {
                    latest.push((account.name.clone(), entry));
                }
            }
        }

        let together = self.sum(balances)?;
        let together_owes = together.is_negative();
        let together = match together.checked_neg() {
            Ok(debt) if together_owes => debt,
            _ => together,
        };

        let mut spending = Vec::new();
        for &id in &self.spending {
            let total = book.balance(id)?;
            if !total.is_zero() {
                spending.push((book.account(id)?.name, total));
            }
        }
        let money_out = self.sum(spending.iter().map(|(_, total)| *total))?;
        // Sorted here, from the exact amounts (D-065); a tie keeps the order
        // of the list.
        spending.sort_by_key(|(_, total)| core::cmp::Reverse(total.minor_units()));

        let mut money_in = Vec::new();
        for &id in &self.income {
            // Income categories give money, so their balance is negative.
            let total = book.balance(id)?;
            money_in.push(
                total
                    .checked_neg()
                    .map_err(|_| StoreError::AmountOverflow)?,
            );
        }
        let money_in = self.sum(money_in)?;

        // Newest first, and on the same day the one added last, as in a
        // register.
        latest.sort_by_key(|(_, entry)| core::cmp::Reverse((entry.date, entry.transaction)));
        let latest = latest
            .into_iter()
            .take(LATEST)
            .map(|(account, entry)| {
                let row = entry.row(&writer);
                LatestRow {
                    account,
                    date: row.date,
                    other_party: row.other_party,
                    category: row.category,
                    amount: row.amount,
                    incoming: row.incoming,
                }
            })
            .collect();

        Ok(Overview {
            accounts,
            together: writer.text(together),
            together_owes,
            money_in: writer.text(money_in),
            money_out: writer.text(money_out),
            spending: spending
                .into_iter()
                .map(|(name, total)| CategoryTotal {
                    name,
                    amount: writer.text(total),
                })
                .collect(),
            latest,
        })
    }

    /// The register of the account at `account` in the overview's list.
    pub fn register(
        &self,
        book: &Book,
        account: usize,
        locale: &impl Locale,
    ) -> Result<Option<RegisterView>, StoreError> {
        self.accounts
            .get(account)
            .map(|&id| register_view(book, id, locale))
            .transpose()
    }

    /// An amount written the way the user should type it, e.g. "1.234,56"
    /// in German: shown when what they typed couldn't be read.
    pub fn amount_example(&self, book: &Book, locale: &impl Locale) -> Result<String, StoreError> {
        amount_example(book, self.currency, locale)
    }

    /// `error` as the UI shows it, with every parameter as text.
    pub fn error_text(
        &self,
        book: &Book,
        error: &FirstRunError,
        locale: &impl Locale,
    ) -> ErrorText {
        let plain = ErrorText::plain;
        match error {
            FirstRunError::Amount(error) => ErrorText::amount(*error, book, self.currency, locale),
            FirstRunError::NotPositive => plain("input.amount-not-positive"),
            FirstRunError::NegativeDebt => plain("input.negative-debt"),
            FirstRunError::Date => ErrorText {
                code: "input.malformed-date",
                params: vec![("example", locale.short_date(locale.today()))],
            },
            FirstRunError::NoSuchEntry => plain("input.no-such-entry"),
            FirstRunError::Store(error) => report_text(book, &error.report(), locale),
            FirstRunError::Transaction(error) => report_text(book, &error.report(), locale),
        }
    }

    fn read_amount(
        &self,
        book: &Book,
        text: &str,
        locale: &impl Locale,
    ) -> Result<Money, FirstRunError> {
        let format = locale.amount_format(self.currency);
        let group_separator = locale.group_separator();
        let reader = AmountReader {
            format: &format,
            group_separator: &group_separator,
            currency: self.currency,
            minor_digits: book.currency_minor_digits(self.currency)?,
        };
        reader.read(text).map_err(FirstRunError::Amount)
    }

    fn sum(&self, amounts: impl IntoIterator<Item = Money>) -> Result<Money, StoreError> {
        Money::checked_sum(self.currency, amounts).map_err(|_| StoreError::AmountOverflow)
    }
}

/// The categories of `kind` in `currency`, with their names as the UI offers
/// them, in the order they were added. If the book has none, the ones named
/// in `names` are added, in that order and in one user action (D-032).
fn categories(
    book: &mut Book,
    kind: AccountKind,
    currency: CurrencyCode,
    names: &[String],
) -> Result<(Vec<AccountId>, Vec<String>), StoreError> {
    let mut found: Vec<(AccountId, String)> = book
        .categories()?
        .into_iter()
        .filter(|entry| {
            let category = &entry.category;
            category.kind == kind && category.currency == currency && !category.closed
        })
        .map(|entry| (entry.category.id, entry.path.join(" › ")))
        .collect();
    // IDs are UUIDv7, so sorted by ID is in the order they were added
    // (D-067), as the UI offered them when they were made.
    found.sort_by_key(|(id, _)| *id);
    if !found.is_empty() || names.is_empty() {
        return Ok(found.into_iter().unzip());
    }
    let ids = book.as_one_action(Action::AddCategory, |book| {
        names
            .iter()
            .map(|name| {
                let id = AccountId::generate();
                book.add_account(&Account {
                    id,
                    name: name.clone(),
                    kind,
                    currency,
                    account_type: None,
                    closed: false,
                    parent: None,
                })?;
                Ok(id)
            })
            .collect::<Result<Vec<_>, StoreError>>()
    })?;
    Ok((ids, names.to_vec()))
}

/// The key the UI translates an account type with.
pub fn type_key(account_type: AccountType) -> &'static str {
    match account_type {
        AccountType::Checking => "checking",
        AccountType::Savings => "savings",
        AccountType::Cash => "cash",
        AccountType::CreditCard => "credit-card",
    }
}

/// The account type for a key from [`type_key`].
pub fn type_from_key(key: &str) -> Option<AccountType> {
    AccountType::ALL
        .into_iter()
        .find(|&account_type| type_key(account_type) == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::number_format::{DecimalPlaces, NumberFormat, UserLocale};
    use crate::register::tests::TestLocale;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn spending() -> [String; 3] {
        [
            "Groceries".to_owned(),
            "Rent".to_owned(),
            "Clothes".to_owned(),
        ]
    }

    fn income() -> [String; 1] {
        ["Salary".to_owned()]
    }

    /// The mock with its book, so the tests read as the UI uses it.
    struct Mock {
        book: Book,
        first_run: FirstRun,
    }

    impl Mock {
        fn add_account(
            &mut self,
            name: &str,
            account_type: AccountType,
            starting_balance: &str,
            locale: &impl Locale,
        ) -> Result<(), FirstRunError> {
            self.first_run
                .add_account(&mut self.book, name, account_type, starting_balance, locale)
        }

        fn add_transaction(
            &mut self,
            entered: &NewTransaction<'_>,
            locale: &impl Locale,
        ) -> Result<(), FirstRunError> {
            self.first_run
                .add_transaction(&mut self.book, entered, locale)
        }

        fn overview(&self, locale: &impl Locale) -> Result<Overview, StoreError> {
            self.first_run.overview(&self.book, locale)
        }

        fn register(
            &self,
            account: usize,
            locale: &impl Locale,
        ) -> Result<Option<RegisterView>, StoreError> {
            self.first_run.register(&self.book, account, locale)
        }

        fn error_text(&self, error: &FirstRunError, locale: &impl Locale) -> ErrorText {
            self.first_run.error_text(&self.book, error, locale)
        }
    }

    fn first_run() -> Mock {
        let mut book = Book::create_in_memory().expect("book in memory");
        let first_run =
            FirstRun::load(&mut book, EUR, &spending(), &income()).expect("mock loaded");
        Mock { book, first_run }
    }

    fn paid<'a>(
        account: usize,
        amount: &'a str,
        to: &'a str,
        category: usize,
    ) -> NewTransaction<'a> {
        NewTransaction {
            account,
            money_in: false,
            amount,
            other_party: to,
            category,
            date: "2026-10-05",
        }
    }

    fn code(result: Result<(), FirstRunError>, first_run: &Mock) -> &'static str {
        let error = result.expect_err("refused");
        first_run.error_text(&error, &TestLocale).code
    }

    #[test]
    fn walks_through_the_first_start() {
        let mut first_run = first_run();
        let locale = TestLocale;
        first_run
            .add_account(
                "Checking account",
                AccountType::Checking,
                "2.450,00",
                &locale,
            )
            .expect("account added");
        first_run
            .add_transaction(&paid(0, "53,20", "Corner Grocery", 0), &locale)
            .expect("transaction added");
        first_run
            .add_transaction(
                &NewTransaction {
                    money_in: true,
                    ..paid(0, "2400", "Example Employer Ltd.", 0)
                },
                &locale,
            )
            .expect("transaction added");
        first_run
            .add_account("Credit card", AccountType::CreditCard, "300", &locale)
            .expect("account added");
        first_run
            .add_transaction(&paid(1, "89,90", "Example Shoes", 2), &locale)
            .expect("transaction added");
        // An account without a starting balance.
        first_run
            .add_account("Cash", AccountType::Cash, "", &locale)
            .expect("account added");

        let overview = first_run.overview(&locale).expect("overview");
        let card = |name: &str, account_type, balance: &str, owes| AccountCard {
            name: name.into(),
            account_type,
            balance: Shown {
                text: balance.into(),
                exact: balance.into(),
            },
            owes,
        };
        assert_eq!(
            overview.accounts,
            [
                card("Checking account", "checking", "4796,80 €", false),
                card("Credit card", "credit-card", "389,90 €", true),
                card("Cash", "cash", "0,00 €", false),
            ]
        );
        assert_eq!(overview.together.text, "4406,90 €");
        assert!(!overview.together_owes);
        assert_eq!(overview.money_in.text, "2400,00 €");
        // Starting balances are neither money in nor money out.
        assert_eq!(overview.money_out.text, "143,10 €");
        let spending: Vec<_> = overview
            .spending
            .iter()
            .map(|total| (total.name.as_str(), total.amount.text.as_str()))
            .collect();
        assert_eq!(spending, [("Clothes", "89,90 €"), ("Groceries", "53,20 €")]);
        let latest: Vec<_> = overview
            .latest
            .iter()
            .map(|row| {
                (
                    row.other_party.as_str(),
                    row.account.as_str(),
                    row.category.as_str(),
                )
            })
            .collect();
        assert_eq!(
            latest,
            [
                ("Example Shoes", "Credit card", "Clothes"),
                ("Example Employer Ltd.", "Checking account", "Salary"),
                ("Corner Grocery", "Checking account", "Groceries"),
            ]
        );

        let register = first_run
            .register(1, &locale)
            .expect("register")
            .expect("an account at 1");
        assert_eq!(register.account_name, "Credit card");
        assert_eq!(register.balance.text, "389,90 €");
        assert!(register.owes);
        let rows: Vec<_> = register
            .rows
            .iter()
            .map(|row| {
                (
                    row.date.as_str(),
                    row.other_party.as_str(),
                    row.category.as_str(),
                    row.amount.text.as_str(),
                    row.starting_balance,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                // The starting balance is booked on the day the account is
                // added, here after the purchase the user dated earlier.
                ("2026-10-19", "", "", "-300,00 €", true),
                ("2026-10-05", "Example Shoes", "Clothes", "-89,90 €", false),
            ]
        );
        assert_eq!(first_run.register(3, &locale).expect("no error"), None);
    }

    #[test]
    fn totals_add_up_the_exact_amounts_before_rounding() {
        let mut first_run = first_run();
        // Amounts are typed as usual, with every decimal place.
        let locale = UserLocale::new(
            TestLocale,
            NumberFormat {
                decimal_places: DecimalPlaces::None,
                ..NumberFormat::default()
            },
        );
        first_run
            .add_account("Cash", AccountType::Cash, "10,00", &locale)
            .expect("account added");
        for _ in 0..3 {
            first_run
                .add_transaction(&paid(0, "0,40", "Kiosk", 0), &locale)
                .expect("transaction added");
        }

        // Each 0,40 € shows as 0 €; their total of 1,20 € as 1 €, not 0 €.
        let overview = first_run.overview(&locale).expect("overview");
        let groceries = &overview.spending[0].amount;
        assert_eq!(groceries.text, "1 €");
        assert_eq!(groceries.exact, "1,20 €");
        assert_eq!(overview.money_out.text, "1 €");
        assert_eq!(
            overview.latest[0].amount,
            Shown {
                text: "0 €".into(),
                exact: "-0,40 €".into()
            }
        );
        let balance = &overview.accounts[0].balance;
        assert_eq!(
            (balance.text.as_str(), balance.exact.as_str()),
            ("9 €", "8,80 €")
        );
    }

    #[test]
    fn a_debt_larger_than_everything_reads_you_owe() {
        let mut first_run = first_run();
        let locale = TestLocale;
        first_run
            .add_account("Credit card", AccountType::CreditCard, "300", &locale)
            .expect("account added");
        let overview = first_run.overview(&locale).expect("overview");
        assert_eq!(overview.together.text, "300,00 €");
        assert!(overview.together_owes);
    }

    #[test]
    fn explains_what_it_refuses() {
        let mut first_run = first_run();
        let locale = TestLocale;
        let result = first_run.add_account("Checking", AccountType::Checking, "12.50", &locale);
        let error = result.expect_err("refused");
        assert_eq!(
            first_run.error_text(&error, &locale),
            ErrorText {
                code: "input.malformed-amount",
                params: vec![("example", "1234,56".into())],
            }
        );
        let result = first_run.add_account("Card", AccountType::CreditCard, "-5", &locale);
        assert_eq!(code(result, &first_run), "input.negative-debt");
        let result = first_run.add_account(" ", AccountType::Cash, "", &locale);
        assert_eq!(code(result, &first_run), "store.empty-account-name");

        first_run
            .add_account("Checking", AccountType::Checking, "", &locale)
            .expect("account added");
        let result = first_run.add_account("checking", AccountType::Savings, "", &locale);
        let error = result.expect_err("refused");
        assert_eq!(
            first_run.error_text(&error, &locale),
            ErrorText {
                code: "store.duplicate-account-name",
                params: vec![("existing", "Checking".into())],
            }
        );

        let result = first_run.add_transaction(&paid(0, "0", "", 0), &locale);
        assert_eq!(code(result, &first_run), "input.amount-not-positive");
        let result = first_run.add_transaction(&paid(0, "-5", "", 0), &locale);
        assert_eq!(code(result, &first_run), "input.amount-not-positive");
        let result = first_run.add_transaction(&paid(0, "", "", 0), &locale);
        assert_eq!(code(result, &first_run), "input.empty-amount");
        let result = first_run.add_transaction(&paid(0, "1,234", "", 0), &locale);
        assert_eq!(code(result, &first_run), "input.too-many-decimal-places");
        let result = first_run.add_transaction(&paid(1, "5", "", 0), &locale);
        assert_eq!(code(result, &first_run), "input.no-such-entry");
        let result = first_run.add_transaction(&paid(0, "5", "", 7), &locale);
        assert_eq!(code(result, &first_run), "input.no-such-entry");
        let result = first_run.add_transaction(
            &NewTransaction {
                date: "yesterday",
                ..paid(0, "5", "", 0)
            },
            &locale,
        );
        let error = result.expect_err("refused");
        assert_eq!(
            first_run.error_text(&error, &locale),
            ErrorText {
                code: "input.malformed-date",
                params: vec![("example", "2026-10-19".into())],
            }
        );

        // Nothing refused was added.
        let overview = first_run.overview(&locale).expect("overview");
        assert_eq!(overview.accounts.len(), 1);
        assert!(overview.latest.is_empty());
    }

    #[test]
    fn finds_what_was_entered_when_the_book_is_opened_again() {
        let folder = crate::test_folder::TestFolder::new();
        let path = folder.join("Household.ksz");
        let computer = kassensturz_store::Computer {
            name: "Desk".into(),
            backup_folder: folder.join("backups"),
        };
        let locale = TestLocale;
        let mut book = Book::create(&path, &computer).expect("book made");
        let mut first_run =
            FirstRun::load(&mut book, EUR, &spending(), &income()).expect("mock loaded");
        first_run
            .add_account(&mut book, "Checking", AccountType::Checking, "100", &locale)
            .expect("account added");
        first_run
            .add_transaction(&mut book, &paid(0, "53,20", "Corner Grocery", 2), &locale)
            .expect("transaction added");
        first_run
            .add_account(&mut book, "Wallet", AccountType::Cash, "20", &locale)
            .expect("account added");
        let before = first_run.overview(&book, &locale).expect("overview");
        drop(book);

        // Opened again, in another language: the categories the book has
        // are kept, not added a second time.
        let mut book = Book::open(&path, &computer).expect("book opened");
        let first_run =
            FirstRun::load(&mut book, EUR, &["Lebensmittel".to_owned()], &[]).expect("mock loaded");
        assert_eq!(
            first_run.account_names(&book).expect("names"),
            ["Checking", "Wallet"]
        );
        assert_eq!(
            first_run.category_names(false),
            ["Groceries", "Rent", "Clothes"]
        );
        assert_eq!(first_run.category_names(true), ["Salary"]);
        assert_eq!(
            first_run.overview(&book, &locale).expect("overview"),
            before
        );
        let spending = book
            .categories()
            .expect("categories")
            .into_iter()
            .filter(|entry| entry.category.kind == AccountKind::Expense)
            .count();
        assert_eq!(spending, 3);
    }

    #[test]
    fn keeps_to_the_currency_of_the_first_account() {
        let mut book = Book::create_in_memory().expect("book in memory");
        let chf = CurrencyCode::new("CHF").expect("valid code");
        let mut first_run =
            FirstRun::load(&mut book, chf, &spending(), &income()).expect("mock loaded");
        first_run
            .add_account(&mut book, "Konto", AccountType::Checking, "", &TestLocale)
            .expect("account added");
        let again = FirstRun::load(&mut book, EUR, &spending(), &income()).expect("mock loaded");
        assert_eq!(again.account_names(&book).expect("names"), ["Konto"]);
        assert_eq!(again.currency, chf);
    }

    #[test]
    fn account_types_round_trip_through_their_keys() {
        for account_type in AccountType::ALL {
            assert_eq!(type_from_key(type_key(account_type)), Some(account_type));
        }
        assert_eq!(type_from_key("loan"), None);
    }
}
