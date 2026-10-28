// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A transaction's fields changed by the AI (D-257): its date, payee, note,
//! account and amount.
//!
//! Which fields may change is the user's to say: a field locked in the AI
//! settings (D-252) is refused, and not offered to a model at all. The
//! date, the payee and the note change on any transaction, unless an
//! account shows its own date or payee for a part of it. The amount and the
//! account change only on a plain transaction: two parts in one currency,
//! one in an account of the user and one in a category or in none yet, so a
//! transfer, a split over several categories and a foreign amount stay the
//! user's to change. A changed amount changes both parts together, so the
//! transaction still balances; a model writes it as exact decimal text in
//! the transaction's currency and nothing is ever rounded for it (D-065).

use super::{Known, Object};
use kassensturz_core::ai_switch::{Field, Permissions};
use kassensturz_core::{
    Account, AccountId, CurrencyCode, Date, Money, Split, Transaction, TransactionId,
};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use crate::error::ToolError;

/// The longest payee or note a model may write.
const MAX_TEXT_CHARS: usize = 200;

/// One side of a [`FieldEdit`]: the value as the book holds it, for the
/// front end to write in the user's format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldValue {
    /// A payee or a note; `None` for none.
    Text(Option<String>),
    /// A day.
    Date(Date),
    /// An amount in the transaction's account: positive is money in.
    Money(Money),
    /// An account, by its name.
    Account(String),
}

/// One field of a transaction that changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldEdit {
    /// Which field.
    pub field: Field,
    /// What it is now.
    pub before: FieldValue,
    /// What it becomes.
    pub after: FieldValue,
}

/// A transaction with some fields changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditTransaction {
    /// The transaction.
    pub transaction: TransactionId,
    /// The day it has now.
    pub date: Date,
    /// Its payee now, if any.
    pub payee: Option<String>,
    /// The account the money moved in or out of, by its name.
    pub account: String,
    /// What it did to that account, in the account's currency.
    pub amount: Money,
    /// The fields that change, in the order the review shows them.
    pub edits: Vec<FieldEdit>,
    /// The transaction as the user reviewed it: applying fails if it
    /// changed since.
    pub(super) reviewed: Transaction,
    /// The transaction as it will be.
    pub(super) edited: Transaction,
}

impl EditTransaction {
    /// The fields this change touches.
    pub fn fields(&self) -> impl Iterator<Item = Field> + '_ {
        self.edits.iter().map(|edit| edit.field)
    }
}

