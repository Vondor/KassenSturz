// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which book the server reads (D-196): the one given with `--book`, or
//! else the book the app opened last, as its settings file says.
//!
//! The app keeps that path with KConfig's `writePathEntry`, which writes the
//! key as `LastBook[$e]`, puts `$HOME` in place of the user's home folder,
//! doubles every other `$` and escapes a few characters (`\\`, `\s`, `\n`,
//! `\t`, `\r`, `\xNN`). This reads exactly that and refuses anything else,
//! such as another variable or a command, rather than guess at a path.

use std::ffi::OsString;
use std::path::PathBuf;

use kassensturz_core::ai_switch;

/// The settings group and key of the book opened last, as the app writes
/// them (`kassensturz-ui-bridge/src/settings.rs`).
const BOOKS: &str = "Books";
const LAST_BOOK: &str = "LastBook";

/// The book named by the program's `arguments`, or the one the app opened
/// last. The error is a message for the user.
pub(crate) fn from_arguments(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<PathBuf, String> {
    let mut arguments = arguments.into_iter();
    match arguments.next() {
        None => last_book(),
        Some(flag) if flag == "--book" => match (arguments.next(), arguments.next()) {
            (Some(path), None) if !path.is_empty() => Ok(PathBuf::from(path)),
            _ => Err(usage()),
        },
        Some(_) => Err(usage()),
    }
}

fn usage() -> String {
    "usage: kassensturz-mcp [--book <file>]. Without --book it reads the book KassenSturz \
     opened last. kassensturz-mcp --request-folder names the folder its write tools use."
        .to_owned()
}

/// The book the app opened last, from its settings file.
fn last_book() -> Result<PathBuf, String> {
    let none = || {
        "KassenSturz hasn't opened a book yet. Open one in KassenSturz, or give it with \
         --book <file>."
            .to_owned()
    };
    let text = ai_switch::settings_file()
        .and_then(|file| std::fs::read_to_string(file).ok())
        .ok_or_else(none)?;
    let home = std::env::var("HOME").ok();
    match last_book_in(&text, home.as_deref()) {
        Ok(Some(path)) => Ok(path),
        Ok(None) => Err(none()),
        Err(()) => Err(
            "the path of the book KassenSturz opened last can't be read here. Give it with \
             --book <file>."
                .to_owned(),
        ),
    }
}

/// The book opened last in the settings file's `text`, with `home` as the
/// user's home folder. `Ok(None)` when there is none, `Err` when the entry
/// is there but isn't a path written as the app writes it.
pub(crate) fn last_book_in(text: &str, home: Option<&str>) -> Result<Option<PathBuf>, ()> {
    let expanded_key = format!("{LAST_BOOK}[$e]");
    // KConfig reads the last entry of a key; the app writes only one of the
    // two forms, so whichever is there is the one.
    let (value, expand) = match ai_switch::value_in(text, BOOKS, &expanded_key) {
        Some(value) => (value, true),
        None => match ai_switch::value_in(text, BOOKS, LAST_BOOK) {
            Some(value) => (value, false),
            None => return Ok(None),
        },
    };
    if value.is_empty() {
        return Ok(None);
    }
    let unescaped = unescape(value)?;
    let path = if expand {
        expand_home(&unescaped, home)?
    } else {
        unescaped
    };
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(Some(path))
    } else {
        Err(())
    }
}

/// Undoes KConfig's escapes of a value.
fn unescape(value: &str) -> Result<String, ()> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut rest = value.bytes();
    while let Some(byte) = rest.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        match rest.next().ok_or(())? {
            b'\\' => bytes.push(b'\\'),
            b's' => bytes.push(b' '),
            b'n' => bytes.push(b'\n'),
            b't' => bytes.push(b'\t'),
            b'r' => bytes.push(b'\r'),
            b'x' => {
                let high = hex(rest.next().ok_or(())?)?;
                let low = hex(rest.next().ok_or(())?)?;
                bytes.push(high << 4 | low);
            }
            _ => return Err(()),
        }
    }
    String::from_utf8(bytes).map_err(|_| ())
}

