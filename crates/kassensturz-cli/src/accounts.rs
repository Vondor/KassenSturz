// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands for accounts, categories, registers and balances.

use std::io::Write;

use kassensturz_core::{Account, AccountId, Money};
use kassensturz_store::{Action, Book};

use crate::amount::{format_minor, money_text};
use crate::args::Arguments;
use crate::error::CliError;
use crate::input::{
    account_type_word, category_kind_word, parse_account_type, parse_category_kind, parse_currency,
};
use crate::lookup::{find_account, find_any, find_category, short_id};
use crate::text::{plain, table};

/// `account add|list|rename|close|reopen`.
pub(crate) fn account(
    book: &mut Book,
    args: &[String],
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::usage(
            "account needs a command: add, list, rename, close or reopen",
        ));
    };
    match command.as_str() {
        "add" => {
            let args = Arguments::parse(rest, &["type", "currency"], &[])?;
            let [name] = args.exactly(["name"])?;
            let account_type = parse_account_type(args.required("type")?)?;
            let currency = parse_currency(args.required("currency")?)?;
            // One change, which undo takes back as a whole, and none at all
            // if the account is refused.
            book.as_one_action(Action::AddAccount, |book| {
                book.use_currency(currency)?;
                book.add_account(&Account {
                    id: AccountId::generate(),
                    name: name.trim().to_owned(),
                    kind: account_type.kind(),
                    currency,
                    account_type: Some(account_type),
                    parent: None,
                    closed: false,
                })
            })?;
            writeln!(out, "Added the account {}.", plain(name.trim()))?;
        }
        "list" => {
            let args = Arguments::parse(rest, &[], &["all"])?;
            args.exactly([])?;
            let mut rows = vec![vec![
                "Name".to_owned(),
                "Type".to_owned(),
                "Currency".to_owned(),
                "Status".to_owned(),
            ]];
            for account in book.accounts(args.flag("all"))? {
                rows.push(vec![
                    plain(&account.name),
                    account
                        .account_type
                        .map_or("-", account_type_word)
                        .to_owned(),
                    account.currency.to_string(),
                    if account.closed { "closed" } else { "open" }.to_owned(),
                ]);
            }
            write!(out, "{}", table(&rows, &[]))?;
        }
        "rename" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [old, new] = args.exactly(["name", "new name"])?;
            let account = find_account(book, old)?;
            book.rename_account(account.id, new.trim())?;
            writeln!(
                out,
                "Renamed {} to {}.",
                plain(&account.name),
                plain(new.trim())
            )?;
        }
        "close" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [name] = args.exactly(["name"])?;
            let account = find_account(book, name)?;
            book.close_account(account.id)?;
            writeln!(out, "Closed {}.", plain(&account.name))?;
        }
        "reopen" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [name] = args.exactly(["name"])?;
            let account = find_account(book, name)?;
            book.reopen_account(account.id)?;
            writeln!(out, "Reopened {}.", plain(&account.name))?;
        }
        other => {
            return Err(CliError::usage(format!(
                "`account {other}` isn't a command"
            )));
        }
    }
    Ok(())
}

