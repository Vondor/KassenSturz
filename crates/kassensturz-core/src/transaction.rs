// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Double-entry transactions and their parts (splits).

use crate::Date;
use crate::account::AccountId;
use crate::currency::CurrencyCode;
use crate::details::TagId;
use crate::id::define_id;
use crate::money::Money;
use crate::rates::DatedRate;

define_id!(
    /// Identifies a transaction within a book.
    TransactionId
);

define_id!(
    /// Identifies one part (split) of a transaction.
    SplitId
);

/// One part of a transaction: an amount booked to one account or category,
/// or to no category yet.
///
/// Users see it as a "Part" (`docs/GLOSSARY.md`, D-017).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split {
    /// Its ID.
    pub id: SplitId,
    /// The account or category the amount is booked to. `None` for a part
    /// that has no category yet, e.g. what an imported purchase was for,
    /// until a Rule or the user picks one (D-017, D-083).
    pub account: Option<AccountId>,
    /// Its amount in the transaction's currency. Positive amounts flow into
    /// the account, negative ones out of it. The amounts of all parts of a
    /// transaction add up to zero.
    pub amount: Money,
    /// For a part booked to an account in another currency than the
    /// transaction's: its amount in that currency, and the rate behind it
    /// (D-043).
    pub foreign: Option<ForeignAmount>,
    /// What a bank or file delivered for this part, kept unchanged (D-067).
    /// Only a part booked to the account a statement is for has it.
    pub imported: Option<ImportedDetails>,
    /// For a part converted from a withdrawn currency, such as an amount in
    /// German marks from an old book: the amount it had, and the fixed rate
    /// it was converted with (D-077, D-104).
    pub original: Option<OriginalAmount>,
    /// The date this part's account shows it with, if not the
    /// transaction's: on the second side of a joined Transfer, the day that
    /// account's bank booked it, so each account's register keeps matching
    /// its statements (D-108). The transaction's own date stays the one date
    /// of the whole Transfer.
    pub own_date: Option<Date>,
    /// The payee this part's account shows, if not the transaction's: on
    /// the second side of a joined Transfer, the one that side had (D-108).
    pub own_payee: Option<String>,
    /// What the user wrote about this part, e.g. what the money was for in
    /// their own words (D-132). Separate from the text a bank delivered
    /// ([`ImportedDetails::remittance_information`]), which stays as it
    /// came.
    pub note: Option<String>,
    /// A cheque number or other reference the user gave the part (D-132).
    pub number: Option<String>,
    /// Whether the user checked the part against a bank statement (D-132).
    pub status: Option<PartStatus>,
    /// The tags the user marked it with, each once, in the user's order
    /// (D-132).
    pub tags: Vec<TagId>,
}

/// How far a part was checked against a bank statement (D-132). KassenSturz
/// doesn't set it yet; it comes over from another program's book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartStatus {
    /// The user saw it on a statement.
    Cleared,
    /// The user reconciled the account with a statement that had it.
    Reconciled,
}

impl Split {
    /// A part of `amount`, in the transaction's currency, booked to
    /// `account`, with a new ID.
    pub fn new(account: AccountId, amount: Money) -> Self {
        Self {
            account: Some(account),
            ..Self::uncategorized(amount)
        }
    }

    /// A part of `amount`, in the transaction's currency, that has no
    /// category yet, with a new ID.
    pub fn uncategorized(amount: Money) -> Self {
        Self {
            id: SplitId::generate(),
            account: None,
            amount,
            foreign: None,
            imported: None,
            original: None,
            own_date: None,
            own_payee: None,
            note: None,
            number: None,
            status: None,
            tags: Vec::new(),
        }
    }

    /// Its amount in its account's currency: the foreign amount of a part in
    /// another currency, otherwise its amount. An account's balance adds up
    /// these.
    pub fn account_amount(&self) -> Money {
        self.foreign
            .as_ref()
            .map_or(self.amount, |foreign| foreign.amount)
    }
}