fn hex(digit: u8) -> Result<u8, ()> {
    char::from(digit)
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or(())
}

/// Undoes KConfig's `$HOME` and `$$` in a path entry.
fn expand_home(value: &str, home: Option<&str>) -> Result<String, ()> {
    let (start, rest) = match value.strip_prefix("$HOME") {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            (home.filter(|home| !home.is_empty()).ok_or(())?, rest)
        }
        _ => ("", value),
    };
    let mut path = String::from(start);
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        if c == '$' {
            // Only a doubled `$` is a `$`; anything else asks KConfig for a
            // variable or a command, which this never does.
            if chars.next() != Some('$') {
                return Err(());
            }
        }
        path.push(c);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<Option<PathBuf>, ()> {
        last_book_in(text, Some("/home/ada"))
    }

    fn path(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    // Unix paths: the app's settings file is read only on Linux yet
    // (D-177), and on Windows "/home/ada" isn't an absolute path.
    #[cfg(unix)]
    #[test]
    fn the_path_the_app_wrote_is_read_with_the_home_folder() {
        assert_eq!(
            read("[Books]\nLastBook[$e]=$HOME/Documents/Household.ksz\n"),
            Ok(Some(path("/home/ada/Documents/Household.ksz")))
        );
        assert_eq!(
            read("[AI]\nEnabled=true\n\n[Books]\nLastBook[$e]=/srv/books/Family.ksz\n"),
            Ok(Some(path("/srv/books/Family.ksz")))
        );
        // KConfig's escapes and a doubled `$`.
        assert_eq!(
            read("[Books]\nLastBook[$e]=$HOME/\\sA\\\\B $$5\\x01.ksz\\s\n"),
            Ok(Some(path("/home/ada/ A\\B $5\u{1}.ksz ")))
        );
        // Without the expansion flag the text is the path.
        assert_eq!(
            read("[Books]\nLastBook=/data/$HOME.ksz\n"),
            Ok(Some(path("/data/$HOME.ksz")))
        );
        // Text after the home folder's name isn't the home folder.
        assert_eq!(read("[Books]\nLastBook[$e]=$HOMEWORK/a.ksz\n"), Err(()));
    }

    #[test]
    fn no_book_yet_is_none() {
        assert_eq!(read(""), Ok(None));
        assert_eq!(read("[Books]\n"), Ok(None));
        assert_eq!(read("[Books]\nLastBook[$e]=\n"), Ok(None));
        assert_eq!(read("[Other]\nLastBook[$e]=/a.ksz\n"), Ok(None));
    }

    #[test]
    fn anything_but_the_apps_own_form_is_refused() {
        // Another variable, a command, a lone `$`.
        assert_eq!(
            read("[Books]\nLastBook[$e]=$XDG_DATA_HOME/a.ksz\n"),
            Err(())
        );
        assert_eq!(read("[Books]\nLastBook[$e]=$(rm -rf ~)/a.ksz\n"), Err(()));
        assert_eq!(read("[Books]\nLastBook[$e]=/a$\n"), Err(()));
        // Bad escapes, a relative path, no home folder known.
        assert_eq!(read("[Books]\nLastBook[$e]=/a\\q.ksz\n"), Err(()));
        assert_eq!(read("[Books]\nLastBook[$e]=/a\\x4\n"), Err(()));
        assert_eq!(read("[Books]\nLastBook[$e]=a.ksz\n"), Err(()));
        assert_eq!(
            last_book_in("[Books]\nLastBook[$e]=$HOME/a.ksz\n", None),
            Err(())
        );
    }

    #[test]
    fn the_arguments_name_the_book_or_nothing() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        assert_eq!(
            from_arguments(args(&["--book", "/srv/a.ksz"])),
            Ok(path("/srv/a.ksz"))
        );
        for wrong in [
            &["--book"][..],
            &["--book", ""],
            &["--book", "/a.ksz", "more"],
            &["--port", "8080"],
            &["/a.ksz"],
        ] {
            assert!(from_arguments(args(wrong)).is_err(), "{wrong:?}");
        }
    }
}
