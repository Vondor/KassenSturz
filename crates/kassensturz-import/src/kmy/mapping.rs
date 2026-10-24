// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A `.kmy` file's records in KassenSturz's terms: which of KMyMoney's
//! accounts are accounts and which are categories, amounts from fractions,
//! and what a book can't hold counted (D-029, D-101).

use std::collections::{BTreeMap, HashMap, HashSet};

use kassensturz_core::{
    AccountKind, AccountType, Currency, CurrencyCode, Date, ImportedDetails, MatchMode, PartStatus,
    PayeeRecognition, TextField, TextOperator,
};

use super::fraction::Fraction;
use super::records::{
    RawAccount, RawCurrency, RawPayee, RawPrice, RawSplit, RawTransaction, Records, Skips,
};
use crate::amount;
use crate::book::{
    BookAccount, BookCondition, BookInstitution, BookPart, BookPayee, BookRate, BookRule, BookTag,
    BookTransaction, ImportedBook, Replacement, Skipped, SkippedData, UnlistedCurrency,
};
use crate::error::ImportError;

/// KMyMoney's top-level accounts, one for each kind, which every other
/// account and category sits inside. They aren't imported: a book has its
/// kinds without them.
const TOP_LEVEL: [(&str, AccountKind); 5] = [
    ("AStd::Asset", AccountKind::Asset),
    ("AStd::Liability", AccountKind::Liability),
    ("AStd::Income", AccountKind::Income),
    ("AStd::Expense", AccountKind::Expense),
    ("AStd::Equity", AccountKind::Equity),
];

/// KMyMoney's numbers for the types a book has too, as KMyMoney names
/// them in the SQLite version of the sample (`accountTypeString`, D-102):
/// `1` Girokonto, `2` Sparkonto, `3` Bargeld. Other types come in without
/// one; their kind still comes from the top-level account they sit in.
const TYPES: [(&str, AccountType); 3] = [
    ("1", AccountType::Checking),
    ("2", AccountType::Savings),
    ("3", AccountType::Cash),
];

/// The book that `records` describe.
pub(super) fn book(records: Records) -> Result<ImportedBook, ImportError> {
    let mut skipped = records.skipped;
    let accounts = Accounts::new(&records.accounts, &records.securities)?;
    let mut currencies = Currencies::new(&records.currencies)?;
    for account in &records.accounts {
        if accounts.investments.contains(account.id.as_str()) {
            skipped.add(SkippedData::Investments);
        }
    }
    let mut book_accounts = Vec::with_capacity(records.accounts.len());
    for account in &records.accounts {
        if let Some(book_account) = accounts.book_account(account, &mut currencies, &mut skipped)? {
            book_accounts.push(book_account);
        }
    }

    let institutions: Vec<BookInstitution> = records
        .institutions
        .iter()
        .map(|institution| BookInstitution {
            key: institution.id.clone(),
            name: institution.name.clone(),
            bank_code: text(&institution.bank_code),
            contact: text(&institution.contact),
            address: institution.address.lines(),
            phone: text(&institution.address.phone),
        })
        .collect();
    let payees: HashMap<&str, &str> = records
        .payees
        .iter()
        .map(|payee| (payee.id.as_str(), payee.name.as_str()))
        .collect();
    let book_payees: Vec<BookPayee> = records.payees.iter().map(payee).collect();
    let rules = payee_rules(&records.payees, &book_accounts, &mut skipped);
    let tags: Vec<BookTag> = records
        .tags
        .iter()
        .map(|tag| BookTag {
            key: tag.id.clone(),
            name: tag.name.clone(),
        })
        .collect();
    let tag_keys: HashSet<&str> = records.tags.iter().map(|tag| tag.id.as_str()).collect();
    let mut transactions = Vec::with_capacity(records.transactions.len());
    for raw in &records.transactions {
        let transaction = transaction(
            raw,
            &accounts,
            &mut currencies,
            &payees,
            &tag_keys,
            &mut skipped,
        )?;
        transactions.push(transaction);
    }

    let mut rates = Vec::new();
    let mut seen = HashSet::new();
    for price in &records.prices {
        let date = date(&price.date).map_err(|error| error.at(price.line))?;
        let fraction = fraction(&price.price).map_err(|error| error.at(price.line))?;
        let rate = match (known_currency(&price.from), known_currency(&price.to)) {
            (Some(from), Some(to)) if from != to => fraction
                .rate()
                .map(|rate| BookRate {
                    date,
                    from,
                    to,
                    rate,
                })
                // A book holds one rate a day for each direction.
                .filter(|_| seen.insert((from, to, date))),
            _ => None,
        };
        match rate {
            Some(rate) => rates.push(rate),
            None => skipped.add(SkippedData::Rates),
        }
    }

    let mut suggestions = suggestions(&records.prices, &currencies.unlisted);
    Ok(ImportedBook {
        institutions,
        accounts: book_accounts,
        payees: book_payees,
        rules,
        tags,
        transactions,
        rates,
        unlisted: currencies
            .unlisted
            .into_values()
            .map(|currency| UnlistedCurrency {
                currency,
                suggestion: suggestions.remove(&currency.code()),
            })
            .collect(),
        skipped: skipped
            .into_counts()
            .into_iter()
            .map(|(what, count)| Skipped { what, count })
            .collect(),
    })
}

