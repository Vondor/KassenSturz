// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fetched data into the importer's [`Statement`], so a bank connection
//! feeds the same import pipeline as a file (D-030, threat T-11): the
//! duplicate check by the bank's ID, the user's decisions, Rules, Transfers
//! and the balance check all happen there, once.

use kassensturz_core::{CurrencyCode, Date};
use kassensturz_import::{
    Balance, Entry, EntryStatus, Limits, Statement, StatementAccount, check_statements,
};

use crate::error::BankError;
use crate::fetched::{Fetched, FetchedTransaction};

/// Turns what a fetch delivered into one statement, exactly: every amount
/// through [`crate::FetchedAmount::money`] in the account's currency with
/// that currency's decimal places (D-141), and the whole statement through
/// the checks every imported statement gets, within `limits` (D-091,
/// threat T-09).
///
/// A transaction without a booking date gets its value date; one with
/// neither fails. The bank's booked balance becomes the closing balance,
/// dated by the bank or, if it didn't say, by the latest booking date; a
/// balance without any date is left out. Nothing is rounded, guessed or
/// dropped: when anything can't be read exactly, the whole statement fails
/// and nothing is imported (AGENTS.md §2).
pub fn statement(fetched: &Fetched, limits: &Limits) -> Result<Statement, BankError> {
    let code = account_currency(fetched)?;
    let currency = kassensturz_import::currency(code)?;
    let mut entries = Vec::with_capacity(fetched.transactions.len());
    for transaction in &fetched.transactions {
        limits.check_entries(entries.len() + 1)?;
        entries.push(entry(transaction, currency)?);
    }
    let closing_balance = match &fetched.balance {
        Some(balance) => {
            let date = balance
                .date
                .or_else(|| entries.iter().map(|entry| entry.booking_date).max());
            date.map(|date| -> Result<Balance, BankError> {
                Ok(Balance {
                    amount: balance.amount.money(currency)?,
                    date,
                })
            })
            .transpose()?
        }
        None => None,
    };
    let statement = Statement {
        account: StatementAccount {
            iban: fetched.account.iban.clone(),
            number: fetched.account.number.clone(),
        },
        currency,
        opening_balance: None,
        closing_balance,
        entries,
    };
    check_statements(std::slice::from_ref(&statement), limits)?;
    Ok(statement)
}

/// The account's currency: the bank's word for it, or, when the bank
/// didn't say, the currency every amount names (D-141).
fn account_currency(fetched: &Fetched) -> Result<CurrencyCode, BankError> {
    if let Some(currency) = fetched.account.currency {
        return Ok(currency);
    }
    let mut named = fetched
        .transactions
        .iter()
        .map(|transaction| &transaction.amount)
        .chain(fetched.balance.iter().map(|balance| &balance.amount))
        .filter_map(|amount| amount.currency.as_deref());
    let Some(first) = named.next() else {
        return Err(BankError::MissingCurrency);
    };
    let code = CurrencyCode::new(first).map_err(|_| BankError::MalformedAmount {
        amount: first.to_owned(),
    })?;
    if named.any(|other| other != first) {
        return Err(BankError::MissingCurrency);
    }
    Ok(code)
}