/// The amount of a part in its account's currency, where that isn't the
/// transaction's (D-043). Example: a hotel bill of 100 USD, paid with a euro
/// card that was charged 92.50 EUR, booked to a category in dollars. The
/// transaction counts in euros: its parts add up to zero there, and the
/// Travel part keeps the 100 USD as its foreign amount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForeignAmount {
    /// The amount in the account's currency, e.g. 100.00 USD. It has the
    /// same sign as the part's amount in the transaction's currency, unless
    /// one of the two is zero.
    pub amount: Money,
    /// The exchange rate from the book's price table that one of the two
    /// amounts was calculated from the other with, copied with its date and
    /// source, so the part keeps it if the rate is changed or removed later
    /// (AGENTS.md §5). `None` if both amounts were given, e.g. by the bank or
    /// the user: then their ratio is the rate.
    pub rate: Option<DatedRate>,
}

/// The amount a part had in a withdrawn currency, before it was converted into
/// the currency that replaced it (D-077, D-104). Example: a purchase of
/// 100.00 DEM from a book kept before the euro is 51.13 EUR now, and keeps
/// the 100.00 DEM and the fixed rate of 1 EUR = 1.95583 DEM.
///
/// It was converted into the part's amount, in the transaction's currency,
/// or into its foreign amount, in its account's: the one in the currency
/// the rate is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OriginalAmount {
    /// The amount in the withdrawn currency, e.g. 100.00 DEM. It has the
    /// same sign as the amount it was converted into, unless one of the two
    /// is zero.
    pub amount: Money,
    /// The fixed rate it was converted with: one unit of the currency that
    /// replaced the withdrawn one costs `rate` units of it, e.g.
    /// 1 EUR = 1.95583 DEM, usually from [`RateSource::Changeover`](crate::RateSource::Changeover).
    pub rate: DatedRate,
}

/// What a bank or file delivered for one part, kept exactly as it was
/// received (D-067), so Rules can be run again (D-031), a statement
/// delivered twice is recognized (D-030), and the original can always be
/// checked. The payee and categories the user edits are kept elsewhere.
///
/// It belongs to the part booked to the account the statement is for, not
/// to the whole transaction: a Transfer matched from two statements (D-021)
/// keeps what both banks delivered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportedDetails {
    /// The bank's own ID for this entry of the statement.
    pub bank_id: Option<String>,
    /// The date the bank booked it.
    pub booking_date: Option<Date>,
    /// The date the money counted from or until, for interest (German:
    /// Wertstellung, D-030).
    pub value_date: Option<Date>,
    /// The other party's name: who paid, or who was paid.
    pub other_party_name: Option<String>,
    /// The other party's IBAN, or whatever account number the bank gave, as
    /// text. It isn't checked, since it is kept as delivered.
    pub other_party_iban: Option<String>,
    /// The remittance information: the text the payment came with (German:
    /// Verwendungszweck).
    pub remittance_information: Option<String>,
}

/// A transaction: two or more parts that add up to exactly zero in the
/// transaction's currency (double-entry, AGENTS.md §5).
///
/// It can only be created through [`Transaction::new`], which checks this, so
/// every `Transaction` balances.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    id: TransactionId,
    booking_date: Date,
    payee: Option<String>,
    currency: CurrencyCode,
    splits: Vec<Split>,
    note: Option<String>,
    entered_on: Option<Date>,
}

impl Transaction {
    /// Checks that `splits` form a valid transaction and creates it.
    ///
    /// The transaction counts in the currency of its parts' amounts, which
    /// must all be the same. A part booked to an account in another currency
    /// also has a [`ForeignAmount`] in that currency (D-043). Changing a
    /// transaction means creating it again with the same ID and the changed
    /// parts, so an edited transaction is checked in the same way (D-017).
    pub fn new(
        id: TransactionId,
        booking_date: Date,
        payee: Option<String>,
        splits: Vec<Split>,
    ) -> Result<Self, TransactionError> {
        let [first, _, ..] = splits.as_slice() else {
            return Err(TransactionError::TooFewSplits {
                found: splits.len(),
            });
        };
        let currency = first.amount.currency();
        for (index, split) in splits.iter().enumerate() {
            if split.amount.currency() != currency {
                return Err(TransactionError::MixedCurrencies {
                    expected: currency,
                    found: split.amount.currency(),
                });
            }
            if splits[..index].iter().any(|earlier| earlier.id == split.id) {
                return Err(TransactionError::DuplicateSplitId(split.id));
            }
            if split.imported.is_some() && split.account.is_none() {
                return Err(TransactionError::ImportedWithoutAccount(split.id));
            }
            if let Some(foreign) = &split.foreign {
                check_foreign(split, foreign, currency)?;
            }
            if let Some(original) = &split.original {
                check_original(split, original, currency)?;
            }
        }
        let total = Money::checked_sum(currency, splits.iter().map(|split| split.amount))
            .map_err(|_| TransactionError::Overflow)?;
        if !total.is_zero() {
            return Err(TransactionError::Unbalanced { total });
        }
        Ok(Self {
            id,
            booking_date,
            payee,
            currency,
            splits,
            note: None,
            entered_on: None,
        })
    }

