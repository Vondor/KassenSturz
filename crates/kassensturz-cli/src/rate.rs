// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands for exchange rates, the book's price table (D-028).

use std::io::Write;
use std::str::FromStr;

use kassensturz_core::{DatedRate, DatedRateId, ExchangeRate, RateSource};
use kassensturz_store::{Action, Book};

use crate::args::Arguments;
use crate::error::CliError;
use crate::input::{parse_currency, parse_date};
use crate::text::table;

/// `rate add|list|remove`. Rates are entered by hand in v1 (D-028).
pub(crate) fn rate(book: &mut Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::usage("rate needs a command: add, list or remove"));
    };
    match command.as_str() {
        "add" => {
            let args = Arguments::parse(rest, &["date", "from", "to", "rate"], &[])?;
            args.exactly([])?;
            let from = parse_currency(args.required("from")?)?;
            let to = parse_currency(args.required("to")?)?;
            let date = parse_date(args.required("date")?)?;
            let rate = ExchangeRate::from_str(args.required("rate")?)?;
            // Both currencies are in use from now on, so a rate can come
            // before the first account in the currency. One change, which
            // undo takes back as a whole.
            book.as_one_action(Action::AddRate, |book| {
                book.use_currency(from)?;
                book.use_currency(to)?;
                book.add_rate(&DatedRate {
                    id: DatedRateId::generate(),
                    date,
                    from,
                    to,
                    rate,
                    source: RateSource::Manual,
                })
            })?;
            writeln!(out, "Added the rate: 1 {from} = {rate} {to} on {date}.")?;
        }
        "list" => {
            let args = Arguments::parse(rest, &["from", "to"], &[])?;
            args.exactly([])?;
            let from = parse_currency(args.required("from")?)?;
            let to = parse_currency(args.required("to")?)?;
            let mut rows = vec![vec![
                "Date".to_owned(),
                format!("1 {from} in {to}"),
                "Source".to_owned(),
            ]];
            for rate in book.rates(from, to)? {
                rows.push(vec![
                    rate.date.to_string(),
                    rate.rate.to_string(),
                    match rate.source {
                        RateSource::Manual => "entered by hand".to_owned(),
                    },
                ]);
            }
            write!(out, "{}", table(&rows, &[1]))?;
        }
        "remove" => {
            let args = Arguments::parse(rest, &["date", "from", "to"], &[])?;
            args.exactly([])?;
            let from = parse_currency(args.required("from")?)?;
            let to = parse_currency(args.required("to")?)?;
            let date = parse_date(args.required("date")?)?;
            // A book holds one rate per pair of currencies, direction and
            // day, so these three name exactly one (D-079).
            let found = book
                .rates(from, to)?
                .into_iter()
                .find(|rate| rate.date == date)
                .ok_or_else(|| {
                    CliError::message(format!("there is no rate from {from} into {to} on {date}"))
                })?;
            book.remove_rate(found.id)?;
            writeln!(out, "Removed the rate from {from} into {to} on {date}.")?;
        }
        other => return Err(CliError::usage(format!("`rate {other}` isn't a command"))),
    }
    Ok(())
}
