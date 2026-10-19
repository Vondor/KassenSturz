// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The first-run mock (M1.M, D-068): add an account, add a transaction, see
//! the overview. It tests whether the first steps are simple enough before
//! the real first-run flow comes in M3.
//!
//! The data is whatever the user makes up while trying it. It lives in a book
//! in memory and is never saved (AGENTS.md §3.1 rule 2). Everything the
//! window shows is text formatted here (D-065); what the user types comes
//! back as text and is read here too.

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, ErrorParam, ErrorReport, Money,
    Report, Split, Transaction, TransactionError, TransactionId,
};
use kassensturz_store::{Action, Book, StoreError};

use crate::format::{Affixes, AmountFormat, Locale};
use crate::parse::{AmountInputError, AmountReader};
use crate::register::{AmountWriter, Entry, RegisterView, entries, register_view};

/// How many transactions the overview lists.
const LATEST: usize = 5;

/// The mock's book and what it knows about it.
pub struct FirstRun {
    book: Book,
    currency: CurrencyCode,
    /// The user's accounts, in the order they were added.
    accounts: Vec<AccountId>,
    /// The spending categories, in the order the UI offers them.
    spending: Vec<AccountId>,
    /// The income categories, in the order the UI offers them.
    income: Vec<AccountId>,
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

/// An error as the UI gets it: a code and its parameters, already as text.
#[derive(Debug, PartialEq, Eq)]
pub struct ErrorText {
    /// The code, e.g. `input.malformed-amount` or `store.duplicate-account-name`.
    pub code: &'static str,
    /// The parameters by name, e.g. the example of a correctly typed amount.
    pub params: Vec<(&'static str, String)>,
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
    pub balance: String,
    /// Whether the balance is shown as "You owe" (D-067).
    pub owes: bool,
}

/// What a category got in total.
#[derive(Debug, PartialEq, Eq)]
pub struct CategoryTotal {
    /// The category's name.
    pub name: String,
    /// Its total.
    pub amount: String,
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
    pub amount: String,
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
    pub together: String,
    /// Whether, all together, the user owes money.
    pub together_owes: bool,
    /// Everything booked to income categories.
    pub money_in: String,
    /// Everything booked to spending categories.
    pub money_out: String,
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
    /// An empty book in `currency` with the categories named in `spending`
    /// and `income`. The UI passes their names in the user's language, as the
    /// real default categories will be stored (D-067).
    pub fn new(
        currency: CurrencyCode,
        spending: &[String],
        income: &[String],
    ) -> Result<Self, StoreError> {
        let mut book = Book::create_in_memory()?;
        book.use_currency(currency)?;
        let mut add_categories = |names: &[String], kind| {
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
        };
        let spending = add_categories(spending, AccountKind::Expense)?;
        let income = add_categories(income, AccountKind::Income)?;
        Ok(Self {
            book,
            currency,
            accounts: Vec::new(),
            spending,
            income,
            starting_balances: None,
        })
    }

    /// The names of the user's accounts, in the order they were added.
    pub fn account_names(&self) -> Result<Vec<String>, StoreError> {
        self.accounts
            .iter()
            .map(|&id| Ok(self.book.account(id)?.name))
            .collect()
    }