impl Known {
    /// Checks one item of `edit_transactions` and works out the transaction
    /// it makes, under the locks of `allowed`.
    pub(super) fn edit_transaction(
        &self,
        book: &Book,
        value: &Value,
        allowed: Permissions,
    ) -> Result<EditTransaction, ToolError> {
        let item = Object::read(
            value,
            &[
                "transaction_id",
                "date",
                "payee",
                "note",
                "amount",
                "account_id",
            ],
            "edit_transactions",
        )?;
        let id = item
            .uuid("transaction_id")?
            .map(TransactionId::from_bytes)
            .ok_or(ToolError::BadArgument {
                argument: "transaction_id",
            })?;
        let stored = match book.transaction(id) {
            Err(StoreError::UnknownTransaction(_)) => return Err(ToolError::UnknownTransaction),
            other => other?,
        };

        let date = item.string("date")?.map(parse_date).transpose()?;
        let payee = item
            .string("payee")?
            .map(|text| free_text(text, "payee"))
            .transpose()?;
        let note = item
            .string("note")?
            .map(|text| free_text(text, "note"))
            .transpose()?;
        let account = item.uuid("account_id")?.map(AccountId::from_bytes);
        let amount = item.string("amount")?;

        let given = [
            (Field::Amount, amount.is_some()),
            (Field::Date, date.is_some()),
            (Field::Payee, payee.is_some()),
            (Field::Account, account.is_some()),
            (Field::Note, note.is_some()),
        ];
        if given.iter().all(|(_, given)| !given) {
            return Err(ToolError::BadArgument {
                argument: "edit_transactions",
            });
        }
        for (field, given) in given {
            if given && allowed.locked(field) {
                return Err(ToolError::FieldLocked {
                    field: field.name(),
                });
            }
        }

        let mut splits: Vec<Split> = stored.splits().to_vec();
        let mut edits = Vec::new();
        let plain = self.plain_parts(&stored);

        // The account the user's money moved in or out of, for the review.
        let own_index = plain.map(|(own, _)| own).or_else(|| {
            splits.iter().position(|split| {
                split
                    .account
                    .and_then(|id| self.accounts.get(&id))
                    .is_some_and(|account| account.kind.is_account())
            })
        });
        let own_account = own_index
            .and_then(|at| splits[at].account)
            .and_then(|id| self.accounts.get(&id));
        let account_name = own_account.map(|account| account.name.clone());
        let current_amount = own_index
            .map(|at| splits[at].account_amount())
            .unwrap_or_else(|| Money::from_minor_units(0, stored.currency()));

        if let Some(text) = amount {
            let (own, other) = plain.ok_or(ToolError::NotEditable)?;
            if splits[own].original.is_some() {
                return Err(ToolError::NotEditable);
            }
            let currency = stored.currency();
            let digits = book.currency_minor_digits(currency)?;
            let new = parse_amount(text, currency, digits)?;
            if new == splits[own].amount {
                return Err(ToolError::BadArgument { argument: "amount" });
            }
            let opposite = new
                .checked_neg()
                .map_err(|_| ToolError::BadArgument { argument: "amount" })?;
            edits.push(FieldEdit {
                field: Field::Amount,
                before: FieldValue::Money(splits[own].amount),
                after: FieldValue::Money(new),
            });
            splits[own].amount = new;
            splits[other].amount = opposite;
        }

        if let Some(new) = account {
            let (own, _) = plain.ok_or(ToolError::NotEditable)?;
            let target = self.open_account(new, stored.currency())?;
            if splits[own].account == Some(new) {
                return Err(ToolError::BadArgument {
                    argument: "account_id",
                });
            }
            edits.push(FieldEdit {
                field: Field::Account,
                before: FieldValue::Account(account_name.clone().unwrap_or_default()),
                after: FieldValue::Account(target.name.clone()),
            });
            splits[own].account = Some(new);
        }

        let booking_date = match date {
            Some(new) => {
                if new == stored.booking_date() {
                    return Err(ToolError::BadArgument { argument: "date" });
                }
                if splits.iter().any(|split| split.own_date.is_some()) {
                    return Err(ToolError::NotEditable);
                }
                edits.push(FieldEdit {
                    field: Field::Date,
                    before: FieldValue::Date(stored.booking_date()),
                    after: FieldValue::Date(new),
                });
                new
            }
            None => stored.booking_date(),
        };

        let new_payee = match payee {
            Some(new) => {
                if new.as_deref() == stored.payee() {
                    return Err(ToolError::BadArgument { argument: "payee" });
                }
                if splits.iter().any(|split| split.own_payee.is_some()) {
                    return Err(ToolError::NotEditable);
                }
                edits.push(FieldEdit {
                    field: Field::Payee,
                    before: FieldValue::Text(stored.payee().map(str::to_owned)),
                    after: FieldValue::Text(new.clone()),
                });
                new
            }
            None => stored.payee().map(str::to_owned),
        };

        let new_note = match note {
            Some(new) => {
                if new.as_deref() == stored.note() {
                    return Err(ToolError::BadArgument { argument: "note" });
                }
                edits.push(FieldEdit {
                    field: Field::Note,
                    before: FieldValue::Text(stored.note().map(str::to_owned)),
                    after: FieldValue::Text(new.clone()),
                });
                new
            }
            None => stored.note().map(str::to_owned),
        };

        let edited = Transaction::new(stored.id(), booking_date, new_payee, splits)
            .map_err(|_| ToolError::NotEditable)?
            .with_details_of(&stored)
            .with_note(new_note);
        // Shown in the order the review lists them.
        edits.sort_by_key(|edit| {
            Field::ALL
                .iter()
                .position(|field| *field == edit.field)
                .unwrap_or(usize::MAX)
        });
        Ok(EditTransaction {
            transaction: id,
            date: stored.booking_date(),
            payee: stored.payee().map(str::to_owned),
            account: account_name.unwrap_or_default(),
            amount: current_amount,
            edits,
            reviewed: stored,
            edited,
        })
    }

    /// The indexes of the part in an account of the user and of the other
    /// part, when the transaction is plain: two parts in the transaction's
    /// currency, no foreign amount, the other part in a category or in none
    /// yet, so not a transfer, and not delivered by a bank or file.
    fn plain_parts(&self, transaction: &Transaction) -> Option<(usize, usize)> {
        let [first, second] = transaction.splits() else {
            return None;
        };
        let account_of = |split: &Split| split.account.and_then(|id| self.accounts.get(&id));
        let (own, other) = match (account_of(first), account_of(second)) {
            (Some(account), _) if account.kind.is_account() => (0, 1),
            (_, Some(account)) if account.kind.is_account() => (1, 0),
            _ => return None,
        };
        let parts = [first, second];
        let own_account = account_of(parts[own])?;
        let other_is_category =
            account_of(parts[other]).is_none_or(|account| account.kind.is_category());
        let in_one_currency = own_account.currency == transaction.currency()
            && parts.iter().all(|part| part.foreign.is_none());
        // A part a bank or file delivered keeps its account and amount (D-067).
        let delivered = parts[own].imported.is_some();
        (other_is_category && in_one_currency && !delivered).then_some((own, other))
    }

