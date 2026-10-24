// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Another program's book, such as KMyMoney's, into a new KassenSturz book
//! (M2.9): turned into the data of a complete export and filled in through
//! [`fill_book`], so it passes every check a book from a file passes.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    Account, AccountId, AccountKind, Actions, BookTo, Changeover, Condition, ConvertedPart,
    Currency, CurrencyCode, DatedRate, DatedRateId, ForeignAmount, Institution, InstitutionId,
    MatchMode, Money, MoneyError, OriginalAmount, Payee, PayeeId, RateSource, RoundingMode, Rule,
    RuleId, RuleNode, Split, Tag, TagId, Transaction, TransactionId, TransferMatch, Withdrawn,
};
use kassensturz_export::BookData;
use kassensturz_import::{
    BookAccount, BookCondition, BookInstitution, BookPart, BookPayee, BookRule, BookTag,
    BookTransaction, ImportError, ImportedBook, Replacement, Skipped, SkippedData, currency,
};
use kassensturz_store::{Book, StoreError};

use crate::book::fill_book;
use crate::error::PipelineError;

/// What [`fill_book_from`] put into the new book, and what it couldn't.
#[derive(Debug)]
pub struct BookImportReport {
    /// How many of the user's accounts the book has now.
    pub accounts: usize,
    /// How many categories.
    pub categories: usize,
    /// How many transactions.
    pub transactions: usize,
    /// How many exchange rates.
    pub rates: usize,
    /// How many transactions the file kept with only their part in one of
    /// the user's accounts, such as an entry the other program imported
    /// from a bank that nobody gave a category yet (D-101). Each got a part
    /// without a category for the other side, as an imported entry does
    /// (D-083); nothing is wrong with them.
    pub uncategorized: usize,
    /// How many other transactions got a part without a category for what
    /// their parts didn't add up to (D-083): the file didn't balance them,
    /// so the user may want to look at them.
    pub completed: usize,
    /// Accounts and categories whose name another one had already, ignoring
    /// capitals, and that got a number after it, e.g. "Savings (2)": their
    /// new IDs and their names in the file.
    pub renamed: Vec<(AccountId, String)>,
    /// Categories that sat inside one in another currency, which a book
    /// can't add up (D-082), and are top-level categories now.
    pub moved_to_top: Vec<AccountId>,
    /// Accounts the other program had closed although money was still in
    /// them, which stay open, so no money is hidden (D-081).
    pub left_open: Vec<AccountId>,
    /// The withdrawn currencies the file used, such as the German mark,
    /// whose amounts are in the currencies that replaced them now (D-104).
    pub converted: Vec<ConvertedCurrency>,
    /// The Transfers to suggest between the new book's accounts (D-087):
    /// two transactions without a category yet that look like the two
    /// sides of one. Nothing is joined: the user confirms each one.
    pub transfers: Vec<TransferMatch>,
    /// How many Rules the file's payee recognition became (D-160).
    pub rules: usize,
    /// Matching texts of those Rules that KassenSturz can't use, such as
    /// patterns written for another program's regular expressions (D-160).
    pub patterns_left_out: Vec<PatternLeftOut>,
    /// What the file held that the book can't, as the importer counted it
    /// (D-029).
    pub skipped: Vec<Skipped>,
}

/// A matching text of a payee that KassenSturz can't use (D-160).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternLeftOut {
    /// The payee, whose name the Rule has.
    pub payee: String,
    /// The pattern, as the file has it.
    pub pattern: String,
    /// Whether the payee still got a Rule, with its other patterns. If not,
    /// none of its patterns could be used.
    pub rule_made: bool,
}

/// A withdrawn currency an imported book used, and what was converted out of
/// it (D-104).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvertedCurrency {
    /// The withdrawn currency, e.g. the German mark, with its fixed rate.
    pub withdrawn: Withdrawn,
    /// How many accounts and categories were in it and are in its successor
    /// now.
    pub accounts: usize,
    /// How many parts were converted out of it, each keeping the amount it
    /// had as its original amount.
    pub parts: usize,
}