    /// Adds an account of `account_type` named `name`, with the starting
    /// balance typed as `starting_balance`: for a credit card what the user
    /// owes on it, otherwise what is in it. Nothing typed means zero.
    pub fn add_account(
        &mut self,
        name: &str,
        account_type: AccountType,
        starting_balance: &str,
        locale: &impl Locale,
    ) -> Result<(), FirstRunError> {
        let amount = if starting_balance.trim().is_empty() {
            Money::zero(self.currency)
        } else {
            self.read_amount(starting_balance, locale)?
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
            self.book
                .as_one_action(Action::AddAccount, |book| -> Result<_, FirstRunError> {
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
        let amount = self.read_amount(entered.amount, locale)?;
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
        self.book.add_transaction(&transaction)?;
        Ok(())
    }

    /// The overview of everything entered so far.
    pub fn overview(&self, locale: &impl Locale) -> Result<Overview, StoreError> {
        let writer = AmountWriter::new(&self.book, self.currency, locale)?;
        let mut accounts = Vec::new();
        let mut balances = Vec::new();
        let mut latest: Vec<(String, Entry)> = Vec::new();
        for &id in &self.accounts {
            let account = self.book.account(id)?;
            let balance = self.book.balance(id)?;
            balances.push(balance);
            let (text, owes) = writer.balance(balance, account.kind);
            accounts.push(AccountCard {
                name: account.name.clone(),
                account_type: account.account_type.map_or("", type_key),
                balance: text,
                owes,
            });
            for entry in entries(&self.book, id)? {
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
            let total = self.book.balance(id)?;
            if !total.is_zero() {
                spending.push((self.book.account(id)?.name, total));
            }
        }
        let money_out = self.sum(spending.iter().map(|(_, total)| *total))?;
        // Sorted here, from the exact amounts (D-065); a tie keeps the order
        // of the list.
        spending.sort_by_key(|(_, total)| core::cmp::Reverse(total.minor_units()));

        let mut money_in = Vec::new();
        for &id in &self.income {
            // Income categories give money, so their balance is negative.
            let total = self.book.balance(id)?;
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
        account: usize,
        locale: &impl Locale,
    ) -> Result<Option<RegisterView>, StoreError> {
        self.accounts
            .get(account)
            .map(|&id| register_view(&self.book, id, locale))
            .transpose()
    }

    /// An amount written the way the user should type it, e.g. "1.234,56"
    /// in German: shown when what they typed couldn't be read.
    pub fn amount_example(&self, locale: &impl Locale) -> Result<String, StoreError> {
        let format = AmountFormat {
            positive: Affixes::default(),
            negative: Affixes::default(),
            ..locale.amount_format(self.currency)
        };
        let digits = self.book.currency_minor_digits(self.currency)?;
        // 1234 and as many decimal places as the currency has, all 5s and 6s
        // so no digit can be mistaken for a separator.
        let mut minor: i64 = 1234;
        for place in 0..digits {
            minor = minor
                .saturating_mul(10)
                .saturating_add(if place == 0 { 5 } else { 6 });
        }
        Ok(format.format(
            Money::from_minor_units(minor, self.currency),
            digits,
            |value| locale.group_digits(value),
        ))
    }

    /// `error` as the UI shows it, with every parameter as text.
    pub fn error_text(&self, error: &FirstRunError, locale: &impl Locale) -> ErrorText {
        let plain = |code| ErrorText {
            code,
            params: Vec::new(),
        };
        match error {
            FirstRunError::Amount(AmountInputError::Empty) => plain("input.empty-amount"),
            FirstRunError::Amount(AmountInputError::Malformed) => ErrorText {
                code: "input.malformed-amount",
                params: vec![("example", self.amount_example(locale).unwrap_or_default())],
            },
            FirstRunError::Amount(AmountInputError::TooManyDecimalPlaces { max }) => ErrorText {
                code: "input.too-many-decimal-places",
                params: vec![("max", max.to_string())],
            },
            FirstRunError::Amount(AmountInputError::TooLarge) => plain("input.amount-too-large"),
            FirstRunError::NotPositive => plain("input.amount-not-positive"),
            FirstRunError::NegativeDebt => plain("input.negative-debt"),
            FirstRunError::Date => ErrorText {
                code: "input.malformed-date",
                params: vec![("example", locale.short_date(locale.today()))],
            },
            FirstRunError::NoSuchEntry => plain("input.no-such-entry"),
            FirstRunError::Store(error) => self.report_text(&error.report(), locale),
            FirstRunError::Transaction(error) => self.report_text(&error.report(), locale),
        }
    }

    fn report_text(&self, report: &ErrorReport, locale: &impl Locale) -> ErrorText {
        let params = report
            .params
            .iter()
            .map(|(name, param)| {
                let text = match param {
                    ErrorParam::Account(id) => self
                        .book
                        .account(*id)
                        .map(|account| account.name)
                        .unwrap_or_default(),
                    ErrorParam::Money(amount) => {
                        AmountWriter::new(&self.book, amount.currency(), locale)
                            .map(|writer| writer.text(*amount))
                            .unwrap_or_default()
                    }
                    ErrorParam::Text(text) => text.clone(),
                    ErrorParam::Number(number) => number.to_string(),
                    ErrorParam::Currency(currency) => currency.to_string(),
                    ErrorParam::Date(date) => locale.short_date(*date),
                    ErrorParam::Keyword(keyword) => (*keyword).to_owned(),
                    other => format!("{other:?}"),
                };
                (*name, text)
            })
            .collect();
        ErrorText {
            code: report.code,
            params,
        }
    }

    fn read_amount(&self, text: &str, locale: &impl Locale) -> Result<Money, FirstRunError> {
        let format = locale.amount_format(self.currency);
        let group_separator = locale.group_separator();
        let reader = AmountReader {
            format: &format,
            group_separator: &group_separator,
            currency: self.currency,
            minor_digits: self.book.currency_minor_digits(self.currency)?,
        };
        reader.read(text).map_err(FirstRunError::Amount)
    }

    fn sum(&self, amounts: impl IntoIterator<Item = Money>) -> Result<Money, StoreError> {
        Money::checked_sum(self.currency, amounts).map_err(|_| StoreError::AmountOverflow)
    }
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
    use crate::register::tests::TestLocale;

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn first_run() -> FirstRun {
        let spending = [
            "Groceries".to_owned(),
            "Rent".to_owned(),
            "Clothes".to_owned(),
        ];
        let income = ["Salary".to_owned()];
        FirstRun::new(EUR, &spending, &income).expect("mock book")
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

    fn code(result: Result<(), FirstRunError>, first_run: &FirstRun) -> &'static str {
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
            balance: balance.into(),
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
        assert_eq!(overview.together, "4406,90 €");
        assert!(!overview.together_owes);
        assert_eq!(overview.money_in, "2400,00 €");
        // Starting balances are neither money in nor money out.
        assert_eq!(overview.money_out, "143,10 €");
        let spending: Vec<_> = overview
            .spending
            .iter()
            .map(|total| (total.name.as_str(), total.amount.as_str()))
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
        assert_eq!(register.balance, "389,90 €");
        assert!(register.owes);
        let rows: Vec<_> = register
            .rows
            .iter()
            .map(|row| {
                (
                    row.date.as_str(),
                    row.other_party.as_str(),
                    row.category.as_str(),
                    row.amount.as_str(),
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
    fn a_debt_larger_than_everything_reads_you_owe() {
        let mut first_run = first_run();
        let locale = TestLocale;
        first_run
            .add_account("Credit card", AccountType::CreditCard, "300", &locale)
            .expect("account added");
        let overview = first_run.overview(&locale).expect("overview");
        assert_eq!(overview.together, "300,00 €");
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
    fn account_types_round_trip_through_their_keys() {
        for account_type in AccountType::ALL {
            assert_eq!(type_from_key(type_key(account_type)), Some(account_type));
        }
        assert_eq!(type_from_key("loan"), None);
    }
}
