// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands for Rules: adding, changing, listing, moving and removing
//! them, and running them on the book's transactions (D-014, D-031, D-088).
//!
//! A condition is typed as one argument, `FIELD OPERATOR VALUE`, e.g.
//! `--if "purpose contains REWE"`, so the shell keeps it together.

use std::io::Write;

use kassensturz_core::{
    Account, Actions, AmountTest, BookTo, Condition, CurrencyCode, MatchMode, Money, Report, Rule,
    RuleId, RuleNode, TextField, TextOperator, TransactionId,
};
use kassensturz_store::{Book, RuleRun};

use crate::amount::{money_text, parse_amount};
use crate::args::Arguments;
use crate::error::CliError;
use crate::input::{parse_currency, parse_date};
use crate::lookup::{find_account, find_category, short_id};
use crate::text::{plain, table};

/// The options that say what a Rule is, shared by `rule add` and `rule
/// change`.
const RULE_OPTIONS: [&str; 4] = ["if", "category", "to", "payee"];
const RULE_FLAGS: [&str; 2] = ["any", "case-sensitive"];

/// The text fields, as typed and as written.
const TEXT_FIELDS: [(&str, TextField); 4] = [
    ("other-party", TextField::OtherPartyName),
    ("iban", TextField::OtherPartyIban),
    ("purpose", TextField::RemittanceInformation),
    ("payee", TextField::Payee),
];

/// How text is compared, as typed and as written.
const TEXT_OPERATORS: [(&str, TextOperator); 5] = [
    ("contains", TextOperator::Contains),
    ("is", TextOperator::Equals),
    ("starts-with", TextOperator::StartsWith),
    ("ends-with", TextOperator::EndsWith),
    ("matches", TextOperator::Matches),
];

/// `rule add|change|list|move|remove|run`.
pub(crate) fn rule(book: &mut Book, args: &[String], out: &mut dyn Write) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::usage(
            "rule needs a command: add, change, list, move, remove or run",
        ));
    };
    match command.as_str() {
        "add" => {
            let mut options = RULE_OPTIONS.to_vec();
            options.push("inside");
            let args = Arguments::parse(rest, &options, &RULE_FLAGS)?;
            let [name] = args.exactly(["name"])?;
            let parent = args
                .value("inside")?
                .map(|parent| find_rule(book, parent))
                .transpose()?;
            let rule = read_rule(book, &args, RuleId::generate(), name)?;
            book.add_rule(&rule, parent.map(|parent| parent.id))?;
            writeln!(out, "Added the Rule {}.", plain(rule.name.trim()))?;
        }
        "change" => {
            let mut options = RULE_OPTIONS.to_vec();
            options.push("name");
            let args = Arguments::parse(rest, &options, &RULE_FLAGS)?;
            let [name] = args.exactly(["name"])?;
            let old = find_rule(book, name)?;
            let new_name = args.value("name")?.unwrap_or(&old.name).to_owned();
            let rule = read_rule(book, &args, old.id, &new_name)?;
            book.update_rule(&rule)?;
            writeln!(out, "Changed the Rule {}.", plain(rule.name.trim()))?;
        }
        "list" => {
            Arguments::parse(rest, &[], &[])?.exactly([])?;
            let rules = book.rules()?;
            if rules.is_empty() {
                writeln!(out, "There are no Rules yet.")?;
            }
            list(book, &rules, 0, out)?;
        }
        "move" => {
            let args = Arguments::parse(rest, &["inside", "position"], &[])?;
            let [name] = args.exactly(["name"])?;
            let rule = find_rule(book, name)?;
            let parent = args
                .value("inside")?
                .map(|parent| find_rule(book, parent))
                .transpose()?;
            // Counted from 1, as `rule list` shows them; without one, last.
            let position = match args.value("position")? {
                Some(text) => text
                    .parse::<usize>()
                    .ok()
                    .and_then(|position| position.checked_sub(1))
                    .ok_or_else(|| {
                        CliError::message(format!(
                            "`{}` isn't a position. Type 1 for the first place",
                            plain(text)
                        ))
                    })?,
                None => usize::MAX,
            };
            book.move_rule(rule.id, parent.as_ref().map(|parent| parent.id), position)?;
            match parent {
                Some(parent) => writeln!(
                    out,
                    "Moved {} inside {}.",
                    plain(&rule.name),
                    plain(&parent.name)
                )?,
                None => writeln!(out, "Moved {}.", plain(&rule.name))?,
            }
        }
        "remove" => {
            let args = Arguments::parse(rest, &[], &[])?;
            let [name] = args.exactly(["name"])?;
            let rule = find_rule(book, name)?;
            book.remove_rule(rule.id)?;
            writeln!(
                out,
                "Removed the Rule {} and its sub-Rules.",
                plain(&rule.name)
            )?;
        }
        "run" => {
            let args = Arguments::parse(rest, &[], &["dry-run"])?;
            args.exactly([])?;
            if args.flag("dry-run") {
                let run = book.preview_rules()?;
                report(book, &run, "Would change", out)?;
                writeln!(out, "Nothing was changed.")?;
            } else {
                let run = book.run_rules()?;
                report(book, &run, "Changed", out)?;
            }
        }
        other => {
            return Err(CliError::usage(format!("`rule {other}` isn't a command")));
        }
    }
    Ok(())
}