    /// The transaction with `note` as what the user wrote about it as a
    /// whole (D-132). An empty note is none.
    #[must_use]
    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note.filter(|note| !note.is_empty());
        self
    }

    /// The transaction with `entered_on` as the day it was entered (D-132).
    #[must_use]
    pub fn with_entry_date(mut self, entered_on: Option<Date>) -> Self {
        self.entered_on = entered_on;
        self
    }

    /// The transaction with the note and entry date of `stored`: what a
    /// transaction built again from a stored one, e.g. with other parts,
    /// keeps (D-132).
    #[must_use]
    pub fn with_details_of(self, stored: &Transaction) -> Self {
        self.with_note(stored.note.clone())
            .with_entry_date(stored.entered_on)
    }

    /// The amount the part `open` needs for all of `parts` to add up to zero:
    /// what the transaction is about, minus what the other parts already
    /// have (D-017). `open`'s own amount doesn't count.
    ///
    /// A Parts editor shows it as the amount of the one part the user leaves
    /// open, so nobody has to do the subtraction. It is in the transaction's
    /// currency, the one of `open`'s amount; a part in another currency gets
    /// its foreign amount converted from it.
    pub fn remainder(parts: &[Split], open: SplitId) -> Result<Money, TransactionError> {
        let currency = parts
            .iter()
            .find(|part| part.id == open)
            .ok_or(TransactionError::UnknownSplit(open))?
            .amount
            .currency();
        let mut others = Vec::with_capacity(parts.len());
        for part in parts.iter().filter(|part| part.id != open) {
            if part.amount.currency() != currency {
                return Err(TransactionError::MixedCurrencies {
                    expected: currency,
                    found: part.amount.currency(),
                });
            }
            others.push(part.amount);
        }
        Money::checked_sum(currency, others)
            .and_then(Money::checked_neg)
            .map_err(|_| TransactionError::Overflow)
    }

    /// Its ID.
    pub fn id(&self) -> TransactionId {
        self.id
    }

    /// The date it is shown with: the date the bank booked it, or the user
    /// entered (D-030). What the bank delivered is kept with the part it
    /// delivered ([`Split::imported`]). A part with its own date is shown
    /// with that one in its account ([`Transaction::date_of`]).
    pub fn booking_date(&self) -> Date {
        self.booking_date
    }

    /// Who the money came from or went to, as the user wants it shown. A
    /// part with its own payee shows that one in its account
    /// ([`Transaction::payee_of`]).
    pub fn payee(&self) -> Option<&str> {
        self.payee.as_deref()
    }

    /// The date the account of `part`, one of its parts, shows it with: the
    /// part's own date if it has one, otherwise the transaction's (D-108).
    pub fn date_of(&self, part: &Split) -> Date {
        part.own_date.unwrap_or(self.booking_date)
    }

    /// The payee the account of `part`, one of its parts, shows: the part's
    /// own payee if it has one, otherwise the transaction's (D-108).
    pub fn payee_of<'a>(&'a self, part: &'a Split) -> Option<&'a str> {
        part.own_payee.as_deref().or(self.payee())
    }

    /// What the user wrote about the transaction as a whole, if anything
    /// (D-132). Each part can have a note of its own ([`Split::note`]).
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The day the transaction was entered, if known: another program's
    /// book says so (D-132). Not the booking date, which can be any day.
    pub fn entered_on(&self) -> Option<Date> {
        self.entered_on
    }

    /// The currency it counts in: the one of its parts' amounts.
    pub fn currency(&self) -> CurrencyCode {
        self.currency
    }

    /// Its parts, in the order they were entered. They add up to zero.
    pub fn splits(&self) -> &[Split] {
        &self.splits
    }
}