/// Fills the new, empty `book` with `imported`, a book another program
/// kept, and reports what it did.
///
/// Everything gets a new ID. Accounts and categories keep their names,
/// with a number after one that is taken already (`renamed`); a
/// transaction whose parts don't add up gets a part without a category for
/// the difference (`completed`), and one the file kept with only its part
/// in an account gets one for the other side (`uncategorized`). The
/// decimal places of each currency are
/// the ones the importer read the amounts in, so nothing is rescaled
/// (D-027). Amounts in a withdrawn currency, such as the German mark, are
/// converted into the one that replaced it at the fixed rate, and each
/// converted part keeps the amount it had (`converted`, D-077, D-104):
/// the accounts and categories are in the successor, and each one's
/// converted balance is its old balance converted and rounded once. The
/// book's settings stay the ones it was created with, and, as for a
/// complete export, nothing is recorded in its change history
/// ([`fill_book`]): undoing the import would only leave an empty book.
///
/// A currency neither of KassenSturz's tables lists
/// ([`ImportedBook::unlisted`]) is converted the same way, into what
/// `replaced` says replaced it, at that rate (D-106): the user says so, a
/// file's own rate is only a suggestion. Without it, or if that isn't a
/// current currency, nothing is imported (`import.successor-needed`,
/// `import.successor-not-current`). What `replaced` says about other
/// currencies is passed over: KassenSturz's table counts for those.
///
/// If anything is refused, the book stays empty, so the front end can
/// remove it again.
pub fn fill_book_from(
    book: &mut Book,
    imported: &ImportedBook,
    replaced: &HashMap<CurrencyCode, Replacement>,
) -> Result<BookImportReport, PipelineError> {
    fill_book_from_with(book, imported, replaced, RuleScope::default())
}

/// Which payees of an imported book get a Rule (D-160).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RuleScope {
    /// Every payee with a default category: where the file has a matching
    /// text for it, the Rule looks for that text or the payee's name;
    /// otherwise for the name only, as the other party and as the payee of
    /// a transaction. What most people want.
    #[default]
    AlsoByName,
    /// Only payees whose file has a matching text and a default category,
    /// each Rule looking for that text. The others are counted as left out.
    WithMatchingText,
}