/// The Rule the options of `rule add` or `rule change` describe.
fn read_rule(book: &Book, args: &Arguments, id: RuleId, name: &str) -> Result<Rule, CliError> {
    let case_sensitive = args.flag("case-sensitive");
    let conditions = args
        .all("if")
        .into_iter()
        .map(|text| parse_condition(book, text, case_sensitive))
        .collect::<Result<Vec<_>, _>>()?;
    if conditions.is_empty() {
        return Err(CliError::usage(
            "a Rule needs at least one --if, such as --if \"purpose contains REWE\"",
        ));
    }
    let book_to = match (args.value("category")?, args.value("to")?) {
        (Some(_), Some(_)) => return Err(CliError::usage("use either --category or --to")),
        (Some(category), None) => Some(BookTo::Category(find_category(book, category)?.id)),
        (None, Some(account)) => Some(BookTo::Transfer(find_account(book, account)?.id)),
        (None, None) => None,
    };
    Ok(Rule {
        id,
        name: name.trim().to_owned(),
        match_mode: if args.flag("any") {
            MatchMode::Any
        } else {
            MatchMode::All
        },
        conditions,
        actions: Actions {
            book_to,
            payee: args.value("payee")?.map(str::to_owned),
        },
    })
}

/// A condition typed as `FIELD OPERATOR VALUE`.
fn parse_condition(book: &Book, text: &str, case_sensitive: bool) -> Result<Condition, CliError> {
    let malformed = || {
        CliError::usage(format!(
            "`{}` isn't a condition. Type FIELD OPERATOR VALUE, such as \"purpose contains REWE\"; \
             `kassensturz-cli help` lists them",
            plain(text)
        ))
    };
    let mut words = text.trim().splitn(3, ' ');
    let field = words.next().unwrap_or_default();
    let operator = words.next().ok_or_else(malformed)?;
    let value = words.next().unwrap_or_default().trim();
    if let Some((_, field)) = TEXT_FIELDS.iter().find(|(word, _)| *word == field) {
        let (_, operator) = TEXT_OPERATORS
            .iter()
            .find(|(word, _)| *word == operator)
            .ok_or_else(malformed)?;
        return Ok(Condition::Text {
            field: *field,
            operator: *operator,
            value: value.to_owned(),
            case_sensitive,
        });
    }
    let values: Vec<&str> = value.split_whitespace().collect();
    match (field, operator, values.as_slice()) {
        ("amount", "is", [amount, currency]) => {
            let currency = parse_currency(currency)?;
            Ok(Condition::Amount(AmountTest::Equals(amount_in(
                book, amount, currency,
            )?)))
        }
        ("amount", "between", [min, max, currency]) => {
            let currency = parse_currency(currency)?;
            Ok(Condition::Amount(AmountTest::Between {
                min: amount_in(book, min, currency)?,
                max: amount_in(book, max, currency)?,
            }))
        }
        ("amount", "in", []) => Ok(Condition::Amount(AmountTest::MoneyIn)),
        ("amount", "out", []) => Ok(Condition::Amount(AmountTest::MoneyOut)),
        ("date", "between", [first, last]) => Ok(Condition::BookingDate {
            first: parse_date(first)?,
            last: parse_date(last)?,
        }),
        ("currency", "is", [currency]) => Ok(Condition::Currency(parse_currency(currency)?)),
        ("account", "is", _) if !value.is_empty() => {
            Ok(Condition::Account(find_account(book, value)?.id))
        }
        _ => Err(malformed()),
    }
}