/// `category add|list|move`.
pub(crate) fn category(
    book: &mut Book,
    args: &[String],
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::usage(
            "category needs a command: add, list or move",
        ));
    };
    match command.as_str() {
        "add" => {
            let args = Arguments::parse(rest, &["kind", "currency", "inside"], &[])?;
            let [name] = args.exactly(["name"])?;
            let parent = args
                .value("inside")?
                .map(|parent| find_category(book, parent))
                .transpose()?;
            // Inside another category, the kind and the currency are the
            // parent's unless said otherwise; the store refuses a mismatch.
            let kind = match (args.value("kind")?, &parent) {
                (Some(kind), _) => parse_category_kind(kind)?,
                (None, Some(parent)) => parent.kind,
                (None, None) => {
                    return Err(CliError::usage("--kind is needed (income or expense)"));
                }
            };
            let currency = match (args.value("currency")?, &parent) {
                (Some(currency), _) => parse_currency(currency)?,
                (None, Some(parent)) => parent.currency,
                (None, None) => return Err(CliError::usage("--currency is needed")),
            };
            book.as_one_action(Action::AddCategory, |book| {
                book.use_currency(currency)?;
                book.add_account(&Account {
                    id: AccountId::generate(),
                    name: name.trim().to_owned(),
                    kind,
                    currency,
                    account_type: None,
                    parent: parent.map(|parent| parent.id),
                    closed: false,
                })
            })?;
            writeln!(out, "Added the category {}.", plain(name.trim()))?;
        }
        "list" => {
            Arguments::parse(rest, &[], &[])?.exactly([])?;
            let mut rows = vec![vec![
                "Category".to_owned(),
                "Kind".to_owned(),
                "Currency".to_owned(),
            ]];
            for entry in book.categories()? {
                let depth = entry.path.len().saturating_sub(1);
                rows.push(vec![
                    format!("{}{}", "  ".repeat(depth), plain(&entry.category.name)),
                    category_kind_word(entry.category.kind).to_owned(),
                    entry.category.currency.to_string(),
                ]);
            }
            write!(out, "{}", table(&rows, &[]))?;
        }
        "move" => {
            let args = Arguments::parse(rest, &["inside"], &[])?;
            let [name] = args.exactly(["name"])?;
            let category = find_category(book, name)?;
            let parent = args
                .value("inside")?
                .map(|parent| find_category(book, parent))
                .transpose()?;
            book.move_category(category.id, parent.as_ref().map(|parent| parent.id))?;
            match parent {
                Some(parent) => writeln!(
                    out,
                    "Moved {} inside {}.",
                    plain(&category.name),
                    plain(&parent.name)
                )?,
                None => writeln!(out, "Moved {} to the top level.", plain(&category.name))?,
            }
        }
        other => {
            return Err(CliError::usage(format!(
                "`category {other}` isn't a command"
            )));
        }
    }
    Ok(())
}

/// `balances [--all]`: every account's balance, and the totals per currency.
pub(crate) fn balances(book: &Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let args = Arguments::parse(args, &[], &["all"])?;
    args.exactly([])?;
    let mut rows = vec![vec!["Account".to_owned(), "Balance".to_owned()]];
    let mut totals: Vec<Money> = Vec::new();
    for account in book.accounts(args.flag("all"))? {
        let balance = book.balance(account.id)?;
        rows.push(vec![plain(&account.name), money_text(Some(book), balance)]);
        // Totals add up the exact amounts; nothing is converted between
        // currencies (D-002).
        match totals
            .iter_mut()
            .find(|total| total.currency() == balance.currency())
        {
            Some(total) => *total = total.checked_add(balance)?,
            None => totals.push(balance),
        }
    }
    totals.sort_by_key(|total| total.currency().as_str().to_owned());
    for total in totals {
        rows.push(vec!["Total".to_owned(), money_text(Some(book), total)]);
    }
    write!(out, "{}", table(&rows, &[1]))?;
    Ok(())
}

/// `register ACCOUNT`: its transactions, oldest first, with the balance after
/// each one.
pub(crate) fn register(book: &Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let args = Arguments::parse(args, &[], &[])?;
    let [name] = args.exactly(["account"])?;
    let account = find_any(book, name)?;
    let digits = book.currency_minor_digits(account.currency)?;
    let mut rows = vec![vec![
        "Date".to_owned(),
        "Payee".to_owned(),
        format!("Amount ({})", account.currency),
        format!("Balance ({})", account.currency),
        "ID".to_owned(),
    ]];
    let mut balance = Money::zero(account.currency);
    for entry in book.register(account.id)? {
        balance = balance.checked_add(entry.amount)?;
        rows.push(vec![
            entry.booking_date.to_string(),
            entry.payee.as_deref().map(plain).unwrap_or_default(),
            format_minor(entry.amount.minor_units(), digits),
            format_minor(balance.minor_units(), digits),
            short_id(entry.transaction),
        ]);
    }
    writeln!(out, "{}", plain(&account.name))?;
    write!(out, "{}", table(&rows, &[2, 3]))?;
    Ok(())
}
