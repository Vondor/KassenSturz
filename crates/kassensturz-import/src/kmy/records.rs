// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The records of a `.kmy` file as it writes them: accounts, payees,
//! transactions with their splits, prices, and how many of everything
//! KassenSturz leaves out. Turning them into a book is `mapping`'s job.
//!
//! What each element and attribute holds was learned from KMyMoney's
//! sample files (D-044, D-102); an attribute the importer doesn't need is
//! passed over, and a record the importer needs that lacks one of its
//! attributes is `import.malformed`.

use std::collections::HashSet;

use super::xml::{self, Element};
use crate::book::SkippedData;
use crate::error::ImportError;
use crate::limits::Limits;

/// The root element of every `.kmy` file.
const ROOT: &str = "KMYMONEY-FILE";

/// The only version of the file's layout the samples show, and the one
/// the importer reads (`FILEINFO`'s `VERSION`).
const VERSION: &str = "1";

/// Everything the importer takes from a `.kmy` file.
#[derive(Debug, Default)]
pub(super) struct Records {
    pub accounts: Vec<RawAccount>,
    pub payees: Vec<RawPayee>,
    pub transactions: Vec<RawTransaction>,
    pub prices: Vec<RawPrice>,
    /// The IDs of the file's securities, which investment accounts count
    /// in instead of a currency.
    pub securities: HashSet<String>,
    /// How many of each kind of data the importer leaves out, as far as
    /// the records alone tell; `mapping` adds what the book can't hold.
    pub skipped: Skips,
}

/// An `ACCOUNT`: an account, a category, or one of KMyMoney's five
/// top-level accounts that every other sits inside.
#[derive(Debug)]
pub(super) struct RawAccount {
    pub id: String,
    /// The ID of the account it sits inside, empty for a top-level one.
    pub parent: String,
    /// KMyMoney's number for its type.
    pub account_type: String,
    pub name: String,
    pub number: String,
    pub description: String,
    /// The code of its currency, or the ID of a security.
    pub currency: String,
    /// Whether it has settings for online banking.
    pub online_banking: bool,
    pub line: Option<u64>,
}

/// A `PAYEE`: who money goes to or comes from.
#[derive(Debug)]
pub(super) struct RawPayee {
    pub id: String,
    pub name: String,
    /// Whether anything besides the name was kept with it.
    pub details: bool,
}

/// A `TRANSACTION` of the file's list of transactions.
#[derive(Debug)]
pub(super) struct RawTransaction {
    /// The date it is shown with, as written.
    pub date: String,
    /// The code of the currency its values count in.
    pub currency: String,
    pub memo: String,
    pub splits: Vec<RawSplit>,
    pub line: Option<u64>,
}

/// A `SPLIT`: one part of a transaction.
#[derive(Debug)]
pub(super) struct RawSplit {
    pub account: String,
    /// Its amount in the transaction's currency, as a fraction.
    pub value: String,
    /// Its amount in the account's currency, or its number of shares, as
    /// a fraction.
    pub shares: String,
    /// The ID of its payee, or empty.
    pub payee: String,
    pub memo: String,
    pub bank_id: String,
    /// Empty or `0` if it wasn't checked against a statement.
    pub reconciled: String,
    /// A cheque number.
    pub number: String,
    pub line: Option<u64>,
}

/// A `PRICE`: what one unit of `from` cost in `to` on a day.
#[derive(Debug)]
pub(super) struct RawPrice {
    pub from: String,
    pub to: String,
    pub date: String,
    /// The price, as a fraction.
    pub price: String,
    pub line: Option<u64>,
}

/// Counts of what the importer leaves out, by kind.
#[derive(Debug, Default)]
pub(super) struct Skips(Vec<(SkippedData, usize)>);

impl Skips {
    /// Counts one more of `what`.
    pub fn add(&mut self, what: SkippedData) {
        self.add_many(what, 1);
    }

    /// Counts `many` more of `what`.
    pub fn add_many(&mut self, what: SkippedData, many: usize) {
        if many == 0 {
            return;
        }
        match self.0.iter_mut().find(|(kind, _)| *kind == what) {
            Some((_, count)) => *count = count.saturating_add(many),
            None => self.0.push((what, many)),
        }
    }

    /// The counts, in the order of [`SkippedData::ALL`].
    pub fn into_counts(self) -> Vec<(SkippedData, usize)> {
        SkippedData::ALL
            .into_iter()
            .filter_map(|what| {
                self.0
                    .iter()
                    .find(|(kind, _)| *kind == what)
                    .map(|&(_, count)| (what, count))
            })
            .collect()
    }
}

