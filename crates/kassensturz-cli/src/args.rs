// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A small argument parser: `--name value`, `--name=value`, flags and
//! positional words. KassenSturz's command line is small enough that a
//! dependency isn't worth it (AGENTS.md §3.1 rule 6).

use crate::error::CliError;

/// The options and words of one command, as typed.
#[derive(Debug, Default)]
pub(crate) struct Arguments {
    values: Vec<(String, String)>,
    flags: Vec<String>,
    words: Vec<String>,
}

impl Arguments {
    /// Splits `args` into options and words. `value_options` take a value,
    /// `flag_options` don't; both are given without the leading dashes. A
    /// value is whatever follows the option, even if it starts with a dash,
    /// so `--amount -12.50` works. `--` ends the options.
    pub(crate) fn parse(
        args: &[String],
        value_options: &[&str],
        flag_options: &[&str],
    ) -> Result<Self, CliError> {
        let mut parsed = Self::default();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            if arg == "--" {
                parsed.words.extend(iter.by_ref().cloned());
                break;
            }
            let Some(option) = arg.strip_prefix("--") else {
                parsed.words.push(arg.clone());
                continue;
            };
            let (name, inline) = match option.split_once('=') {
                Some((name, value)) => (name, Some(value.to_owned())),
                None => (option, None),
            };
            if value_options.contains(&name) {
                let value = match inline {
                    Some(value) => value,
                    None => iter
                        .next()
                        .cloned()
                        .ok_or_else(|| CliError::usage(format!("--{name} needs a value")))?,
                };
                parsed.values.push((name.to_owned(), value));
            } else if flag_options.contains(&name) && inline.is_none() {
                parsed.flags.push(name.to_owned());
            } else {
                return Err(CliError::usage(format!("unknown option --{name}")));
            }
        }
        Ok(parsed)
    }

    /// The positional words.
    #[cfg(test)]
    pub(crate) fn words(&self) -> &[String] {
        &self.words
    }

    /// Exactly `N` words, or a usage error that names them.
    pub(crate) fn exactly<const N: usize>(&self, names: [&str; N]) -> Result<[&str; N], CliError> {
        let words: Vec<&str> = self.words.iter().map(String::as_str).collect();
        <[&str; N]>::try_from(words).map_err(|_| {
            CliError::usage(format!(
                "expected {}{}",
                names.len(),
                names
                    .iter()
                    .map(|name| format!(" <{name}>"))
                    .collect::<String>()
            ))
        })
    }

    /// The value of an option given at most once.
    pub(crate) fn value(&self, name: &str) -> Result<Option<&str>, CliError> {
        let mut found = self.values.iter().filter(|(option, _)| option == name);
        let first = found.next().map(|(_, value)| value.as_str());
        if found.next().is_some() {
            return Err(CliError::usage(format!("--{name} was given twice")));
        }
        Ok(first)
    }

    /// The value of an option that has to be there.
    pub(crate) fn required(&self, name: &str) -> Result<&str, CliError> {
        self.value(name)?
            .ok_or_else(|| CliError::usage(format!("--{name} is needed")))
    }

    /// Every value of an option that may repeat, in the order typed.
    pub(crate) fn all(&self, name: &str) -> Vec<&str> {
        self.values
            .iter()
            .filter(|(option, _)| option == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    /// Whether a flag was given.
    pub(crate) fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|flag| flag == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    #[test]
    fn values_flags_and_words() {
        let parsed = Arguments::parse(
            &args(&["one", "--amount", "-12.50", "--name=Bob", "--all", "two"]),
            &["amount", "name"],
            &["all"],
        )
        .expect("parses");
        assert_eq!(parsed.words(), ["one", "two"]);
        assert_eq!(parsed.value("amount").expect("once"), Some("-12.50"));
        assert_eq!(parsed.value("name").expect("once"), Some("Bob"));
        assert!(parsed.flag("all"));
    }

    #[test]
    fn two_dashes_end_the_options() {
        let parsed = Arguments::parse(&args(&["--", "--odd", "word"]), &[], &[]).expect("parses");
        assert_eq!(parsed.words(), ["--odd", "word"]);
    }

    #[test]
    fn unknown_missing_and_repeated_options_are_usage_errors() {
        assert!(Arguments::parse(&args(&["--nope"]), &[], &[]).is_err());
        assert!(Arguments::parse(&args(&["--amount"]), &["amount"], &[]).is_err());
        assert!(Arguments::parse(&args(&["--all=1"]), &[], &["all"]).is_err());
        let twice =
            Arguments::parse(&args(&["--a", "1", "--a", "2"]), &["a"], &[]).expect("parses");
        assert!(twice.value("a").is_err());
        assert_eq!(twice.all("a"), ["1", "2"]);
    }

    #[test]
    fn exactly_checks_the_number_of_words() {
        let parsed = Arguments::parse(&args(&["a", "b"]), &[], &[]).expect("parses");
        assert_eq!(parsed.exactly(["x", "y"]).expect("two"), ["a", "b"]);
        assert!(parsed.exactly(["x"]).is_err());
    }
}
