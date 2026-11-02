// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The limits a chart description is checked against before it is parsed
//! (D-281, D-505), and the raw scan of its text.
//!
//! The scan runs on text that may not be JSON at all, before `serde_json`
//! sees it, so it must never panic and must cost one pass over the text. It
//! refuses nesting deeper than [`MAX_SPEC_DEPTH`] and an object with the same
//! key twice (which `serde_json` would silently resolve to the last one, so
//! the checks and the drawing could see different specs). It also records the
//! order in which each object's keys are written, because `serde_json`'s map
//! sorts them and the "not supported here" list follows the text (design
//! section 4). Anything else that is wrong with the text is left to
//! `serde_json`.
//!
//! `document::scan` does the first two checks for documents, but it runs
//! after `serde_json` there and assumes the text is JSON; this one does not.

use std::collections::HashSet;

/// Most bytes of a chart description (D-281).
pub const MAX_SPEC_BYTES: usize = 32 * 1024;
/// Deepest nesting of a chart description's brackets (D-281).
pub const MAX_SPEC_DEPTH: usize = 10;

/// What the raw scan refused.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Refused {
    /// The nesting is deeper than [`MAX_SPEC_DEPTH`].
    TooDeep,
    /// An object has the same key twice; the path of the second one.
    DuplicateKey(String),
    /// A string isn't closed, or a key can't be decoded.
    NotJson,
}

/// The order of the text: per object its keys as written, per array its
/// items. Scalars are empty nodes. It mirrors the parsed value for any text
/// that is JSON; for any other text it is thrown away with the parse error.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Order {
    keys: Vec<(String, Order)>,
    items: Vec<Order>,
}

/// An empty order, for a value whose order wasn't recorded.
pub(super) static NO_ORDER: Order = Order {
    keys: Vec::new(),
    items: Vec::new(),
};

impl Order {
    /// The keys of an object, as written.
    pub(super) fn keys(&self) -> &[(String, Order)] {
        &self.keys
    }

    /// The order of an array's item.
    pub(super) fn item(&self, index: usize) -> &Order {
        self.items.get(index).unwrap_or(&NO_ORDER)
    }
}

/// One open bracket of the scan.
struct Open {
    order: Order,
    /// The keys seen so far, if it is an object.
    keys: Option<HashSet<String>>,
    /// Whether the next string is a key (objects) or the next value starts
    /// a new item (arrays).
    next: bool,
}

/// Scans `text` as described in the module comment. The text is at most
/// [`MAX_SPEC_BYTES`] already.
pub(super) fn scan(text: &str) -> Result<Order, Refused> {
    let mut open: Vec<Open> = Vec::new();
    let mut root = Order::default();
    let mut chars = text.char_indices();
    while let Some((start, c)) = chars.next() {
        if c.is_ascii_whitespace() || c == ':' {
            continue;
        }
        // A value starts inside an array: give it its place.
        if let Some(top) = open.last_mut()
            && top.keys.is_none()
            && top.next
            && !matches!(c, ']' | ',')
        {
            top.next = false;
            top.order.items.push(Order::default());
        }
        match c {
            '{' | '[' => {
                if open.len() >= MAX_SPEC_DEPTH {
                    return Err(Refused::TooDeep);
                }
                open.push(Open {
                    order: Order::default(),
                    keys: (c == '{').then(HashSet::new),
                    next: true,
                });
            }
            '}' | ']' => {
                let Some(closed) = open.pop() else {
                    return Err(Refused::NotJson);
                };
                match open.last_mut() {
                    Some(parent) if parent.keys.is_some() => {
                        if let Some((_, place)) = parent.order.keys.last_mut() {
                            *place = closed.order;
                        }
                    }
                    Some(parent) => {
                        if let Some(place) = parent.order.items.last_mut() {
                            *place = closed.order;
                        }
                    }
                    None => root = closed.order,
                }
            }
            ',' => {
                if let Some(top) = open.last_mut() {
                    top.next = true;
                }
            }
            '"' => {
                let end = string_end(&mut chars).ok_or(Refused::NotJson)?;
                let Some(top) = open.last_mut() else {
                    continue;
                };
                if top.keys.is_none() || !top.next {
                    continue;
                }
                top.next = false;
                let key: String =
                    serde_json::from_str(&text[start..end]).map_err(|_| Refused::NotJson)?;
                let fresh = top
                    .keys
                    .as_mut()
                    .is_some_and(|keys| keys.insert(key.clone()));
                top.order.keys.push((key, Order::default()));
                if !fresh {
                    return Err(Refused::DuplicateKey(path_of(&open)));
                }
            }
            _ => {}
        }
    }
    Ok(root)
}

/// Moves past a string's closing quote and gives the byte just after it, or
/// `None` when the string isn't closed.
fn string_end(chars: &mut std::str::CharIndices<'_>) -> Option<usize> {
    let mut escaped = false;
    for (at, c) in chars.by_ref() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(at + 1);
        }
    }
    None
}

/// The path of the innermost key or item of the open brackets.
fn path_of(open: &[Open]) -> String {
    let mut path = String::new();
    for bracket in open {
        if bracket.keys.is_some() {
            if let Some((key, _)) = bracket.order.keys.last() {
                push_key(&mut path, key);
            }
        } else if !bracket.order.items.is_empty() {
            push_index(&mut path, bracket.order.items.len() - 1);
        }
    }
    path
}

/// Adds a key to a path: `a.b` for a plain key, `a["x.y"]` (JSON-quoted, so
/// control characters are escaped) for any other. Characters that are
/// invisible or change the direction of the text around them are escaped
/// too (`\u202e`), so a hostile key can't hide or reorder what the "not
/// supported here" list shows next to it (M6e.4, D-522, T-38).
pub(super) fn push_key(path: &mut String, key: &str) {
    let plain = !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'-'));
    if plain {
        if !path.is_empty() {
            path.push('.');
        }
        path.push_str(key);
    } else {
        path.push('[');
        for c in serde_json::Value::from(key).to_string().chars() {
            if hidden(c) {
                path.push_str(&format!("\\u{:04x}", u32::from(c)));
            } else {
                path.push(c);
            }
        }
        path.push(']');
    }
}

/// Whether `c` is invisible or sets the direction of the text around it:
/// the marks and overrides of the Unicode bidirectional algorithm, the
/// zero-width characters and the byte-order mark.
fn hidden(c: char) -> bool {
    matches!(
        c,
        '\u{061c}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

/// Adds an array index to a path: `a[3]`.
pub(super) fn push_index(path: &mut String, index: usize) {
    path.push('[');
    path.push_str(&index.to_string());
    path.push(']');
}