/// [`fill_book_from`] with the choice of which payees get a Rule (D-160).
pub fn fill_book_from_with(
    book: &mut Book,
    imported: &ImportedBook,
    replaced: &HashMap<CurrencyCode, Replacement>,
    scope: RuleScope,
) -> Result<BookImportReport, PipelineError> {
    let rounding = book.rounding_mode()?;
    let mut mapper = Mapper {
        replaced: replaced_currencies(imported, replaced)?,
        ..Mapper::default()
    };
    let institutions = mapper.institutions(&imported.institutions)?;
    let tags = mapper.tags(&imported.tags)?;
    let payees = Mapper::payees(&imported.payees);
    let mut accounts = mapper.accounts(&imported.accounts)?;
    let (rules, patterns_left_out, without_text) = mapper.rules(&imported.rules, scope)?;
    let mut changeover = Changeover::new(rounding);
    let mut uncategorized = 0;
    let mut completed = 0;
    // In date order, so each account's balance in a withdrawn currency
    // grows as it did in the other program (D-104); the book gets them in
    // the file's order.
    let mut order: Vec<usize> = (0..imported.transactions.len()).collect();
    order.sort_by_key(|&index| imported.transactions[index].date);
    let mut transactions: Vec<Option<Transaction>> = vec![None; imported.transactions.len()];
    for index in order {
        let transaction = &imported.transactions[index];
        let (splits, added) = mapper.splits(transaction, &mut changeover, rounding)?;
        match added {
            Some(Added::OtherSide) => uncategorized += 1,
            Some(Added::Difference) => completed += 1,
            None => {}
        }
        let transaction = Transaction::new(
            TransactionId::generate(),
            transaction.date,
            transaction.payee.clone(),
            splits,
        )
        // The importer's checks (`kassensturz_import::import`) make sure of
        // what `Transaction::new` asks: one currency, an account for every
        // amount in another one, the same sign for both of a part's
        // amounts. A book handed over without them is refused here.
        .map_err(|_| ImportError::Malformed { line: None })?
        .with_note(transaction.note.clone())
        .with_entry_date(transaction.entered_on);
        transactions[index] = Some(transaction);
    }
    let transactions: Vec<Transaction> = transactions.into_iter().flatten().collect();
    let mut rates = Vec::with_capacity(imported.rates.len());
    for rate in &imported.rates {
        for code in [rate.from, rate.to] {
            mapper.uses_code(code)?;
        }
        rates.push(DatedRate {
            id: DatedRateId::generate(),
            date: rate.date,
            from: rate.from,
            to: rate.to,
            rate: rate.rate,
            // Entered in the other program, the only way a rate gets into
            // a book in v1 (D-028).
            source: RateSource::Manual,
        });
    }
    let left_open = Mapper::keep_open_with_money(&mut accounts, &transactions);
    let data = BookData {
        rounding_mode: book.rounding_mode()?,
        history_keep_days: match book.history_retention()? {
            kassensturz_store::HistoryRetention::Days(days) => Some(days),
            kassensturz_store::HistoryRetention::Forever => None,
        },
        currencies: mapper.currencies,
        accounts,
        rates,
        rules,
        transactions,
        rule_marks: Vec::new(),
        institutions,
        payees,
        tags,
        bank_links: Vec::new(),
    };
    fill_book(book, &data)?;
    let count = |kind: fn(AccountKind) -> bool| {
        data.accounts
            .iter()
            .filter(|account| kind(account.kind))
            .count()
    };
    Ok(BookImportReport {
        accounts: count(AccountKind::is_account),
        categories: count(AccountKind::is_category),
        transactions: data.transactions.len(),
        rates: data.rates.len(),
        uncategorized,
        completed,
        renamed: mapper.renamed,
        moved_to_top: mapper.moved_to_top,
        left_open,
        converted: mapper.converted,
        transfers: book.transfer_suggestions()?,
        rules: data.rules.len(),
        patterns_left_out,
        skipped: with_counted(&imported.skipped, SkippedData::PayeeRules, without_text),
    })
}

/// The currencies of `imported` that neither of KassenSturz's tables lists,
/// as withdrawn ones that `replaced` says replaced them (D-106).
fn replaced_currencies(
    imported: &ImportedBook,
    replaced: &HashMap<CurrencyCode, Replacement>,
) -> Result<HashMap<CurrencyCode, Withdrawn>, ImportError> {
    let mut withdrawn = HashMap::with_capacity(imported.unlisted.len());
    for unlisted in &imported.unlisted {
        let currency = unlisted.currency;
        let replacement = replaced
            .get(&currency.code())
            .ok_or(ImportError::SuccessorNeeded {
                currency: currency.code(),
            })?;
        let not_current = ImportError::SuccessorNotCurrent {
            currency: currency.code(),
            successor: replacement.successor,
        };
        // A current currency is one an account can be in, which no
        // withdrawn one is.
        if replacement.successor.iso_minor_digits().is_none() {
            return Err(not_current);
        }
        let replaced = Withdrawn::new(
            currency,
            replacement.successor,
            replacement.rate,
            replacement.date,
        )
        .ok_or(not_current)?;
        withdrawn.insert(currency.code(), replaced);
    }
    Ok(withdrawn)
}

/// Why a transaction got a part without a category (D-083).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Added {
    /// The file kept only its part in one of the user's accounts, such as
    /// an entry imported from a bank: the part is the other side, with no
    /// category yet.
    OtherSide,
    /// Its parts didn't add up: the part is the difference.
    Difference,
}

/// Turns the file's keys and names into a book's IDs and unique names, and
/// its withdrawn currencies into their successors.
#[derive(Default)]
struct Mapper {
    /// The withdrawn currencies neither of KassenSturz's tables lists, with
    /// what the user said replaced them (D-106).
    replaced: HashMap<CurrencyCode, Withdrawn>,
    ids: HashMap<String, AccountId>,
    institutions: HashMap<String, InstitutionId>,
    tags: HashMap<String, TagId>,
    /// Whether the account with the key is one of the user's accounts, as
    /// opposed to a category or an internal account.
    user_accounts: HashMap<String, bool>,
    currencies: Vec<Currency>,
    renamed: Vec<(AccountId, String)>,
    moved_to_top: Vec<AccountId>,
    converted: Vec<ConvertedCurrency>,
}