fn amount_in(book: &Book, text: &str, currency: CurrencyCode) -> Result<Money, CliError> {
    let digits = book.currency_minor_digits(currency)?;
    parse_amount(text, currency, digits)
}

/// Writes `nodes` and their sub-Rules, each level indented further.
fn list(
    book: &Book,
    nodes: &[RuleNode],
    depth: usize,
    out: &mut dyn Write,
) -> Result<(), CliError> {
    let indent = "  ".repeat(depth);
    for (number, node) in (1..).zip(nodes) {
        let rule = &node.rule;
        writeln!(
            out,
            "{indent}{number}. {}  (ID {})",
            plain(&rule.name),
            short_id(rule.id)
        )?;
        let joined = match rule.match_mode {
            MatchMode::All => "when all of",
            MatchMode::Any => "when any of",
        };
        let conditions: Vec<String> = rule
            .conditions
            .iter()
            .map(|condition| condition_text(book, condition))
            .collect();
        writeln!(out, "{indent}   {joined}: {}", conditions.join("; "))?;
        writeln!(
            out,
            "{indent}   then: {}",
            actions_text(book, &rule.actions)
        )?;
        list(book, &node.sub_rules, depth + 1, out)?;
    }
    Ok(())
}

/// A condition as it is typed.
fn condition_text(book: &Book, condition: &Condition) -> String {
    match condition {
        Condition::Text {
            field,
            operator,
            value,
            case_sensitive,
        } => {
            let field = TEXT_FIELDS
                .iter()
                .find(|(_, listed)| listed == field)
                .map_or("?", |(word, _)| word);
            let operator = TEXT_OPERATORS
                .iter()
                .find(|(_, listed)| listed == operator)
                .map_or("?", |(word, _)| word);
            let capitals = if *case_sensitive {
                " (capitals count)"
            } else {
                ""
            };
            format!("{field} {operator} {:?}{capitals}", plain(value))
        }
        Condition::Amount(AmountTest::Equals(amount)) => {
            format!("amount is {}", money_text(Some(book), *amount))
        }
        Condition::Amount(AmountTest::Between { min, max }) => format!(
            "amount between {} and {}",
            money_text(Some(book), *min),
            money_text(Some(book), *max)
        ),
        Condition::Amount(AmountTest::MoneyIn) => "amount in".to_owned(),
        Condition::Amount(AmountTest::MoneyOut) => "amount out".to_owned(),
        Condition::BookingDate { first, last } => format!("date between {first} and {last}"),
        Condition::Currency(currency) => format!("currency is {currency}"),
        Condition::Account(id) => format!("account is {}", account_name(book, *id)),
    }
}

fn actions_text(book: &Book, actions: &Actions) -> String {
    let mut parts = Vec::new();
    match actions.book_to {
        Some(BookTo::Category(id)) => parts.push(format!("category {}", account_name(book, id))),
        Some(BookTo::Transfer(id)) => {
            parts.push(format!("Transfer to {}", account_name(book, id)));
        }
        None => {}
    }
    if let Some(payee) = &actions.payee {
        parts.push(format!("payee {:?}", plain(payee)));
    }
    if parts.is_empty() {
        "nothing, unless a sub-Rule says so".to_owned()
    } else {
        parts.join(", ")
    }
}