/// Reads the records of the XML of a `.kmy` file.
pub(super) fn read(xml: &[u8], limits: &Limits) -> Result<Records, ImportError> {
    let mut records = Records::default();
    let mut owner = false;
    let mut pair: Option<(String, String)> = None;
    xml::read_elements(xml, limits, |path, element| {
        let names: Vec<&str> = path.iter().map(String::as_str).collect();
        let malformed = || ImportError::Malformed {
            line: Some(element.line),
        };
        let name = element.name.as_str();
        match names.as_slice() {
            [] if name != ROOT => return Err(malformed()),
            [] => {}
            [ROOT] => {
                section(&mut records.skipped, name);
                // The owner's details are cleared in the samples (D-102);
                // a file whose owner gave them loses them.
                if name == "USER" {
                    owner |= any_text(element);
                }
            }
            [ROOT, "FILEINFO"] if name == "VERSION" => {
                if element.get("id") != Some(VERSION) {
                    return Err(malformed());
                }
            }
            [ROOT, "USER", ..] => owner |= any_text(element),
            [ROOT, "INSTITUTIONS"] => records.skipped.add(SkippedData::Institutions),
            [ROOT, "PAYEES"] if name == "PAYEE" => records.payees.push(RawPayee {
                id: required(element, "id")?.to_owned(),
                name: required(element, "name")?.to_owned(),
                details: ["reference", "email", "matchkey"]
                    .iter()
                    .any(|key| element.get(key).is_some_and(|value| !value.is_empty())),
            }),
            [ROOT, "PAYEES", "PAYEE", ..] => {
                if any_text(element)
                    && let Some(payee) = records.payees.last_mut()
                {
                    payee.details = true;
                }
            }
            [ROOT, "TAGS"] => records.skipped.add(SkippedData::Tags),
            [ROOT, "COSTCENTERS"] => records.skipped.add(SkippedData::Other),
            [ROOT, "ACCOUNTS"] if name == "ACCOUNT" => records.accounts.push(RawAccount {
                id: required(element, "id")?.to_owned(),
                parent: required(element, "parentaccount")?.to_owned(),
                account_type: required(element, "type")?.to_owned(),
                name: required(element, "name")?.to_owned(),
                number: element.get("number").unwrap_or_default().to_owned(),
                description: element.get("description").unwrap_or_default().to_owned(),
                currency: required(element, "currency")?.to_owned(),
                online_banking: false,
                line: Some(element.line),
            }),
            [ROOT, "ACCOUNTS", "ACCOUNT"] if name == "ONLINEBANKING" => {
                let account = records.accounts.last_mut().ok_or_else(malformed)?;
                account.online_banking = true;
            }
            [ROOT, "TRANSACTIONS"] if name == "TRANSACTION" => {
                records.transactions.push(RawTransaction {
                    date: required(element, "postdate")?.to_owned(),
                    currency: required(element, "commodity")?.to_owned(),
                    memo: element.get("memo").unwrap_or_default().to_owned(),
                    splits: Vec::new(),
                    line: Some(element.line),
                });
            }
            [ROOT, "TRANSACTIONS", "TRANSACTION", "SPLITS"] if name == "SPLIT" => {
                let transaction = records.transactions.last_mut().ok_or_else(malformed)?;
                transaction.splits.push(RawSplit {
                    account: required(element, "account")?.to_owned(),
                    value: required(element, "value")?.to_owned(),
                    shares: required(element, "shares")?.to_owned(),
                    payee: element.get("payee").unwrap_or_default().to_owned(),
                    memo: element.get("memo").unwrap_or_default().to_owned(),
                    bank_id: element.get("bankid").unwrap_or_default().to_owned(),
                    reconciled: element.get("reconcileflag").unwrap_or_default().to_owned(),
                    number: element.get("number").unwrap_or_default().to_owned(),
                    line: Some(element.line),
                });
            }
            // Each entry of these lists is one of what it holds; what is
            // inside an entry doesn't matter, since it is left out whole.
            [ROOT, "SCHEDULES"] => records.skipped.add(SkippedData::Schedules),
            [ROOT, "BUDGETS"] => records.skipped.add(SkippedData::Budgets),
            [ROOT, "REPORTS"] => records.skipped.add(SkippedData::Reports),
            [ROOT, "ONLINEJOBS"] => records.skipped.add(SkippedData::OnlineBanking),
            [ROOT, "SECURITIES"] => {
                records
                    .securities
                    .insert(required(element, "id")?.to_owned());
            }
            [ROOT, "PRICES"] if name == "PRICEPAIR" => {
                pair = Some((
                    required(element, "from")?.to_owned(),
                    required(element, "to")?.to_owned(),
                ));
            }
            [ROOT, "PRICES", "PRICEPAIR"] if name == "PRICE" => {
                let (from, to) = pair.clone().ok_or_else(malformed)?;
                records.prices.push(RawPrice {
                    from,
                    to,
                    date: required(element, "date")?.to_owned(),
                    price: required(element, "price")?.to_owned(),
                    line: Some(element.line),
                });
            }
            // Everything else is either KMyMoney's own bookkeeping, such as
            // its list of currencies, the file's dates and its settings,
            // or repeats what the records above say, such as the lists of
            // the accounts inside an account.
            _ => {}
        }
        Ok(())
    })?;
    if owner {
        records.skipped.add(SkippedData::Owner);
    }
    Ok(records)
}

/// Counts a section of the file KassenSturz doesn't know at all.
fn section(skipped: &mut Skips, name: &str) {
    const KNOWN: [&str; 16] = [
        "FILEINFO",
        "USER",
        "INSTITUTIONS",
        "PAYEES",
        "COSTCENTERS",
        "TAGS",
        "ACCOUNTS",
        "TRANSACTIONS",
        "KEYVALUEPAIRS",
        "SCHEDULES",
        "SECURITIES",
        "CURRENCIES",
        "PRICES",
        "REPORTS",
        "BUDGETS",
        "ONLINEJOBS",
    ];
    if !KNOWN.contains(&name) {
        skipped.add(SkippedData::Other);
    }
}

/// The attribute `name` of `element`, which it must have.
fn required<'a>(element: &'a Element, name: &str) -> Result<&'a str, ImportError> {
    element.get(name).ok_or(ImportError::Malformed {
        line: Some(element.line),
    })
}

/// Whether any attribute of `element` holds text.
fn any_text(element: &Element) -> bool {
    element
        .attributes
        .iter()
        .any(|(_, value)| !value.trim().is_empty())
}
