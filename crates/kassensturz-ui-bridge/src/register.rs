// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An account's register, ready for display: every amount and date is
//! already text in the user's regional format (D-065).

use kassensturz_core::AccountId;
use kassensturz_store::{Book, StoreError};

use crate::format::Locale;

/// What the transaction list shows for one account.
#[derive(Debug, PartialEq, Eq)]
pub struct RegisterView {
    /// The account's name.
    pub account_name: String,
    /// Its balance.
    pub balance: String,
    /// Its transactions, newest first, like in a banking app.
    pub rows: Vec<RegisterRow>,
}

/// One transaction in the list.
#[derive(Debug, PartialEq, Eq)]
pub struct RegisterRow {
    /// The booking date.
    pub date: String,
    /// Who the money came from or went to; may be empty.
    pub other_party: String,
    /// The amount, as seen from the account.
    pub amount: String,
    /// Whether money came in, shown as "From" rather than "To" (D-034).
    pub incoming: bool,
}

/// Reads `account`'s register from `book` and formats it for `locale`.
pub fn register_view(
    book: &Book,
    account: AccountId,
    locale: &impl Locale,
) -> Result<RegisterView, StoreError> {
    let details = book.account(account)?;
    let minor_digits = book.currency_minor_digits(details.currency)?;
    let format = locale.amount_format(details.currency);
    let text = |amount| format.format(amount, minor_digits, |value| locale.group_digits(value));

    let rows = book
        .register(account)?
        .into_iter()
        .rev()
        .map(|entry| RegisterRow {
            date: locale.short_date(entry.booking_date),
            other_party: entry.payee.unwrap_or_default(),
            amount: text(entry.amount),
            incoming: entry.amount.minor_units() > 0,
        })
        .collect();
    Ok(RegisterView {
        account_name: details.name,
        balance: text(book.balance(account)?),
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{Affixes, AmountFormat};
    use crate::sample_book;
    use kassensturz_core::{CurrencyCode, Date};

    /// A fixed, German-like format, independent of the machine's settings.
    struct TestLocale;

    impl Locale for TestLocale {
        fn amount_format(&self, _currency: CurrencyCode) -> AmountFormat {
            AmountFormat {
                decimal_point: ",".into(),
                zero_digit: '0',
                positive: Affixes::spaced(String::new(), " €".into()),
                negative: Affixes::spaced("-".into(), " €".into()),
            }
        }

        fn group_digits(&self, value: u64) -> String {
            value.to_string()
        }

        fn short_date(&self, date: Date) -> String {
            date.to_string()
        }
    }

    #[test]
    fn shows_the_sample_account_newest_first() {
        let sample = sample_book::create().expect("sample book");
        let view = register_view(&sample.book, sample.checking, &TestLocale).expect("view");
        assert_eq!(view.account_name, "Checking account");
        assert_eq!(view.balance, "1369,35 €");
        let rows: Vec<_> = view
            .rows
            .iter()
            .map(|row| {
                (
                    row.date.as_str(),
                    row.other_party.as_str(),
                    row.amount.as_str(),
                    row.incoming,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("2026-10-12", "Corner Grocery", "-27,45 €", false),
                ("2026-10-05", "Corner Grocery", "-53,20 €", false),
                ("2026-10-02", "Example Housing Co.", "-950,00 €", false),
                ("2026-10-01", "Example Employer Ltd.", "2400,00 €", true),
            ]
        );
    }
}