impl Mapper {
    /// The ID the account with `key` gets.
    fn id(&self, key: &str) -> Result<AccountId, ImportError> {
        self.ids
            .get(key)
            .copied()
            .ok_or(ImportError::Malformed { line: None })
    }

    /// Notes that the book uses `currency` with its decimal places. Fails
    /// if the same code came with others before.
    fn uses(&mut self, currency: Currency) -> Result<(), ImportError> {
        match self
            .currencies
            .iter()
            .find(|known| known.code() == currency.code())
        {
            Some(known) if known.minor_digits() != currency.minor_digits() => {
                Err(ImportError::DecimalPlacesDiffer {
                    currency: currency.code(),
                    book: known.minor_digits(),
                    file: currency.minor_digits(),
                })
            }
            Some(_) => Ok(()),
            None => {
                self.currencies.push(currency);
                Ok(())
            }
        }
    }

    /// The withdrawn currency `code` is, from KassenSturz's table or as the
    /// user said (D-104, D-106); `None` for a current one. Fails for one
    /// that neither table lists if the user didn't say what replaced it.
    fn withdrawn(&self, code: CurrencyCode) -> Result<Option<Withdrawn>, ImportError> {
        if let Some(withdrawn) = code
            .withdrawn()
            .or_else(|| self.replaced.get(&code).copied())
        {
            return Ok(Some(withdrawn));
        }
        if code.iso_minor_digits().is_none() {
            return Err(ImportError::SuccessorNeeded { currency: code });
        }
        Ok(None)
    }

    /// The current currency `currency` is in the book: its successor, with
    /// the decimal places KassenSturz's ISO 4217 table gives it, if it was
    /// withdrawn (D-104), otherwise itself. Notes that the book uses it.
    fn current(&mut self, currency: Currency) -> Result<Currency, ImportError> {
        let current = match self.withdrawn(currency.code())? {
            Some(withdrawn) => self.known_or_listed(withdrawn.successor())?,
            None => currency,
        };
        self.uses(current)?;
        Ok(current)
    }

    /// `code` with the decimal places the book uses for it already, or else
    /// those KassenSturz's ISO 4217 table gives it.
    fn known_or_listed(&self, code: CurrencyCode) -> Result<Currency, ImportError> {
        match self.currencies.iter().find(|known| known.code() == code) {
            Some(&known) => Ok(known),
            None => currency(code),
        }
    }

    /// Notes that `count` accounts were in `withdrawn` and `parts` parts were
    /// converted out of it, which keep their amounts in it: the book lists
    /// it then, with its decimal places.
    fn note_converted(
        &mut self,
        withdrawn: Withdrawn,
        accounts: usize,
        parts: usize,
    ) -> Result<(), ImportError> {
        if parts > 0 {
            self.uses(withdrawn.currency())?;
        }
        match self
            .converted
            .iter_mut()
            .find(|converted| converted.withdrawn == withdrawn)
        {
            Some(converted) => {
                converted.accounts += accounts;
                converted.parts += parts;
            }
            None => self.converted.push(ConvertedCurrency {
                withdrawn,
                accounts,
                parts,
            }),
        }
        Ok(())
    }

    /// Notes that the book uses the currency `code`, which no amount gave
    /// decimal places yet, e.g. one only rates are in: KassenSturz's ISO
    /// 4217 table gives them (D-027).
    fn uses_code(&mut self, code: CurrencyCode) -> Result<(), ImportError> {
        if self.currencies.iter().any(|known| known.code() == code) {
            return Ok(());
        }
        self.uses(currency(code)?)
    }