/// Checks a part's foreign amount: it needs an account, another currency than
/// the transaction's, the same sign, and a rate between the two currencies.
fn check_foreign(
    split: &Split,
    foreign: &ForeignAmount,
    currency: CurrencyCode,
) -> Result<(), TransactionError> {
    if split.account.is_none() {
        return Err(TransactionError::ForeignWithoutAccount(split.id));
    }
    let foreign_currency = foreign.amount.currency();
    if foreign_currency == currency {
        return Err(TransactionError::ForeignInTransactionCurrency(split.id));
    }
    let opposite = (split.amount.is_negative() && foreign.amount.minor_units() > 0)
        || (split.amount.minor_units() > 0 && foreign.amount.is_negative());
    if opposite {
        return Err(TransactionError::ForeignSignMismatch(split.id));
    }
    if let Some(rate) = &foreign.rate {
        let pair = (rate.from, rate.to);
        if pair != (foreign_currency, currency) && pair != (currency, foreign_currency) {
            return Err(TransactionError::RateCurrencyMismatch(split.id));
        }
    }
    Ok(())
}

/// Checks a part's original amount: in neither of the part's currencies, with
/// a rate from one of them into its own, and the same sign as the amount it
/// was converted into. The part keeps one rate, so its foreign amount has
/// none.
fn check_original(
    split: &Split,
    original: &OriginalAmount,
    currency: CurrencyCode,
) -> Result<(), TransactionError> {
    let original_currency = original.amount.currency();
    let foreign = split.foreign.as_ref();
    if original_currency == currency
        || foreign.is_some_and(|foreign| foreign.amount.currency() == original_currency)
    {
        return Err(TransactionError::OriginalInPartCurrency(split.id));
    }
    if foreign.is_some_and(|foreign| foreign.rate.is_some()) {
        return Err(TransactionError::OriginalWithForeignRate(split.id));
    }
    if original.rate.to != original_currency {
        return Err(TransactionError::RateCurrencyMismatch(split.id));
    }
    let converted = if original.rate.from == currency {
        split.amount
    } else {
        match foreign {
            Some(foreign) if foreign.amount.currency() == original.rate.from => foreign.amount,
            _ => return Err(TransactionError::RateCurrencyMismatch(split.id)),
        }
    };
    let opposite = (converted.is_negative() && original.amount.minor_units() > 0)
        || (converted.minor_units() > 0 && original.amount.is_negative());
    if opposite {
        return Err(TransactionError::OriginalSignMismatch(split.id));
    }
    Ok(())
}

