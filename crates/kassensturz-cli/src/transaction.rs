// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands for transactions.

use std::io::Write;

use kassensturz_core::{
    Account, ForeignAmount, Money, RateSource, Split, SplitId, Transaction, TransactionId,
};
use kassensturz_store::Book;

use crate::amount::{money_text, parse_amount};
use crate::args::Arguments;
use crate::error::CliError;
use crate::input::parse_date;
use crate::lookup::{find_account, find_any, find_category, find_transaction, short_id};
use crate::text::{plain, table};

/// `transaction add|show|remove`.
pub(crate) fn transaction(
    book: &mut Book,
    args: &[String],
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::usage(
            "transaction needs a command: add, show or remove",
        ));
    };
    match command.as_str() {
        "add" => add(book, rest, out),
        "show" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [id] = args.exactly(["id"])?;
            show(book, find_transaction(book, id)?, out)
        }
        "remove" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [id] = args.exactly(["id"])?;
            let id = find_transaction(book, id)?;
            book.remove_transaction(id)?;
            writeln!(out, "Removed the transaction {}.", short_id(id))?;
            Ok(())
        }
        other => Err(CliError::usage(format!(
            "`transaction {other}` isn't a command"
        ))),
    }
}

/// One part the user named with `--part NAME=AMOUNT`; no amount leaves it
/// open for the remainder (D-017).
struct NamedPart {
    account: Account,
    amount: Option<Money>,
}

/// `transaction add`: money moving in or out of one account, to a category,
/// to another account (a transfer), split into several parts, or to no
/// category yet.
///
/// Amounts are what happens to the account's balance: `--amount -50` spends
/// 50. The other side of the transaction is then worked out so that it
/// balances.
fn add(book: &mut Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let args = Arguments::parse(
        args,
        &[
            "date", "account", "amount", "payee", "category", "to", "part",
        ],
        &[],
    )?;
    args.exactly([])?;
    let date = parse_date(args.required("date")?)?;
    let account = find_account(book, args.required("account")?)?;
    let digits = book.currency_minor_digits(account.currency)?;
    let amount = parse_amount(args.required("amount")?, account.currency, digits)?;
    let payee = args
        .value("payee")?
        .map(str::trim)
        .filter(|payee| !payee.is_empty())
        .map(str::to_owned);

    let mut named = Vec::new();
    if let Some(name) = args.value("category")? {
        named.push(NamedPart {
            account: find_category(book, name)?,
            amount: Some(amount),
        });
    }
    if let Some(name) = args.value("to")? {
        named.push(NamedPart {
            account: find_account(book, name)?,
            amount: Some(amount),
        });
    }
    let parts = args.all("part");
    if !parts.is_empty() && !named.is_empty() {
        return Err(CliError::usage("use either --part, or --category and --to"));
    }
    if named.len() > 1 {
        return Err(CliError::usage(
            "use either --category or --to, or several --part",
        ));
    }
    for part in parts {
        // The amount comes after the last `=`, so a name may contain one.
        let (name, text) = part.rsplit_once('=').ok_or_else(|| {
            CliError::usage("--part needs NAME=AMOUNT, or NAME= to leave the amount open")
        })?;
        let amount = match text.trim() {
            "" => None,
            text => Some(parse_amount(text, account.currency, digits)?),
        };
        named.push(NamedPart {
            account: find_any(book, name)?,
            amount,
        });
    }

    // The account's part first, then the others. Each other part books the
    // opposite of what it took from the account, so the parts add up to zero.
    let mut splits = vec![Split::new(account.id, amount)];
    let mut accounts = vec![account.clone()];
    let mut open: Option<SplitId> = None;
    for part in &named {
        let split = match part.amount {
            Some(amount) => Split::new(part.account.id, negated(amount)?),
            None => {
                let split = Split::new(part.account.id, Money::zero(account.currency));
                if open.replace(split.id).is_some() {
                    return Err(CliError::usage("only one --part can leave its amount open"));
                }
                split
            }
        };
        splits.push(split);
        accounts.push(part.account.clone());
    }
    if named.is_empty() {
        // No category yet: a Rule or the user gives one later (D-083).
        splits.push(Split::uncategorized(negated(amount)?));
    }
    if let Some(open) = open {
        let remainder = Transaction::remainder(&splits, open)?;
        if let Some(split) = splits.iter_mut().find(|split| split.id == open) {
            split.amount = remainder;
        }
    }
    // A part booked to an account in another currency keeps its amount there
    // too, converted with the price table's rate (D-043, D-083).
    for (split, target) in splits.iter_mut().zip(&accounts) {
        if target.currency != account.currency {
            let (converted, rate) = book.convert(split.amount, target.currency, date)?;
            split.foreign = Some(ForeignAmount {
                amount: converted,
                rate: Some(rate),
            });
        }
    }

    let transaction = Transaction::new(TransactionId::generate(), date, payee, splits)?;
    book.add_transaction(&transaction)?;
    writeln!(out, "Added the transaction {}.", short_id(transaction.id()))?;
    Ok(())
}

fn negated(amount: Money) -> Result<Money, CliError> {
    Ok(amount.checked_neg()?)
}

/// `transaction show ID`: one transaction with all its parts.
fn show(book: &Book, id: TransactionId, out: &mut dyn Write) -> Result<(), CliError> {
    let transaction = book.transaction(id)?;
    writeln!(out, "Transaction {id}")?;
    writeln!(out, "Date:  {}", transaction.booking_date())?;
    writeln!(
        out,
        "Payee: {}",
        transaction.payee().map(plain).unwrap_or_default()
    )?;
    writeln!(out, "Parts:")?;
    let mut rows = Vec::new();
    for split in transaction.splits() {
        let place = match split.account {
            Some(id) => plain(&book.account(id)?.name),
            None => "(no category yet)".to_owned(),
        };
        let mut note = String::new();
        if let Some(foreign) = &split.foreign {
            note = format!("= {}", money_text(Some(book), foreign.amount));
            match &foreign.rate {
                Some(rate) => note.push_str(&format!(
                    " at 1 {} = {} {} ({}, {})",
                    rate.from,
                    rate.rate,
                    rate.to,
                    rate.date,
                    match rate.source {
                        RateSource::Manual => "entered by hand",
                    }
                )),
                None => note.push_str(" (both amounts given)"),
            }
        }
        rows.push(vec![place, money_text(Some(book), split.amount), note]);
    }
    for line in table(&rows, &[1]).lines() {
        writeln!(out, "  {line}")?;
    }
    Ok(())
}