    /// The book's accounts and categories, with new IDs, names that are
    /// unique where the book needs them to be (D-081, D-082), and every
    /// category inside one of its own currency.
    /// The banks, with new IDs. Two with the same name, ignoring capitals,
    /// stay two banks; the book allows that.
    fn institutions(
        &mut self,
        imported: &[BookInstitution],
    ) -> Result<Vec<Institution>, PipelineError> {
        let mut institutions = Vec::with_capacity(imported.len());
        for institution in imported {
            let id = InstitutionId::generate();
            if self
                .institutions
                .insert(institution.key.clone(), id)
                .is_some()
            {
                return Err(ImportError::Malformed { line: None }.into());
            }
            institutions.push(Institution {
                id,
                name: institution.name.clone(),
                bank_code: institution.bank_code.clone(),
                contact: institution.contact.clone(),
                address: institution.address.clone(),
                phone: institution.phone.clone(),
            });
        }
        Ok(institutions)
    }

    /// The tags, with new IDs and names that differ, as the book asks:
    /// a second "Kids" becomes "Kids (2)".
    fn tags(&mut self, imported: &[BookTag]) -> Result<Vec<Tag>, PipelineError> {
        let mut taken = HashSet::new();
        let mut tags = Vec::with_capacity(imported.len());
        for tag in imported {
            let id = TagId::generate();
            if self.tags.insert(tag.key.clone(), id).is_some() {
                return Err(ImportError::Malformed { line: None }.into());
            }
            tags.push(Tag {
                id,
                name: unique_name(&tag.name, &mut taken),
            });
        }
        Ok(tags)
    }

    /// The payees with their details, with new IDs and names that differ,
    /// as the book asks. A transaction names its payee by name, so no ID
    /// of theirs needs mapping.
    fn payees(imported: &[BookPayee]) -> Vec<Payee> {
        let mut taken = HashSet::new();
        imported
            .iter()
            .map(|payee| Payee {
                id: PayeeId::generate(),
                name: unique_name(&payee.name, &mut taken),
                address: payee.address.clone(),
                email: payee.email.clone(),
                phone: payee.phone.clone(),
                reference: payee.reference.clone(),
                recognition: payee.recognition.clone(),
                recognition_ignores_case: payee.recognition_ignores_case,
            })
            .collect()
    }

    /// The Rules the file asks for, with new IDs and names that differ, as
    /// the book asks, and how many payees `scope` left without one. A
    /// pattern KassenSturz can't use is left out of its Rule and reported;
    /// a Rule with no usable condition left isn't made.
    fn rules(
        &self,
        imported: &[BookRule],
        scope: RuleScope,
    ) -> Result<(Vec<RuleNode>, Vec<PatternLeftOut>, usize), PipelineError> {
        let mut taken = HashSet::new();
        let mut rules = Vec::with_capacity(imported.len());
        let mut left_out = Vec::new();
        let mut without_text = 0;
        for wanted in imported {
            let usable = |given: &BookCondition| {
                let condition = Condition::Text {
                    field: given.field,
                    operator: given.operator,
                    value: given.value.clone(),
                    case_sensitive: wanted.case_sensitive,
                };
                alone(&condition, wanted)
                    .check()
                    .is_ok()
                    .then_some(condition)
            };
            if scope == RuleScope::WithMatchingText && wanted.conditions.is_empty() {
                without_text += 1;
                continue;
            }
            let mut conditions = Vec::new();
            let mut unusable = Vec::new();
            for given in &wanted.conditions {
                match usable(given) {
                    Some(condition) => conditions.push(condition),
                    None => unusable.push(given.value.clone()),
                }
            }
            let mut match_mode = wanted.match_mode;
            if scope == RuleScope::AlsoByName {
                // Either the file's texts or the name: one is enough.
                conditions.extend(wanted.name_conditions.iter().filter_map(usable));
                match_mode = MatchMode::Any;
            }
            for pattern in unusable {
                left_out.push(PatternLeftOut {
                    payee: wanted.payee.clone(),
                    pattern,
                    rule_made: !conditions.is_empty(),
                });
            }
            if conditions.is_empty() {
                continue;
            }
            let id = self.id(&wanted.book_to)?;
            let book_to = if self.user_accounts.get(&wanted.book_to) == Some(&true) {
                BookTo::Transfer(id)
            } else {
                BookTo::Category(id)
            };
            rules.push(RuleNode {
                rule: Rule {
                    id: RuleId::generate(),
                    name: unique_name(&wanted.name, &mut taken),
                    match_mode,
                    conditions,
                    actions: Actions {
                        book_to: Some(book_to),
                        payee: Some(wanted.payee.clone()),
                    },
                },
                sub_rules: Vec::new(),
            });
        }
        Ok((rules, left_out, without_text))
    }