    /// An open account of the user in `currency`, by its ID.
    fn open_account(&self, id: AccountId, currency: CurrencyCode) -> Result<&Account, ToolError> {
        let account = self
            .accounts
            .get(&id)
            .filter(|account| account.kind.is_account() && !account.closed)
            .ok_or(ToolError::UnknownAccount)?;
        if account.currency != currency {
            return Err(ToolError::OtherCurrency);
        }
        Ok(account)
    }
}

/// A day written as `YYYY-MM-DD`, as the read functions write it.
pub(super) fn parse_date(text: &str) -> Result<Date, ToolError> {
    let wellformed = text.len() == 10
        && text.char_indices().all(|(at, c)| match at {
            4 | 7 => c == '-',
            _ => c.is_ascii_digit(),
        });
    wellformed
        .then(|| text.parse::<Date>().ok())
        .flatten()
        .ok_or(ToolError::BadArgument { argument: "date" })
}

/// A payee or note a model wrote, as the book will keep it: trimmed, not
/// too long, without control characters or characters that hide text
/// (threat T-08). Empty means none.
pub(super) fn free_text(text: &str, argument: &'static str) -> Result<Option<String>, ToolError> {
    let text = text.trim();
    let bad = text.chars().count() > MAX_TEXT_CHARS
        || text
            .chars()
            .any(|c| c.is_control() || crate::text::hides_text(c));
    if bad {
        return Err(ToolError::BadArgument { argument });
    }
    Ok((!text.is_empty()).then(|| text.to_owned()))
}

/// Reads an amount written as `[+-]digits[.digits]` in a currency with
/// `digits` decimal places: more places than that are refused, never
/// rounded, and so is zero (D-065).
pub(super) fn parse_amount(
    text: &str,
    currency: CurrencyCode,
    digits: u8,
) -> Result<Money, ToolError> {
    let bad = || ToolError::BadArgument { argument: "amount" };
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let all_digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    let point_without_fraction = unsigned.contains('.') && fraction.is_empty();
    if whole.is_empty()
        || !all_digits(whole)
        || !all_digits(fraction)
        || point_without_fraction
        || fraction.len() > usize::from(digits)
    {
        return Err(bad());
    }
    let padded = format!("{fraction:0<width$}", width = usize::from(digits));
    let mut minor: i128 = 0;
    for digit in whole.bytes().chain(padded.bytes()) {
        minor = minor
            .checked_mul(10)
            .and_then(|minor| minor.checked_add(i128::from(digit - b'0')))
            .ok_or_else(bad)?;
    }
    if minor == 0 {
        return Err(bad());
    }
    if negative {
        minor = -minor;
    }
    let minor = i64::try_from(minor).map_err(|_| bad())?;
    Ok(Money::from_minor_units(minor, currency))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eur() -> CurrencyCode {
        CurrencyCode::new("EUR").expect("valid code")
    }

    fn minor(text: &str, digits: u8) -> Result<i64, &'static str> {
        use kassensturz_core::Report;
        parse_amount(text, eur(), digits)
            .map(Money::minor_units)
            .map_err(|error| error.report().code)
    }

    #[test]
    fn an_amount_is_read_exactly() {
        assert_eq!(minor("12.50", 2), Ok(1250));
        assert_eq!(minor("-3", 2), Ok(-300));
        assert_eq!(minor("+0.05", 2), Ok(5));
        assert_eq!(minor("1234", 0), Ok(1234));
        assert_eq!(minor("1.234", 3), Ok(1234));
    }

    #[test]
    fn an_amount_is_never_rounded_or_guessed() {
        for text in [
            "",
            "-",
            ".5",
            "5.",
            "1,5",
            "1.234",
            "1e3",
            " 5",
            "5 ",
            "0",
            "0.00",
            "-0",
            "9223372036854775808",
            "NaN",
        ] {
            assert_eq!(minor(text, 2), Err("ai-tools.bad-argument"), "{text:?}");
        }
    }

    #[test]
    fn a_payee_is_trimmed_and_empty_means_none() {
        assert_eq!(
            free_text("  Moon Bakery ", "payee").ok(),
            Some(Some("Moon Bakery".to_owned()))
        );
        assert_eq!(free_text("   ", "payee").ok(), Some(None));
        assert!(free_text("a\u{202e}b", "payee").is_err());
        assert!(free_text("a\nb", "note").is_err());
        assert!(free_text(&"x".repeat(201), "note").is_err());
    }

    #[test]
    fn a_day_is_read_only_in_iso_form() {
        assert!(parse_date("2026-10-28").is_ok());
        for text in ["28.10.2026", "2026-1-28", "2026-13-01", "2026-10-28 "] {
            assert!(parse_date(text).is_err(), "{text:?}");
        }
    }
}