/// Why parts don't form a valid transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionError {
    /// A transaction needs at least two parts: where the money comes from and
    /// where it goes.
    TooFewSplits {
        /// How many parts there were.
        found: usize,
    },
    /// Not all parts' amounts are in the same currency. A part in another
    /// currency keeps that amount as its foreign amount (D-043).
    MixedCurrencies {
        /// The currency of the first part.
        expected: CurrencyCode,
        /// The currency of a later part.
        found: CurrencyCode,
    },
    /// Two parts have the same ID.
    DuplicateSplitId(SplitId),
    /// The parts don't add up to zero.
    Unbalanced {
        /// What they add up to instead.
        total: Money,
    },
    /// The parts add up to more than 64 bits can hold.
    Overflow,
    /// The part to calculate the remainder for isn't among the parts.
    UnknownSplit(SplitId),
    /// What a bank or file delivered belongs to the part booked to the
    /// account the statement is for, so that part needs an account.
    ImportedWithoutAccount(SplitId),
    /// A part with an amount in another currency needs an account in that
    /// currency. A part without a category counts in the transaction's
    /// currency.
    ForeignWithoutAccount(SplitId),
    /// A part's foreign amount is in the transaction's own currency.
    ForeignInTransactionCurrency(SplitId),
    /// A part's amount and its foreign amount have opposite signs. Exchange
    /// rates are above zero, so both point the same way.
    ForeignSignMismatch(SplitId),
    /// The rate a part's foreign amount was converted with isn't between the
    /// part's two currencies, or the rate of its original amount isn't from
    /// one of them into the original amount's currency.
    RateCurrencyMismatch(SplitId),
    /// A part's original amount is in the currency of its amount or of its
    /// foreign amount. It is in the withdrawn currency the part was
    /// converted from.
    OriginalInPartCurrency(SplitId),
    /// A part's original amount and the amount it was converted into have
    /// opposite signs. Exchange rates are above zero, so both point the same
    /// way.
    OriginalSignMismatch(SplitId),
    /// A part has both an original amount and a rate for its foreign amount.
    /// A part keeps one rate: a converted part's foreign amount comes with
    /// none, since both of its amounts were given or converted with the
    /// fixed rate.
    OriginalWithForeignRate(SplitId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rates::{DatedRateId, RateSource};

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };
    const USD: CurrencyCode = match CurrencyCode::new("USD") {
        Ok(code) => code,
        Err(_) => panic!("USD is a valid code"),
    };

    fn eur(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, EUR)
    }

    fn usd(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, USD)
    }

    fn split(account: AccountId, minor_units: i64) -> Split {
        Split::new(account, eur(minor_units))
    }

    fn date() -> Date {
        Date::new(2026, 10, 5).expect("valid date")
    }

    fn new(parts: Vec<Split>) -> Result<Transaction, TransactionError> {
        Transaction::new(TransactionId::generate(), date(), None, parts)
    }

    /// The hotel bill of D-043: 100 USD to a category in dollars, 92.50 EUR
    /// from a euro account.
    fn hotel_parts() -> Vec<Split> {
        vec![
            split(AccountId::generate(), -9250),
            Split {
                foreign: Some(ForeignAmount {
                    amount: usd(10_000),
                    rate: None,
                }),
                ..split(AccountId::generate(), 9250)
            },
        ]
    }

    fn rate(from: CurrencyCode, to: CurrencyCode) -> DatedRate {
        DatedRate {
            id: DatedRateId::generate(),
            date: date(),
            from,
            to,
            rate: "1.0856".parse().expect("valid rate"),
            source: RateSource::Manual,
        }
    }

    #[test]
    fn accepts_parts_that_add_up_to_zero() {
        let checking = AccountId::generate();
        let groceries = AccountId::generate();
        let household = AccountId::generate();
        let parts = vec![
            split(checking, -5320),
            split(groceries, 4520),
            split(household, 800),
        ];
        let transaction = Transaction::new(
            TransactionId::generate(),
            date(),
            Some("Corner Grocery".into()),
            parts.clone(),
        )
        .expect("balanced");
        assert_eq!(transaction.splits(), parts.as_slice());
        assert_eq!(transaction.currency(), EUR);
        assert_eq!(transaction.payee(), Some("Corner Grocery"));
    }

    #[test]
    fn rejects_parts_that_do_not_add_up() {
        let result = new(vec![
            split(AccountId::generate(), -5320),
            split(AccountId::generate(), 5300),
        ]);
        assert_eq!(
            result,
            Err(TransactionError::Unbalanced { total: eur(-20) })
        );
    }

    #[test]
    fn needs_at_least_two_parts() {
        for parts in [vec![], vec![split(AccountId::generate(), 0)]] {
            let found = parts.len();
            assert_eq!(new(parts), Err(TransactionError::TooFewSplits { found }));
        }
    }

    #[test]
    fn rejects_mixed_currencies() {
        let mut parts = vec![
            split(AccountId::generate(), -100),
            split(AccountId::generate(), 100),
        ];
        parts[1].amount = usd(100);
        assert_eq!(
            new(parts),
            Err(TransactionError::MixedCurrencies {
                expected: EUR,
                found: USD
            })
        );
    }

    #[test]
    fn rejects_duplicate_part_ids() {
        let first = split(AccountId::generate(), -100);
        let mut second = split(AccountId::generate(), 100);
        let id = first.id;
        second.id = id;
        assert_eq!(
            new(vec![first, second]),
            Err(TransactionError::DuplicateSplitId(id))
        );
    }

    #[test]
    fn reports_overflow_instead_of_wrapping() {
        let result = new(vec![
            split(AccountId::generate(), i64::MAX),
            split(AccountId::generate(), i64::MAX),
            split(AccountId::generate(), 2),
        ]);
        assert_eq!(result, Err(TransactionError::Overflow));
    }

    #[test]
    fn a_part_can_be_without_a_category() {
        let checking = split(AccountId::generate(), -5320);
        let open = Split::uncategorized(eur(5320));
        assert_eq!(open.account, None);
        let transaction = new(vec![checking, open.clone()]).expect("balanced");
        assert_eq!(transaction.splits()[1], open);
    }

    #[test]
    fn the_remainder_is_what_the_other_parts_leave() {
        // A purchase of 53.20 broken down: 40 and 8 are entered, the open
        // part gets the rest.
        let mut parts = vec![
            split(AccountId::generate(), -5320),
            split(AccountId::generate(), 4000),
            split(AccountId::generate(), 800),
            Split::uncategorized(eur(123_456)),
        ];
        let open = parts[3].id;
        let remainder = Transaction::remainder(&parts, open).expect("remainder");
        assert_eq!(remainder, eur(520));
        parts[3].amount = remainder;
        new(parts).expect("balanced");
    }

    #[test]
    fn the_remainder_needs_its_part_and_one_currency() {
        let mut parts = vec![
            split(AccountId::generate(), -100),
            split(AccountId::generate(), 0),
        ];
        let missing = SplitId::generate();
        assert_eq!(
            Transaction::remainder(&parts, missing),
            Err(TransactionError::UnknownSplit(missing))
        );
        parts[0].amount = usd(-100);
        assert_eq!(
            Transaction::remainder(&parts, parts[1].id),
            Err(TransactionError::MixedCurrencies {
                expected: EUR,
                found: USD
            })
        );
        parts[0].amount = Money::from_minor_units(i64::MIN, EUR);
        assert_eq!(
            Transaction::remainder(&parts, parts[1].id),
            Err(TransactionError::Overflow)
        );
    }

    #[test]
    fn a_part_in_another_currency_balances_in_the_transactions() {
        let transaction = new(hotel_parts()).expect("balanced in euros");
        assert_eq!(transaction.currency(), EUR);
        let travel = &transaction.splits()[1];
        assert_eq!(travel.amount, eur(9250));
        assert_eq!(travel.account_amount(), usd(10_000));
        assert_eq!(transaction.splits()[0].account_amount(), eur(-9250));
    }

    #[test]
    fn a_part_in_another_currency_may_record_the_rate_used() {
        for (from, to) in [(USD, EUR), (EUR, USD)] {
            let mut parts = hotel_parts();
            parts[1].foreign.as_mut().expect("foreign").rate = Some(rate(from, to));
            new(parts).expect("a rate between the part's two currencies");
        }
        let mut parts = hotel_parts();
        let yen = CurrencyCode::new("JPY").expect("valid code");
        parts[1].foreign.as_mut().expect("foreign").rate = Some(rate(USD, yen));
        let id = parts[1].id;
        assert_eq!(new(parts), Err(TransactionError::RateCurrencyMismatch(id)));
    }

    #[test]
    fn a_foreign_amount_needs_an_account_another_currency_and_the_same_sign() {
        /// The hotel bill with its Travel part changed by `change`, and that
        /// part's ID.
        fn changed(
            change: impl FnOnce(&mut Split),
        ) -> (Result<Transaction, TransactionError>, SplitId) {
            let mut parts = hotel_parts();
            change(&mut parts[1]);
            let id = parts[1].id;
            (new(parts), id)
        }
        fn foreign(part: &mut Split) -> &mut ForeignAmount {
            part.foreign.as_mut().expect("a part in another currency")
        }

        let (result, id) = changed(|part| part.account = None);
        assert_eq!(result, Err(TransactionError::ForeignWithoutAccount(id)));
        let (result, id) = changed(|part| foreign(part).amount = eur(9250));
        assert_eq!(
            result,
            Err(TransactionError::ForeignInTransactionCurrency(id))
        );
        let (result, id) = changed(|part| foreign(part).amount = usd(-10_000));
        assert_eq!(result, Err(TransactionError::ForeignSignMismatch(id)));

        // A tiny amount can round to zero in the other currency.
        let mut parts = hotel_parts();
        parts[0].amount = eur(0);
        parts[1].amount = eur(0);
        foreign(&mut parts[1]).amount = usd(1);
        new(parts).expect("zero and a positive amount");
    }

    const DEM: CurrencyCode = match CurrencyCode::new("DEM") {
        Ok(code) => code,
        Err(_) => panic!("DEM is a valid code"),
    };

    fn dem(minor_units: i64) -> Money {
        Money::from_minor_units(minor_units, DEM)
    }

    /// 100.00 DEM converted at the fixed rate of 1 EUR = 1.95583 DEM.
    fn mark_original() -> OriginalAmount {
        OriginalAmount {
            amount: dem(10_000),
            rate: DatedRate {
                id: DatedRateId::generate(),
                date: Date::new(1999, 1, 1).expect("valid date"),
                from: EUR,
                to: DEM,
                rate: "1.95583".parse().expect("valid rate"),
                source: RateSource::Changeover,
            },
        }
    }

    /// A purchase of 100.00 DEM from a book kept in marks, in euros now.
    fn converted_parts() -> Vec<Split> {
        let original = mark_original();
        vec![
            Split {
                original: Some(OriginalAmount {
                    amount: dem(-10_000),
                    ..original
                }),
                ..split(AccountId::generate(), -5113)
            },
            Split {
                original: Some(original),
                ..split(AccountId::generate(), 5113)
            },
        ]
    }

    #[test]
    fn a_converted_part_keeps_its_original_amount() {
        let transaction = new(converted_parts()).expect("balanced in euros");
        let part = &transaction.splits()[1];
        assert_eq!(part.account_amount(), eur(5113));
        assert_eq!(
            part.original.map(|original| original.amount),
            Some(dem(10_000))
        );

        // In a transaction in dollars, a part booked to an account in marks
        // that is in euros now: the euro amount is converted from the marks.
        let mut parts = hotel_parts();
        parts[1].foreign = Some(ForeignAmount {
            amount: eur(5113),
            rate: None,
        });
        parts[0].amount = usd(-5500);
        parts[1].amount = usd(5500);
        parts[1].original = Some(mark_original());
        new(parts).expect("converted into the foreign amount");
    }

    #[test]
    fn an_original_amount_is_in_a_third_currency_with_a_rate_into_it() {
        /// The converted purchase with its second part changed by `change`,
        /// and that part's ID.
        fn changed(
            change: impl FnOnce(&mut Split),
        ) -> (Result<Transaction, TransactionError>, SplitId) {
            let mut parts = converted_parts();
            change(&mut parts[1]);
            let id = parts[1].id;
            (new(parts), id)
        }
        fn original(part: &mut Split) -> &mut OriginalAmount {
            part.original.as_mut().expect("a converted part")
        }

        let (result, id) = changed(|part| original(part).amount = eur(10_000));
        assert_eq!(result, Err(TransactionError::OriginalInPartCurrency(id)));
        let (result, id) = changed(|part| {
            part.foreign = Some(ForeignAmount {
                amount: dem(10_000),
                rate: None,
            });
        });
        assert_eq!(result, Err(TransactionError::OriginalInPartCurrency(id)));
        let (result, id) = changed(|part| original(part).amount = dem(-10_000));
        assert_eq!(result, Err(TransactionError::OriginalSignMismatch(id)));
        let (result, id) = changed(|part| original(part).rate.to = USD);
        assert_eq!(result, Err(TransactionError::RateCurrencyMismatch(id)));
        let (result, id) = changed(|part| original(part).rate.from = USD);
        assert_eq!(result, Err(TransactionError::RateCurrencyMismatch(id)));
        let (result, id) = changed(|part| {
            part.foreign = Some(ForeignAmount {
                amount: usd(5500),
                rate: Some(rate(EUR, USD)),
            });
        });
        assert_eq!(result, Err(TransactionError::OriginalWithForeignRate(id)));

        // A tiny amount can convert to zero.
        let mut parts = converted_parts();
        parts[0].amount = eur(0);
        parts[1].amount = eur(0);
        original(&mut parts[1]).amount = dem(1);
        new(parts).expect("zero and a positive amount");
    }

    #[test]
    fn imported_details_belong_to_a_part_with_an_account() {
        // The IBAN is the example for Germany in SWIFT's IBAN Registry, never
        // one that could belong to someone (D-055).
        let details = ImportedDetails {
            bank_id: Some("2026100500017".into()),
            booking_date: Some(date()),
            value_date: Some(date()),
            other_party_name: Some("Corner Grocery".into()),
            other_party_iban: Some("DE89370400440532013000".into()),
            remittance_information: Some("Card payment".into()),
        };
        let mut parts = vec![
            split(AccountId::generate(), -5320),
            Split::uncategorized(eur(5320)),
        ];
        parts[0].imported = Some(details.clone());
        new(parts.clone()).expect("the checking account's part came from the bank");
        parts[1].imported = Some(details);
        assert_eq!(
            new(parts.clone()),
            Err(TransactionError::ImportedWithoutAccount(parts[1].id))
        );
    }
}