/// What the importer knows about each account of the file.
struct Accounts<'a> {
    /// Every account by its ID, top-level ones included.
    by_id: HashMap<&'a str, &'a RawAccount>,
    /// The kind of every account but the top-level ones, from the
    /// top-level account it sits in.
    kinds: HashMap<&'a str, AccountKind>,
    /// The accounts that count in a security: investments, which the book
    /// leaves out.
    investments: HashSet<&'a str>,
}

impl<'a> Accounts<'a> {
    fn new(accounts: &'a [RawAccount], securities: &HashSet<String>) -> Result<Self, ImportError> {
        let malformed = |account: &RawAccount| ImportError::Malformed { line: account.line };
        let mut by_id = HashMap::with_capacity(accounts.len());
        for account in accounts {
            if by_id.insert(account.id.as_str(), account).is_some() {
                return Err(malformed(account));
            }
        }
        let mut kinds = HashMap::with_capacity(accounts.len());
        let mut investments = HashSet::new();
        for account in accounts {
            if account.parent.is_empty() {
                // A top-level account must be one of the five.
                top_level(&account.id).ok_or_else(|| malformed(account))?;
                continue;
            }
            // Up the tree to the top-level account; a tree that loops or
            // points nowhere is broken.
            let mut parent = account;
            let mut steps = 0;
            let kind = loop {
                if parent.parent.is_empty() {
                    break top_level(&parent.id).ok_or_else(|| malformed(account))?;
                }
                parent = by_id
                    .get(parent.parent.as_str())
                    .ok_or_else(|| malformed(account))?;
                steps += 1;
                if steps > accounts.len() {
                    return Err(malformed(account));
                }
            };
            kinds.insert(account.id.as_str(), kind);
            if securities.contains(&account.currency) {
                investments.insert(account.id.as_str());
            }
        }
        Ok(Self {
            by_id,
            kinds,
            investments,
        })
    }

    /// The book's account for `account`, or `None` if the book leaves it
    /// out: a top-level account or an investment.
    fn book_account(
        &self,
        account: &RawAccount,
        currencies: &mut Currencies<'_>,
        skipped: &mut Skips,
    ) -> Result<Option<BookAccount>, ImportError> {
        let Some(&kind) = self.kinds.get(account.id.as_str()) else {
            return Ok(None);
        };
        if self.investments.contains(account.id.as_str()) {
            return Ok(None);
        }
        let currency = currencies.get(code(&account.currency, account.line)?, account.line)?;
        let account_type = TYPES
            .iter()
            .find(|(number, _)| *number == account.account_type)
            .map(|&(_, account_type)| account_type)
            .filter(|account_type| account_type.kind() == kind);
        let parent_is_top = self
            .by_id
            .get(account.parent.as_str())
            .is_some_and(|parent| parent.parent.is_empty());
        // Categories keep their tree, and an account the account it was
        // grouped under (D-132); the top-level accounts aren't in the book.
        let parent = (!parent_is_top).then(|| account.parent.clone());
        if account.online_banking {
            skipped.add(SkippedData::OnlineBanking);
        }
        let institution = text(&account.institution);
        if institution.is_some() && !kind.is_account() {
            return Err(ImportError::Malformed { line: account.line });
        }
        Ok(Some(BookAccount {
            key: account.id.clone(),
            name: account.name.clone(),
            kind,
            account_type,
            currency,
            parent,
            // The samples close no account, so the importer doesn't know
            // how a file says it: every account comes in open, which hides
            // no money (D-081).
            closed: false,
            number: kind.is_account().then(|| text(&account.number)).flatten(),
            notes: text(&account.description),
            institution,
        }))
    }
}