    fn accounts(&mut self, imported: &[BookAccount]) -> Result<Vec<Account>, PipelineError> {
        let mut current = HashMap::with_capacity(imported.len());
        for account in imported {
            if self.ids.contains_key(&account.key) {
                return Err(ImportError::Malformed { line: None }.into());
            }
            self.ids.insert(account.key.clone(), AccountId::generate());
            self.user_accounts
                .insert(account.key.clone(), account.kind.is_account());
            current.insert(account.key.as_str(), self.current(account.currency)?);
            if let Some(withdrawn) = self.withdrawn(account.currency.code())? {
                self.note_converted(withdrawn, 1, 0)?;
            }
        }
        let by_key: HashMap<&str, &BookAccount> = imported
            .iter()
            .map(|account| (account.key.as_str(), account))
            .collect();
        let mut accounts = Vec::with_capacity(imported.len());
        // Names in use, per list of names that must differ: the user's
        // accounts share one, categories one per kind and parent.
        let mut taken: HashSet<(Option<AccountKind>, Option<AccountId>, String)> = HashSet::new();
        for account in imported {
            let id = self.id(&account.key)?;
            let parent = match &account.parent {
                Some(key) => {
                    let parent = by_key
                        .get(key.as_str())
                        .ok_or(ImportError::Malformed { line: None })?;
                    // Compared in the book's currencies: a category in marks
                    // inside one in euros fits once both are in euros. An
                    // account grouped under another one may be in any
                    // currency, since the group doesn't add up (D-132).
                    let currency = |key: &str| current[key].code();
                    if account.kind.is_account() || currency(&parent.key) == currency(&account.key)
                    {
                        Some(self.id(key)?)
                    } else {
                        self.moved_to_top.push(id);
                        None
                    }
                }
                None => None,
            };
            let list = ((!account.kind.is_account()).then_some(account.kind), parent);
            let wanted = account.name.trim();
            let mut name = wanted.to_owned();
            let mut number = 1;
            while !taken.insert((list.0, list.1, name.to_lowercase())) {
                number += 1;
                name = format!("{wanted} ({number})");
            }
            if number > 1 {
                self.renamed.push((id, account.name.clone()));
            }
            accounts.push(Account {
                id,
                name,
                kind: account.kind,
                currency: current[account.key.as_str()].code(),
                account_type: account.account_type,
                parent,
                // Only the user's accounts can be closed (D-081).
                closed: account.closed && account.kind.is_account(),
                // Only the user's accounts have a number and a bank; the
                // importer's checks leave a category's empty.
                number: account.number.clone(),
                notes: account.notes.clone(),
                institution: account
                    .institution
                    .as_deref()
                    .map(|key| {
                        self.institutions
                            .get(key)
                            .copied()
                            .ok_or(ImportError::Malformed { line: None })
                    })
                    .transpose()?,
            });
        }
        Ok(accounts)
    }