fn account_name(book: &Book, id: kassensturz_core::AccountId) -> String {
    book.account(id)
        .map_or_else(|_| id.to_string(), |account: Account| plain(&account.name))
}

/// Writes what a run of the Rules changed, or would change, and what it
/// left alone.
fn report(book: &Book, run: &RuleRun, verb: &str, out: &mut dyn Write) -> Result<(), CliError> {
    let rule_name = |id: RuleId| {
        book.rule(id)
            .map_or_else(|_| short_id(id), |rule| plain(&rule.name))
    };
    if run.changed.is_empty() {
        writeln!(out, "The Rules change no transaction.")?;
    } else {
        writeln!(out, "{verb} {} transaction(s):", run.changed.len())?;
        let mut rows = vec![header(&["Date", "Payee", "Rule", "ID"])];
        for change in &run.changed {
            let (date, payee) = date_and_payee(book, change.transaction)?;
            rows.push(vec![
                date,
                payee,
                rule_name(change.rule),
                short_id(change.transaction),
            ]);
        }
        write!(out, "{}", indented(&table(&rows, &[])))?;
    }
    if !run.skipped.is_empty() {
        writeln!(
            out,
            "Left {} transaction(s) alone that a Rule matched:",
            run.skipped.len()
        )?;
        let mut rows = vec![header(&["Date", "Rule", "Why", "ID"])];
        for skipped in &run.skipped {
            let (date, _) = date_and_payee(book, skipped.transaction)?;
            rows.push(vec![
                date,
                rule_name(skipped.rule),
                skipped.reason.report().code.to_owned(),
                short_id(skipped.transaction),
            ]);
        }
        write!(out, "{}", indented(&table(&rows, &[])))?;
    }
    Ok(())
}

fn header(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn indented(text: &str) -> String {
    text.lines().map(|line| format!("  {line}\n")).collect()
}

fn date_and_payee(book: &Book, id: TransactionId) -> Result<(String, String), CliError> {
    // A preview reads the transaction as it is now; a run, as the Rules
    // left it.
    let transaction = book.transaction(id)?;
    Ok((
        transaction.booking_date().to_string(),
        transaction.payee().map(plain).unwrap_or_default(),
    ))
}

/// The Rule called `name`: its whole path from the top, such as
/// `Shops/Drinks`, or just its own name if no other Rule has it.
pub(crate) fn find_rule(book: &Book, name: &str) -> Result<Rule, CliError> {
    let mut candidates = Vec::new();
    collect(&book.rules()?, "", &mut candidates);
    let wanted = name.trim().to_lowercase();
    let mut matches: Vec<&(Rule, String)> = candidates
        .iter()
        .filter(|(_, path)| path.trim().to_lowercase() == wanted)
        .collect();
    if matches.is_empty() {
        matches = candidates
            .iter()
            .filter(|(rule, _)| rule.name.trim().to_lowercase() == wanted)
            .collect();
    }
    match matches.as_slice() {
        [] => Err(CliError::message(format!(
            "there is no Rule called `{}`; `rule list` shows them",
            plain(name)
        ))),
        [(rule, _)] => Ok(rule.clone()),
        several => Err(CliError::message(format!(
            "`{}` could be several: {}. Type the whole path, such as Shops/Drinks",
            plain(name),
            several
                .iter()
                .map(|(_, path)| plain(path))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn collect(nodes: &[RuleNode], above: &str, into: &mut Vec<(Rule, String)>) {
    for node in nodes {
        let path = if above.is_empty() {
            node.rule.name.clone()
        } else {
            format!("{above}/{}", node.rule.name)
        };
        collect(&node.sub_rules, &path, into);
        into.push((node.rule.clone(), path));
    }
}