fn entry(
    transaction: &FetchedTransaction,
    currency: kassensturz_core::Currency,
) -> Result<Entry, BankError> {
    let booking_date: Date = transaction
        .booking_date
        .or(transaction.value_date)
        .ok_or(BankError::MissingDate)?;
    let lines: Vec<&str> = transaction
        .purpose
        .iter()
        .map(|line| line.trim_end())
        .filter(|line| !line.is_empty())
        .collect();
    Ok(Entry {
        status: if transaction.booked {
            EntryStatus::Booked
        } else {
            EntryStatus::Pending
        },
        bank_id: non_empty(transaction.bank_id.as_deref()),
        booking_date,
        value_date: transaction.value_date,
        amount: transaction.amount.money(currency)?,
        other_party_name: non_empty(transaction.other_party_name.as_deref()),
        other_party_iban: non_empty(transaction.other_party_iban.as_deref()),
        remittance_information: (!lines.is_empty()).then(|| lines.join("\n")),
    })
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{Currency, Money};
    use kassensturz_import::ImportError;

    use super::*;
    use crate::backend::{BankAccount, BankAccountId, BankAccountKind, LoginId};
    use crate::fetched::{FetchedAmount, FetchedBalance};

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn eur() -> Currency {
        Currency::new(EUR, 2).expect("two decimal places")
    }

    fn day(day: i8) -> Date {
        Date::new(2026, 10, day).expect("a valid date")
    }

    fn account(currency: Option<CurrencyCode>) -> BankAccount {
        BankAccount {
            id: BankAccountId::new("acct-1"),
            login: LoginId::new("login-1"),
            kind: BankAccountKind::Current,
            iban: Some("DE89370400440532013000".to_owned()),
            number: Some("0532013000".to_owned()),
            currency,
            name: None,
        }
    }

    fn transaction(minor_units: i64) -> FetchedTransaction {
        FetchedTransaction {
            booked: true,
            bank_id: Some("ID-1".to_owned()),
            booking_date: Some(day(5)),
            value_date: Some(day(6)),
            amount: FetchedAmount::from_minor_units(minor_units, eur()),
            other_party_name: Some(" Grocer ".to_owned()),
            other_party_iban: Some("NL91ABNA0417164300".to_owned()),
            purpose: vec!["Line one ".to_owned(), String::new(), "Line two".to_owned()],
        }
    }

    fn fetched(transactions: Vec<FetchedTransaction>) -> Fetched {
        Fetched {
            account: account(Some(EUR)),
            balance: Some(FetchedBalance {
                amount: FetchedAmount::from_minor_units(100_000, eur()),
                date: Some(day(9)),
            }),
            transactions,
        }
    }

    #[test]
    fn every_field_arrives_as_the_bank_sent_it() {
        let statement = super::statement(&fetched(vec![transaction(-1234)]), &Limits::default())
            .expect("a statement");
        assert_eq!(
            statement.account,
            StatementAccount {
                iban: Some("DE89370400440532013000".to_owned()),
                number: Some("0532013000".to_owned()),
            }
        );
        assert_eq!(statement.currency, eur());
        assert_eq!(statement.opening_balance, None);
        assert_eq!(
            statement.closing_balance,
            Some(Balance {
                amount: Money::from_minor_units(100_000, EUR),
                date: day(9),
            })
        );
        assert_eq!(
            statement.entries,
            vec![Entry {
                status: EntryStatus::Booked,
                bank_id: Some("ID-1".to_owned()),
                booking_date: day(5),
                value_date: Some(day(6)),
                amount: Money::from_minor_units(-1234, EUR),
                other_party_name: Some("Grocer".to_owned()),
                other_party_iban: Some("NL91ABNA0417164300".to_owned()),
                remittance_information: Some("Line one\nLine two".to_owned()),
            }]
        );
    }

    #[test]
    fn a_pending_transaction_stays_pending_and_empty_texts_are_none() {
        let mut pending = transaction(-500);
        pending.booked = false;
        pending.bank_id = Some("  ".to_owned());
        pending.other_party_name = None;
        pending.other_party_iban = Some(String::new());
        pending.purpose = vec![String::new()];
        let statement =
            super::statement(&fetched(vec![pending]), &Limits::default()).expect("a statement");
        let entry = &statement.entries[0];
        assert_eq!(entry.status, EntryStatus::Pending);
        assert_eq!(entry.bank_id, None);
        assert_eq!(entry.other_party_name, None);
        assert_eq!(entry.other_party_iban, None);
        assert_eq!(entry.remittance_information, None);
    }

    #[test]
    fn the_value_date_stands_in_for_a_missing_booking_date() {
        let mut only_value_date = transaction(100);
        only_value_date.booking_date = None;
        let statement = super::statement(&fetched(vec![only_value_date]), &Limits::default())
            .expect("a statement");
        assert_eq!(statement.entries[0].booking_date, day(6));
        assert_eq!(statement.entries[0].value_date, Some(day(6)));
        let mut no_date = transaction(100);
        no_date.booking_date = None;
        no_date.value_date = None;
        assert_eq!(
            super::statement(&fetched(vec![no_date]), &Limits::default()),
            Err(BankError::MissingDate)
        );
    }

    #[test]
    fn a_balance_without_a_date_takes_the_latest_booking_date_or_is_left_out() {
        let mut fetched = fetched(vec![transaction(100), transaction(200)]);
        fetched.transactions[1].booking_date = Some(day(8));
        fetched.balance.as_mut().expect("a balance").date = None;
        let statement = super::statement(&fetched, &Limits::default()).expect("a statement");
        assert_eq!(
            statement.closing_balance.map(|balance| balance.date),
            Some(day(8))
        );
        fetched.transactions.clear();
        let statement = super::statement(&fetched, &Limits::default()).expect("a statement");
        assert_eq!(statement.closing_balance, None);
    }

    #[test]
    fn the_currency_comes_from_the_account_or_from_every_amount() {
        let mut fetched = fetched(vec![transaction(100)]);
        fetched.account.currency = None;
        let statement = super::statement(&fetched, &Limits::default()).expect("a statement");
        assert_eq!(statement.currency, eur());
        fetched.transactions[0].amount.currency = None;
        fetched.balance = None;
        assert_eq!(
            super::statement(&fetched, &Limits::default()),
            Err(BankError::MissingCurrency)
        );
        fetched.transactions[0].amount.currency = Some("EUR".to_owned());
        fetched.transactions.push(transaction(5));
        fetched.transactions[1].amount.currency = Some("USD".to_owned());
        assert_eq!(
            super::statement(&fetched, &Limits::default()),
            Err(BankError::MissingCurrency)
        );
    }

    #[test]
    fn an_inexact_amount_fails_the_whole_statement() {
        let mut fetched = fetched(vec![transaction(100), transaction(200)]);
        fetched.transactions[1].amount.fraction = "1/3".to_owned();
        assert_eq!(
            super::statement(&fetched, &Limits::default()),
            Err(BankError::InexactAmount {
                amount: "1/3".to_owned()
            })
        );
    }

    #[test]
    fn a_currency_no_table_lists_is_refused() {
        let mut fetched = fetched(vec![transaction(100)]);
        let unknown = CurrencyCode::new("XQZ").expect("three letters");
        fetched.account.currency = Some(unknown);
        fetched.transactions[0].amount.currency = None;
        fetched.balance = None;
        assert_eq!(
            super::statement(&fetched, &Limits::default()),
            Err(BankError::Statement(ImportError::UnknownCurrency {
                currency: unknown,
                line: None
            }))
        );
    }

    #[test]
    fn the_limits_apply_to_fetched_data_too() {
        let limits = Limits {
            max_entries: 1,
            ..Limits::default()
        };
        assert_eq!(
            super::statement(&fetched(vec![transaction(1), transaction(2)]), &limits),
            Err(BankError::Statement(ImportError::TooManyEntries { max: 1 }))
        );
        let limits = Limits {
            max_text_bytes: 4,
            ..Limits::default()
        };
        assert_eq!(
            super::statement(&fetched(vec![transaction(1)]), &limits),
            Err(BankError::Statement(ImportError::TextTooLong {
                max_bytes: 4,
                line: None
            }))
        );
    }
}
