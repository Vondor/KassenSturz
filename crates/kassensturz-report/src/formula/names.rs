// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the formula field offers while a formula is typed (M6b.7, D-322):
//! the closed list of functions and the names of the book's categories and
//! groups, each written so that it reads back as exactly that one (T-42).

use kassensturz_core::GroupMember;

use super::eval::fits;
use crate::Source;

/// A function of the closed list, for the list beside the formula field. The
/// plain explanation is the app's, looked up by the name (D-034).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionInfo {
    /// The English name a formula writes.
    pub name: &'static str,
    /// The names of its arguments, in order; `…` after the last one means
    /// more of the same may follow.
    pub arguments: &'static [&'static str],
}

const fn function(name: &'static str, arguments: &'static [&'static str]) -> FunctionInfo {
    FunctionInfo { name, arguments }
}

/// Every function a formula can call, in the order the list shows them: the
/// order of the spec (D-292).
pub const FUNCTIONS: [FunctionInfo; 18] = [
    function("SUM", &["x", "…"]),
    function("AVERAGE", &["x", "…"]),
    function("MIN", &["x", "…"]),
    function("MAX", &["x", "…"]),
    function("COUNT", &["name", "…"]),
    function("ABS", &["x"]),
    function("SIGN", &["x"]),
    function("ROUND", &["x", "places"]),
    function("ROUNDUP", &["x", "places"]),
    function("ROUNDDOWN", &["x", "places"]),
    function("INT", &["x"]),
    function("IF", &["test", "yes", "no"]),
    function("AND", &["test", "…"]),
    function("OR", &["test", "…"]),
    function("NOT", &["test"]),
    function("IFERROR", &["x", "fallback"]),
    function("PERCENT_OF", &["part", "whole"]),
    function("CHANGE", &["new", "old"]),
];

/// A category or group of the book as a formula names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownName {
    /// What to type: the bare name when that is enough, else in single
    /// quotes, with the category's path when its name alone fits more than
    /// one.
    pub written: String,
    /// The category's names from the top-level category down, or the group's
    /// name alone.
    pub path: Vec<String>,
    /// Whether it is one of My groups.
    pub group: bool,
}

/// A bare name starts with a letter and goes on with letters, digits and
/// `_` (D-292).
fn is_bare(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(char::is_alphabetic) && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn quoted(name: &str) -> String {
    format!("'{}'", name.replace('\'', "''"))
}

fn written(name: &str) -> String {
    if is_bare(name) {
        name.to_owned()
    } else {
        quoted(name)
    }
}

/// The groups in the book's order, then the categories in the order of the
/// category tree. A name that cannot be written so that it reads back as
/// itself alone is left out; the user can still type its path.
pub(super) fn known(source: &Source) -> Vec<KnownName> {
    let candidates = source
        .groups()
        .iter()
        .map(|group| (GroupMember::Group(group.id), vec![group.name.clone()]))
        .chain(
            source
                .tree()
                .iter()
                .map(|entry| (GroupMember::Category(entry.category.id), entry.path.clone())),
        );
    candidates
        .filter_map(|(member, path)| {
            let last = path.last()?;
            let alone = fits(source, last);
            let text = if alone.len() == 1 {
                written(last)
            } else {
                quoted(&path.join(" > "))
            };
            // What is typed must fit this one and nothing else.
            let reads_back = match fits(source, &unquote(&text)).as_slice() {
                [(found, _)] => *found == member,
                _ => false,
            };
            reads_back.then_some(KnownName {
                written: text,
                path,
                group: matches!(member, GroupMember::Group(_)),
            })
        })
        .collect()
}

/// The name a bare or quoted name stands for.
fn unquote(text: &str) -> String {
    match text
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        Some(inner) => inner.replace("''", "'"),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse::Function;
    use super::*;
    use crate::Formula;

    #[test]
    fn the_function_list_is_the_closed_list_the_parser_knows() {
        let names: Vec<&str> = FUNCTIONS.iter().map(|function| function.name).collect();
        let known: Vec<&str> = Function::ALL
            .iter()
            .map(|function| function.name())
            .collect();
        assert_eq!(names, known);
        // Written the way the list shows it, every function is one the
        // parser accepts or refuses for its arguments, never as an unknown
        // function.
        for function in &FUNCTIONS {
            let arguments = function
                .arguments
                .iter()
                .filter(|argument| **argument != "…")
                .map(|_| "1")
                .collect::<Vec<_>>()
                .join(";");
            let text = format!("{}({arguments})", function.name);
            let code = match Formula::parse(&text) {
                Ok(_) => "ok".to_owned(),
                Err(error) => kassensturz_core::Report::report(&error).code.to_owned(),
            };
            assert_ne!(code, "report.formula-unknown-function", "{text}");
        }
    }
}