/// The currencies the file's amounts are in: with the decimal places
/// KassenSturz's tables give them, or for one neither table lists, those of
/// the file's own list (D-106).
struct Currencies<'a> {
    /// The file's list, by code.
    listed: HashMap<&'a str, &'a RawCurrency>,
    /// The currencies neither table lists that accounts or transactions
    /// are in, by code.
    unlisted: BTreeMap<CurrencyCode, Currency>,
}

impl<'a> Currencies<'a> {
    fn new(listed: &'a [RawCurrency]) -> Result<Self, ImportError> {
        let mut by_code = HashMap::with_capacity(listed.len());
        for currency in listed {
            if by_code.insert(currency.id.as_str(), currency).is_some() {
                return Err(ImportError::Malformed {
                    line: currency.line,
                });
            }
        }
        Ok(Self {
            listed: by_code,
            unlisted: BTreeMap::new(),
        })
    }

    /// The currency `code`, used on `line`. One that neither KassenSturz's
    /// tables nor the file's list has is `import.unknown-currency` there;
    /// one whose fraction in the file's list isn't a power of ten, as `100`
    /// for cents is, is `import.malformed` on the list's line.
    fn get(&mut self, code: CurrencyCode, line: Option<u64>) -> Result<Currency, ImportError> {
        if let Ok(currency) = amount::currency(code) {
            return Ok(currency);
        }
        if let Some(&currency) = self.unlisted.get(&code) {
            return Ok(currency);
        }
        let listed = self
            .listed
            .get(code.as_str())
            .ok_or(ImportError::UnknownCurrency {
                currency: code,
                line,
            })?;
        let currency = decimal_places(&listed.fraction)
            .and_then(|digits| Currency::new(code, digits))
            .ok_or(ImportError::Malformed { line: listed.line })?;
        self.unlisted.insert(code, currency);
        Ok(currency)
    }
}

/// The decimal places a smallest fraction such as `100` stands for: 2. It
/// must be a power of ten.
fn decimal_places(fraction: &str) -> Option<u8> {
    let zeros = fraction.strip_prefix('1')?;
    if !zeros.bytes().all(|byte| byte == b'0') {
        return None;
    }
    u8::try_from(zeros.len()).ok()
}

/// What the file's prices say replaced each of the `unlisted` currencies:
/// the latest price between it and a current currency whose rate has an
/// exact decimal form, turned into how many units of it one unit of the
/// current currency was worth (D-106). A price into another withdrawn
/// currency, such as the old lev into the lev (BGN), which the euro replaced
/// since, suggests nothing.
fn suggestions(
    prices: &[RawPrice],
    unlisted: &BTreeMap<CurrencyCode, Currency>,
) -> HashMap<CurrencyCode, Replacement> {
    let mut suggestions: HashMap<CurrencyCode, Replacement> = HashMap::new();
    for price in prices {
        let (Ok(date), Ok(fraction)) = (date(&price.date), fraction(&price.price)) else {
            continue;
        };
        let code = |text: &str| CurrencyCode::new(text).ok();
        let (withdrawn, successor, rate) = match (code(&price.from), code(&price.to)) {
            // One unit of the withdrawn currency costs `fraction` units of
            // the current one.
            (Some(from), Some(to)) if unlisted.contains_key(&from) => {
                (from, to, fraction.inverse().and_then(Fraction::rate))
            }
            (Some(from), Some(to)) if unlisted.contains_key(&to) => (to, from, fraction.rate()),
            _ => continue,
        };
        let Some(rate) = rate else {
            continue;
        };
        if successor.iso_minor_digits().is_none() {
            continue;
        }
        let later = suggestions
            .get(&withdrawn)
            .is_none_or(|earlier| earlier.date <= date);
        if later {
            suggestions.insert(
                withdrawn,
                Replacement {
                    successor,
                    rate,
                    date,
                },
            );
        }
    }
    suggestions
}

/// The kind of the top-level account `id`, if it is one of the five.
fn top_level(id: &str) -> Option<AccountKind> {
    TOP_LEVEL
        .iter()
        .find(|(top, _)| *top == id)
        .map(|&(_, kind)| kind)
}

