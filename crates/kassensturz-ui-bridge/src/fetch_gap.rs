// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fetch gap warning (M5.U, D-180, D-181): a linked account whose new
//! transactions were last fetched from the bank longer ago than a limit
//! shows a plain note on the overview. Banks only list statements back a
//! limited time, so a long gap can leave transactions out for good. The limit
//! belongs to the computer (D-168, `settings.rs`), not to the book, and can be
//! turned off.

use kassensturz_core::{AccountId, Date};
use kassensturz_store::{Book, StoreError};

use crate::format::Locale;

/// The limit in days when the user chose none (Q-103).
pub const DEFAULT_DAYS: u32 = 60;

/// The word the settings file keeps for "no warning".
pub const OFF: &str = "off";

/// The limits the Settings page offers, as the words the settings file keeps
/// them by; the page owns the labels (D-034).
pub const CHOICES: [&str; 5] = ["30", "60", "90", "180", OFF];

/// An account whose last fetch is longer ago than the limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchGap {
    /// The account in the book.
    pub account_id: AccountId,
    /// Its name.
    pub account: String,
    /// The day it was last fetched, in the locale's short format.
    pub last_fetched: String,
    /// How many days ago that was.
    pub days: u32,
}

/// The limit a settings word stands for: `None` for "off", the default for a
/// word nobody here wrote (an empty entry, or a newer build's choice).
pub fn limit_of(word: &str) -> Option<u32> {
    match word {
        OFF => None,
        _ => Some(
            CHOICES
                .iter()
                .find(|choice| **choice == word)
                .and_then(|choice| choice.parse().ok())
                .unwrap_or(DEFAULT_DAYS),
        ),
    }
}

/// The open linked accounts whose last fetch was more than `limit` days
/// before `today`, longest gap first. Accounts never fetched, closed ones and
/// a last fetch dated after `today` (the clock was set back) give no warning;
/// `None` for the limit gives none at all.
pub fn gaps(
    book: &Book,
    today: Date,
    limit: Option<u32>,
    locale: &impl Locale,
) -> Result<Vec<FetchGap>, StoreError> {
    let Some(limit) = limit else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    for link in book.bank_links()? {
        let Some(last) = link.last_fetched_on else {
            continue;
        };
        let account = book.account(link.account)?;
        if account.closed {
            continue;
        }
        let days = last
            .until(today)
            .ok()
            .and_then(|span| u32::try_from(span.get_days()).ok());
        if let Some(days) = days.filter(|days| *days > limit) {
            found.push(FetchGap {
                account_id: link.account,
                account: account.name,
                last_fetched: locale.short_date(last),
                days,
            });
        }
    }
    found.sort_by(|a, b| b.days.cmp(&a.days).then_with(|| a.account.cmp(&b.account)));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{AccountType, BankLink};

    use super::*;
    use crate::accounts::{NewAccount, add_account, close_account};
    use crate::register::tests::TestLocale;

    fn day(year: i16, month: i8, date: i8) -> Date {
        Date::new(year, month, date).expect("a date")
    }

    /// An account made the way the screens make it, linked to the example
    /// IBAN and fetched on `fetched`, through the store's own operations.
    fn linked(book: &mut Book, name: &str, iban: &str, fetched: Option<Date>) -> AccountId {
        let account = add_account(
            book,
            &NewAccount {
                name,
                account_type: AccountType::Checking,
                currency: "EUR",
                balance: "",
                date: "2026-01-01",
            },
            &TestLocale,
        )
        .expect("account added");
        book.link_bank_account(&BankLink {
            account,
            iban: Some(iban.to_owned()),
            bank_code: None,
            number: None,
            last_fetched_on: None,
        })
        .expect("linked");
        if let Some(on) = fetched {
            book.note_bank_fetch(account, on).expect("fetch noted");
        }
        account
    }

    #[test]
    fn words_stand_for_limits() {
        assert_eq!(limit_of("30"), Some(30));
        assert_eq!(limit_of("180"), Some(180));
        assert_eq!(limit_of("off"), None);
        // Nothing written, or a word this build doesn't offer: the default.
        assert_eq!(limit_of(""), Some(DEFAULT_DAYS));
        assert_eq!(limit_of("7"), Some(DEFAULT_DAYS));
        assert_eq!(limit_of("soon"), Some(DEFAULT_DAYS));
        for choice in CHOICES {
            assert_eq!(limit_of(choice).is_none(), choice == OFF, "{choice}");
        }
    }

    #[test]
    fn a_gap_longer_than_the_limit_is_found_longest_first() {
        let mut book = Book::create_in_memory().expect("book in memory");
        let today = day(2026, 10, 25);
        // 61 days ago, 60 days ago (not longer than the limit), 100 days ago.
        let _ = linked(
            &mut book,
            "Giro",
            "DE89370400440532013000",
            Some(day(2026, 8, 25)),
        );
        let _ = linked(
            &mut book,
            "Card",
            "DE02120300000000202051",
            Some(day(2026, 8, 26)),
        );
        let old = linked(
            &mut book,
            "Savings",
            "DE02500105170137075030",
            Some(day(2026, 7, 17)),
        );

        let found = gaps(&book, today, Some(60), &TestLocale).expect("gaps");
        let shown: Vec<(&str, u32)> = found
            .iter()
            .map(|gap| (gap.account.as_str(), gap.days))
            .collect();
        assert_eq!(shown, [("Savings", 100), ("Giro", 61)]);
        assert_eq!(found[0].account_id, old);
        assert_eq!(
            found[0].last_fetched,
            TestLocale.short_date(day(2026, 7, 17))
        );

        // A longer limit lets the shorter gap go; "off" gives none.
        let found = gaps(&book, today, Some(90), &TestLocale).expect("gaps");
        assert_eq!(found.len(), 1);
        assert!(
            gaps(&book, today, None, &TestLocale)
                .expect("gaps")
                .is_empty()
        );
    }

    #[test]
    fn never_fetched_closed_and_future_fetches_give_no_warning() {
        let mut book = Book::create_in_memory().expect("book in memory");
        let today = day(2026, 10, 25);
        let _ = linked(&mut book, "Never", "DE89370400440532013000", None);
        let closed = linked(
            &mut book,
            "Closed",
            "DE02120300000000202051",
            Some(day(2026, 1, 1)),
        );
        let _ = linked(
            &mut book,
            "Future",
            "DE02500105170137075030",
            Some(day(2027, 1, 1)),
        );
        close_account(&mut book, &closed.to_string()).expect("closed");
        assert!(
            gaps(&book, today, Some(60), &TestLocale)
                .expect("gaps")
                .is_empty()
        );
    }

    #[test]
    fn a_fetch_clears_the_warning() {
        let mut book = Book::create_in_memory().expect("book in memory");
        let today = day(2026, 10, 25);
        let account = linked(
            &mut book,
            "Giro",
            "DE89370400440532013000",
            Some(day(2026, 1, 1)),
        );
        assert_eq!(
            gaps(&book, today, Some(60), &TestLocale)
                .expect("gaps")
                .len(),
            1
        );
        book.note_bank_fetch(account, today).expect("fetch noted");
        assert!(
            gaps(&book, today, Some(60), &TestLocale)
                .expect("gaps")
                .is_empty()
        );
    }
}