    /// The parts of `transaction` in the book, converted out of withdrawn
    /// currencies (D-104), and why a part without a category was added for
    /// what they didn't add up to, if one was (D-083).
    fn splits(
        &mut self,
        transaction: &BookTransaction,
        changeover: &mut Changeover,
        rounding: RoundingMode,
    ) -> Result<(Vec<Split>, Option<Added>), PipelineError> {
        let file_currency = transaction.currency;
        let withdrawn = self.withdrawn(file_currency.code())?;
        let currency = self.current(file_currency)?;
        let mut splits = Vec::with_capacity(transaction.parts.len() + 1);
        let mut parts = Vec::with_capacity(transaction.parts.len());
        for part in &transaction.parts {
            let (split, converted) = self.split(part, withdrawn, currency, changeover, rounding)?;
            splits.push(split);
            parts.push(converted);
        }
        let file_total = Money::checked_sum(
            file_currency.code(),
            transaction.parts.iter().map(|part| part.amount),
        )
        .map_err(money_error)?;
        // Each part was converted on its own; where the file's parts added
        // up, the converted ones do too.
        if withdrawn.is_some() && file_total.is_zero() {
            changeover
                .balance(currency.code(), &mut parts)
                .map_err(money_error)?;
            for (split, part) in splits.iter_mut().zip(&parts) {
                split.amount = part.amount;
            }
        }
        let total = Money::checked_sum(currency.code(), splits.iter().map(|split| split.amount))
            .map_err(money_error)?;
        if total.is_zero() && splits.len() >= 2 {
            return Ok((splits, None));
        }
        splits.push(Split::uncategorized(
            total.checked_neg().map_err(money_error)?,
        ));
        let added = match transaction.parts.as_slice() {
            [only] if self.is_user_account(only.account.as_deref()) => Added::OtherSide,
            _ => Added::Difference,
        };
        Ok((splits, Some(added)))
    }

    /// Whether `key` is one of the user's accounts, as opposed to a
    /// category, an internal account or nothing.
    fn is_user_account(&self, key: Option<&str>) -> bool {
        key.and_then(|key| self.user_accounts.get(key)).copied() == Some(true)
    }

    /// One part of a transaction that was in `file_currency`'s
    /// `withdrawn` currency, if it was in one, and is in `currency` now;
    /// and the part as [`Changeover::balance`] sees it.
    fn split(
        &mut self,
        part: &BookPart,
        withdrawn: Option<Withdrawn>,
        currency: Currency,
        changeover: &mut Changeover,
        rounding: RoundingMode,
    ) -> Result<(Split, ConvertedPart), PipelineError> {
        let account = match &part.account {
            Some(key) => Some((self.id(key)?, self.user_accounts[key.as_str()])),
            None => None,
        };
        let tags = part
            .tags
            .iter()
            .map(|key| {
                self.tags
                    .get(key)
                    .copied()
                    .ok_or(ImportError::Malformed { line: None })
            })
            .collect::<Result<Vec<TagId>, ImportError>>()?;
        let mut split = Split {
            account: account.map(|(id, _)| id),
            foreign: part
                .account_amount
                .map(|amount| ForeignAmount { amount, rate: None }),
            imported: part.details.clone(),
            note: part.note.clone(),
            number: part.number.clone(),
            status: part.status,
            tags,
            ..Split::uncategorized(part.amount)
        };
        // An amount given in the transaction's currency stays as it is.
        let mut converted = ConvertedPart {
            amount: part.amount,
            original: None,
            account: None,
            to_user_account: false,
        };
        // What the part booked to its account, in the account's currency.
        let booked = part.account_amount.unwrap_or(part.amount);
        let account_withdrawn = match account {
            Some(_) => self.withdrawn(booked.currency())?,
            None => None,
        };
        if let (Some((id, user)), Some(account_withdrawn)) = (account, account_withdrawn) {
            // Its account is in the successor now, and the account's balance
            // follows its balance in the withdrawn currency.
            let account_currency = self.current(account_withdrawn.currency())?;
            split.original = Some(OriginalAmount {
                amount: booked,
                rate: account_withdrawn.dated_rate(DatedRateId::generate()),
            });
            self.note_converted(account_withdrawn, 0, 1)?;
            if account_currency.code() == currency.code() {
                split.foreign = None;
                if withdrawn.is_some() {
                    split.amount = changeover
                        .book(id, booked, account_withdrawn, account_currency)
                        .map_err(money_error)?;
                    converted = ConvertedPart {
                        amount: split.amount,
                        original: Some(booked),
                        account: Some(id),
                        to_user_account: user,
                    };
                } else {
                    // The file gave its amount in the successor.
                    changeover
                        .book_given(id, booked, part.amount)
                        .map_err(money_error)?;
                }
            } else {
                split.foreign = Some(ForeignAmount {
                    amount: changeover
                        .book(id, booked, account_withdrawn, account_currency)
                        .map_err(money_error)?,
                    rate: None,
                });
                if let Some(withdrawn) = withdrawn {
                    split.amount = withdrawn
                        .convert(part.amount, currency, rounding)
                        .map_err(money_error)?;
                    converted.amount = split.amount;
                    converted.original = Some(part.amount);
                }
            }
        } else if let Some(withdrawn) = withdrawn {
            // The transaction was in a withdrawn currency, the account
            // isn't: the part keeps its amount in the transaction's.
            split.original = Some(OriginalAmount {
                amount: part.amount,
                rate: withdrawn.dated_rate(DatedRateId::generate()),
            });
            self.note_converted(withdrawn, 0, 1)?;
            match split.foreign {
                // Its account is in the successor: the file gave the amount.
                Some(foreign) if foreign.amount.currency() == currency.code() => {
                    split.amount = foreign.amount;
                    split.foreign = None;
                    converted.amount = split.amount;
                }
                _ => {
                    split.amount = withdrawn
                        .convert(part.amount, currency, rounding)
                        .map_err(money_error)?;
                    converted.amount = split.amount;
                    converted.original = Some(part.amount);
                }
            }
        }
        Ok((split, converted))
    }