/// One transaction of the book.
fn transaction(
    raw: &RawTransaction,
    accounts: &Accounts<'_>,
    currencies: &mut Currencies<'_>,
    payees: &HashMap<&str, &str>,
    tags: &HashSet<&str>,
    skipped: &mut Skips,
) -> Result<BookTransaction, ImportError> {
    let at_transaction = |error: ImportError| error.at(raw.line);
    let currency = currencies.get(code(&raw.currency, raw.line)?, raw.line)?;
    // A book has one payee for a transaction; KMyMoney keeps one for each
    // split, and the first one given stands for the transaction.
    let payee = match raw.splits.iter().find(|split| !split.payee.is_empty()) {
        Some(split) => Some(
            (*payees
                .get(split.payee.as_str())
                .ok_or(ImportError::Malformed { line: split.line })?)
            .to_owned(),
        )
        .filter(|name| !name.trim().is_empty()),
        None => None,
    };
    let mut parts = Vec::with_capacity(raw.splits.len());
    for split in &raw.splits {
        let part = part(split, currency, accounts, currencies, tags, skipped)
            .map_err(|error| error.at(split.line))?;
        parts.push(part);
    }
    let entered_on = match raw.entered.trim() {
        "" => None,
        entered => Some(date(entered).map_err(at_transaction)?),
    };
    Ok(BookTransaction {
        date: date(&raw.date).map_err(at_transaction)?,
        payee,
        currency,
        parts,
        note: text(&raw.memo),
        entered_on,
    })
}

/// One part of a transaction in `currency`.
fn part(
    split: &RawSplit,
    currency: Currency,
    accounts: &Accounts<'_>,
    currencies: &mut Currencies<'_>,
    tags: &HashSet<&str>,
    skipped: &mut Skips,
) -> Result<BookPart, ImportError> {
    let malformed = || ImportError::Malformed { line: split.line };
    let value = fraction(&split.value)?;
    let shares = fraction(&split.shares)?;
    let amount = value.money(currency, &split.value)?;
    // KMyMoney's `reconcileflag`: `1` seen on a statement, `2` reconciled,
    // as the samples show (D-102); anything else is unchecked.
    let status = match split.reconciled.trim() {
        "1" => Some(PartStatus::Cleared),
        "2" => Some(PartStatus::Reconciled),
        _ => None,
    };
    let mut part_tags: Vec<String> = Vec::new();
    for tag in &split.tags {
        if tags.contains(tag.as_str()) && !part_tags.contains(tag) {
            part_tags.push(tag.clone());
        } else {
            skipped.add(SkippedData::Tags);
        }
    }
    let account = accounts
        .by_id
        .get(split.account.as_str())
        .ok_or_else(malformed)?;
    let Some(&kind) = accounts.kinds.get(account.id.as_str()) else {
        // Booked to a top-level account itself.
        return Err(malformed());
    };
    let own = BookPart {
        account: None,
        amount,
        account_amount: None,
        details: None,
        note: text(&split.memo),
        number: text(&split.number),
        status,
        tags: part_tags,
    };
    if accounts.investments.contains(account.id.as_str()) {
        // Its money stays in the transaction, without a category.
        return Ok(own);
    }
    let account_currency = code(&account.currency, account.line)?;
    let account_amount = if account_currency == currency.code() {
        // Both amounts are in the same currency: they must agree.
        if !value.equals(shares) {
            return Err(malformed());
        }
        None
    } else {
        let account_currency = currencies.get(account_currency, account.line)?;
        Some(shares.money(account_currency, &split.shares)?)
    };
    // What a bank delivered stays with a part booked to one of the user's
    // accounts (D-083). KMyMoney keeps one text per split, whether the bank
    // delivered it or the user typed it: on a part booked to an account it
    // is the delivered text, on any other part the part's note (D-132).
    let (details, note) = if kind.is_account() {
        let details = ImportedDetails {
            bank_id: text(&split.bank_id),
            remittance_information: text(&split.memo),
            ..ImportedDetails::default()
        };
        (
            (details != ImportedDetails::default()).then_some(details),
            None,
        )
    } else {
        (None, own.note.clone())
    };
    Ok(BookPart {
        account: Some(account.id.clone()),
        account_amount,
        details,
        note,
        ..own
    })
}

/// The key words of a payee, one on each line, without the empty ones.
fn key_words(raw: &RawPayee) -> Vec<String> {
    raw.match_keys
        .lines()
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A payee of the book, with what the file keeps about it (D-132).
fn payee(raw: &RawPayee) -> BookPayee {
    let key_words = key_words(raw);
    let recognition = match (raw.matching.trim(), key_words.is_empty()) {
        ("1", _) => PayeeRecognition::ByName,
        ("2", false) => PayeeRecognition::ByKeyWords(key_words),
        _ => PayeeRecognition::Never,
    };
    BookPayee {
        key: raw.id.clone(),
        name: raw.name.clone(),
        address: raw.address.lines(),
        email: text(&raw.email),
        phone: text(&raw.address.phone),
        reference: text(&raw.reference),
        recognition,
        recognition_ignores_case: matches!(raw.match_ignores_case.trim(), "1" | "Y"),
    }
}

/// The Rules that KMyMoney's payee recognition amounts to (D-160).
///
/// KMyMoney compares the payee text of an imported entry with each payee:
/// by the payee's name, which has to appear in the text, or by a list of
/// key words, each a regular expression, of which one has to match; the
/// payee found gets its default category. A Rule does the same with the
/// other party's name, the text a bank delivers for the payee: it books to
/// the default category and sets the payee. The patterns are passed on as
/// the file has them: the pipeline checks that KassenSturz can use each one
/// (it doesn't take everything a Qt regular expression does) and reports
/// the others.
///
/// A payee gets a Rule only if it has both a way of being recognized and a
/// default category the book has. Without a category the Rule would only
/// set the payee, and since the first Rule that matches decides, it would
/// keep the user's own later Rules from booking the entry. A payee with
/// only one of the two, or a way of recognizing that this importer doesn't
/// know, is counted as left out (`SkippedData::PayeeRules`).
fn payee_rules(
    payees: &[RawPayee],
    accounts: &[BookAccount],
    skipped: &mut Skips,
) -> Vec<BookRule> {
    let usable: HashSet<&str> = accounts
        .iter()
        .filter(|account| account.kind != AccountKind::Equity)
        .map(|account| account.key.as_str())
        .collect();
    let mut rules = Vec::new();
    for raw in payees {
        let name = raw.name.trim();
        let by = |operator, value: &str| BookCondition {
            field: TextField::OtherPartyName,
            operator,
            value: value.to_owned(),
        };
        let key_words = key_words(raw);
        let (match_mode, conditions, known) = match raw.matching.trim() {
            "1" if !name.is_empty() => {
                (MatchMode::All, vec![by(TextOperator::Contains, name)], true)
            }
            "2" => (
                MatchMode::Any,
                key_words
                    .iter()
                    .map(|key| by(TextOperator::Matches, key))
                    .collect(),
                true,
            ),
            "" | "0" | "1" => (MatchMode::All, Vec::new(), true),
            _ => (MatchMode::All, Vec::new(), false),
        };
        let default = usable
            .contains(raw.default_account.as_str())
            .then(|| raw.default_account.clone());
        if name.is_empty() || conditions.is_empty() || default.is_none() {
            // Recognizing by name, which KMyMoney does for every new payee,
            // says nothing the name doesn't: without a category it is no loss.
            let by_name_only = default.is_none() && raw.matching.trim() == "1";
            let lost = match (conditions.is_empty(), default.is_some()) {
                (true, true) => true,
                (false, false) => !by_name_only,
                (true, false) => !known,
                (false, true) => false,
            };
            if lost {
                skipped.add(SkippedData::PayeeRules);
            }
            continue;
        }
        rules.push(BookRule {
            name: name.to_owned(),
            match_mode,
            conditions,
            case_sensitive: !matches!(raw.match_ignores_case.trim(), "1" | "Y"),
            book_to: default,
            payee: name.to_owned(),
        });
    }
    rules
}

/// `text` as an optional text: `None` if it is empty or only spaces.
fn text(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_owned())
}

/// The fraction `text`, which must be one.
fn fraction(text: &str) -> Result<Fraction, ImportError> {
    Fraction::parse(text).ok_or_else(|| ImportError::BadAmount {
        found: text.to_owned(),
        line: None,
    })
}

/// The currency code `text`, which must be one.
fn code(text: &str, line: Option<u64>) -> Result<CurrencyCode, ImportError> {
    CurrencyCode::new(text).map_err(|_| ImportError::Malformed { line })
}

/// The currency code `text`, if KassenSturz's ISO 4217 table lists it.
fn known_currency(text: &str) -> Option<CurrencyCode> {
    CurrencyCode::new(text)
        .ok()
        .filter(|code| code.iso_minor_digits().is_some())
}

/// A date as the file writes it, `2026-10-01`.
fn date(text: &str) -> Result<Date, ImportError> {
    let bad = || ImportError::BadDate {
        found: text.to_owned(),
        line: None,
    };
    let well_formed = text.len() == 10
        && text.bytes().enumerate().all(|(at, byte)| match at {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    if !well_formed {
        return Err(bad());
    }
    text.parse().map_err(|_| bad())
}