    /// Opens again the accounts the file says are closed whose balance
    /// isn't zero, since a book closes only those (D-081), and returns
    /// them: no money is hidden.
    fn keep_open_with_money(
        accounts: &mut [Account],
        transactions: &[Transaction],
    ) -> Vec<AccountId> {
        let mut balances: HashMap<AccountId, i128> = HashMap::new();
        for split in transactions.iter().flat_map(Transaction::splits) {
            if let Some(account) = split.account {
                *balances.entry(account).or_default() +=
                    i128::from(split.account_amount().minor_units());
            }
        }
        let mut left_open = Vec::new();
        for account in accounts.iter_mut().filter(|account| account.closed) {
            if balances
                .get(&account.id)
                .is_some_and(|&balance| balance != 0)
            {
                account.closed = false;
                left_open.push(account.id);
            }
        }
        left_open
    }
}

/// A calculation the conversion out of a withdrawn currency couldn't do:
/// amounts too large for 64 bits, or, in a book handed over without the
/// importer's checks, an account's amounts in two currencies.
/// `wanted`, trimmed, or with a number after it if `taken` holds it
/// already, ignoring capitals: the names a book asks to differ do.
fn unique_name(wanted: &str, taken: &mut HashSet<String>) -> String {
    let wanted = wanted.trim();
    let mut name = wanted.to_owned();
    let mut number = 1;
    while !taken.insert(name.to_lowercase()) {
        number += 1;
        name = format!("{wanted} ({number})");
    }
    name
}

/// `skipped` with `more` added to the count of `what`, in the order of
/// [`SkippedData::ALL`].
fn with_counted(skipped: &[Skipped], what: SkippedData, more: usize) -> Vec<Skipped> {
    let mut counts: Vec<Skipped> = skipped.to_vec();
    if more > 0 {
        match counts.iter_mut().find(|counted| counted.what == what) {
            Some(counted) => counted.count = counted.count.saturating_add(more),
            None => counts.push(Skipped { what, count: more }),
        }
        counts.sort_by_key(|counted| counted.what);
    }
    counts
}

/// `condition` as a Rule of its own, to check it without the others.
fn alone(condition: &Condition, wanted: &BookRule) -> Rule {
    Rule {
        id: RuleId::generate(),
        name: wanted.name.clone(),
        match_mode: wanted.match_mode,
        conditions: vec![condition.clone()],
        actions: Actions::default(),
    }
}

fn money_error(error: MoneyError) -> PipelineError {
    match error {
        MoneyError::CurrencyMismatch { .. } => ImportError::Malformed { line: None }.into(),
        _ => StoreError::AmountOverflow.into(),
    }
}
