// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the receipt inbox files a bill (US7, #333, D-752,
//! `docs/RECEIPT_INBOX.md` §5): the path inside the archive, built by code
//! from the book's own names and never by the AI, and the check that the
//! two folders the user chose don't overlap.
//!
//! `<archive>/<account>/<category>/<YYMMDD>_<name>.<ext>`. Every part is
//! sanitised ([`sanitise`]) so it can never leave the archive, name a
//! reserved Windows device or grow past the lengths every desktop system
//! takes. Nothing here touches the disk: the caller lists the names a folder
//! already holds, and moving the file is `kassensturz-platform`'s.

use std::path::{Path, PathBuf};

use crate::Date;

/// The most characters one part of the path may have.
pub const MAX_COMPONENT_CHARS: usize = 80;

/// The most characters the whole path may have, the archive included, so
/// it works on Windows without long-path support.
pub const MAX_PATH_CHARS: usize = 240;

/// The name a part gets when nothing of it is left after sanitising.
pub const EMPTY_NAME: &str = "bill";

/// The shortest a part is cut to when the whole path is too long.
const MIN_COMPONENT_CHARS: usize = 8;

/// Room kept at the end of the file name for the number a taken name gets
/// (`-2` up to `-999`).
const NUMBER_ROOM: usize = 4;

/// The characters no file name may hold on one of the desktop systems.
const FORBIDDEN: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// The names Windows keeps for its devices, also with an ending.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `name` made safe as one part of a path: control characters and
/// `/ \ : * ? " < > |` removed, leading and trailing dots and spaces
/// removed, a reserved Windows name followed by `_`, cut to
/// [`MAX_COMPONENT_CHARS`], and [`EMPTY_NAME`] for nothing left. The
/// result is never `.` or `..`, holds no separator and is never empty.
///
/// Unicode normalisation is left out: it needs a library that isn't a
/// dependency (AGENTS.md §3.1 rule 6); names are compared without case
/// instead ([`free_file_name`]).
pub fn sanitise(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| !c.is_control() && !FORBIDDEN.contains(c))
        .collect();
    cut(&kept, MAX_COMPONENT_CHARS)
}

/// `name` with `_` after a reserved Windows name, before its ending.
fn unreserved(name: &str) -> String {
    if !is_reserved(name) {
        return name.to_owned();
    }
    match name.split_once('.') {
        Some((stem, rest)) => format!("{}_.{rest}", stem.trim_end()),
        None => format!("{name}_"),
    }
}

/// `name` without leading and trailing dots and white space.
fn trimmed(name: &str) -> &str {
    name.trim_matches(|c: char| c == '.' || c.is_whitespace())
}

/// Whether `name`, up to its first dot, is a name Windows keeps.
fn is_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    RESERVED
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

/// `name` cut to at most `chars` characters, without leading or trailing
/// dots or spaces, not a reserved Windows name, and never empty.
fn cut(name: &str, chars: usize) -> String {
    let chars = chars.max(EMPTY_NAME.len() + 1);
    let mut short = trimmed(name).to_owned();
    loop {
        let candidate: String = short.chars().take(chars).collect();
        let candidate = unreserved(trimmed(&candidate));
        if candidate.is_empty() {
            return EMPTY_NAME.to_owned();
        }
        if candidate.chars().count() <= chars {
            return candidate;
        }
        // The `_` made it too long: one character less of the name.
        short = short.chars().take(chars - 1).collect();
    }
}

/// The place a bill goes inside the archive, before a free name is found:
/// the account's folder, the category's folder, and the file's stem and
/// ending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivePlace {
    /// The account's folder.
    pub account: String,
    /// The category's folder.
    pub category: String,
    /// The file name without its ending: `YYMMDD_<name>`.
    pub stem: String,
    /// The ending, lower case, without the dot.
    pub ending: String,
}

/// Why no place in the archive could be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// The ending isn't one a bill may have.
    Ending,
    /// The archive's own path is so long that no file name fits under
    /// [`MAX_PATH_CHARS`].
    TooLong,
}

impl ArchivePlace {
    /// The place for a bill of the transaction on `date` in `account`,
    /// booked to `category` (`None`: `unsorted`, the user's words for a
    /// part without a category), named `name` (the title, or the file's
    /// own stem), with the file's `ending`, one of `endings`. The whole
    /// path under `archive` stays below [`MAX_PATH_CHARS`]: the longest
    /// part is cut first, none below a few characters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        archive: &Path,
        account: &str,
        category: Option<&str>,
        unsorted: &str,
        date: Date,
        name: &str,
        ending: &str,
        endings: &[&str],
    ) -> Result<Self, PlaceError> {
        let ending = ending.to_lowercase();
        if !endings.contains(&ending.as_str()) {
            return Err(PlaceError::Ending);
        }
        let day = format!(
            "{:02}{:02}{:02}_",
            date.year().rem_euclid(100),
            date.month(),
            date.day()
        );
        let mut parts = [
            sanitise(account),
            sanitise(category.unwrap_or(unsorted)),
            sanitise(name),
        ];
        // The day goes before the name and counts towards its length.
        let name_room = MAX_COMPONENT_CHARS - day.len();
        parts[2] = cut(&parts[2], name_room);
        // The archive, three separators, the day, the dot, the ending and
        // room for a number.
        let fixed = archive.as_os_str().to_string_lossy().chars().count()
            + 3
            + day.len()
            + 1
            + ending.len()
            + NUMBER_ROOM;
        let room = (MAX_PATH_CHARS - 1)
            .checked_sub(fixed)
            .ok_or(PlaceError::TooLong)?;
        loop {
            let used: usize = parts.iter().map(|part| part.chars().count()).sum();
            if used <= room {
                break;
            }
            let (longest, length) = parts
                .iter()
                .enumerate()
                .map(|(index, part)| (index, part.chars().count()))
                .max_by_key(|&(index, length)| (length, usize::MAX - index))
                .unwrap_or((0, 0));
            if length <= MIN_COMPONENT_CHARS {
                return Err(PlaceError::TooLong);
            }
            parts[longest] = cut(&parts[longest], length - 1);
        }
        let [account, category, name] = parts;
        Ok(Self {
            account,
            category,
            stem: format!("{day}{name}"),
            ending,
        })
    }

    /// The folder inside `archive` the bill goes into.
    pub fn folder(&self, archive: &Path) -> PathBuf {
        archive.join(&self.account).join(&self.category)
    }
}

/// The first name `<stem>.<ending>`, `<stem>-2.<ending>`, … that none of
/// `taken` holds, compared without regard to case so it is free on Windows
/// and macOS too. Never overwrites: a name is only ever free.
pub fn free_file_name<'a>(
    stem: &str,
    ending: &str,
    taken: impl IntoIterator<Item = &'a str>,
) -> String {
    let taken: std::collections::HashSet<String> =
        taken.into_iter().map(str::to_lowercase).collect();
    let mut number = 1usize;
    loop {
        let name = if number == 1 {
            format!("{stem}.{ending}")
        } else {
            format!("{stem}-{number}.{ending}")
        };
        if !taken.contains(&name.to_lowercase()) {
            return name;
        }
        number += 1;
    }
}

/// Why the two folders of the receipt inbox can't be used together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldersRefused {
    /// The inbox and the archive are one folder, one lies inside the other,
    /// or one is KassenSturz's own data folder or the book's folder.
    Nested,
}

/// Checks the inbox and the archive the user chose against each other and
/// against KassenSturz's own folders (`own`: its data folder and the
/// book's folder). All paths are compared as given, so the caller resolves
/// links first. An archive inside the inbox would be read as an inbox
/// again; an inbox inside the archive would file bills into itself.
pub fn check_folders(inbox: &Path, archive: &Path, own: &[&Path]) -> Result<(), FoldersRefused> {
    let overlap = |a: &Path, b: &Path| a.starts_with(b) || b.starts_with(a);
    if overlap(inbox, archive)
        || own
            .iter()
            .any(|folder| inbox == *folder || archive == *folder)
    {
        return Err(FoldersRefused::Nested);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const ENDINGS: [&str; 4] = ["pdf", "png", "jpg", "jpeg"];

    fn day() -> Date {
        Date::constant(2026, 11, 5)
    }

    #[test]
    fn a_bill_goes_under_its_account_and_category_with_the_day_first() {
        let place = ArchivePlace::new(
            Path::new("/home/sam/Archive"),
            "Checking account",
            Some("Groceries"),
            "Not sorted yet",
            day(),
            "RE-0042",
            "PDF",
            &ENDINGS,
        )
        .unwrap();
        assert_eq!(place.stem, "261105_RE-0042");
        assert_eq!(place.ending, "pdf");
        assert_eq!(
            place.folder(Path::new("/home/sam/Archive")),
            PathBuf::from("/home/sam/Archive/Checking account/Groceries")
        );
        let unsorted = ArchivePlace::new(
            Path::new("/a"),
            "Cash",
            None,
            "Not sorted yet",
            day(),
            "x",
            "jpg",
            &ENDINGS,
        )
        .unwrap();
        assert_eq!(unsorted.category, "Not sorted yet");
    }

    #[test]
    fn an_ending_that_is_not_a_bills_is_refused() {
        let place = ArchivePlace::new(Path::new("/a"), "A", None, "U", day(), "x", "exe", &ENDINGS);
        assert_eq!(place, Err(PlaceError::Ending));
    }

    #[test]
    fn names_that_climb_or_hide_are_made_plain() {
        assert_eq!(sanitise("../../etc"), "etc");
        assert_eq!(sanitise(".."), EMPTY_NAME);
        assert_eq!(sanitise("  . "), EMPTY_NAME);
        assert_eq!(sanitise("a/b\\c:d*e?f\"g<h>i|j"), "abcdefghij");
        assert_eq!(sanitise("tab\there\u{7}"), "tabhere");
        assert_eq!(sanitise("CON"), "CON_");
        assert_eq!(sanitise("com1.pdf"), "com1_.pdf");
        assert_eq!(sanitise("nul .txt"), "nul_.txt");
        assert_eq!(sanitise("Console"), "Console");
        assert_eq!(sanitise("name. "), "name");
        assert_eq!(
            sanitise(&"x".repeat(200)).chars().count(),
            MAX_COMPONENT_CHARS
        );
        // Cutting never leaves a trailing dot or space.
        let dotted = format!("{}. tail", "y".repeat(MAX_COMPONENT_CHARS - 1));
        assert!(!sanitise(&dotted).ends_with('.'));
    }

    #[test]
    fn a_taken_name_gets_a_number_also_when_only_the_case_differs() {
        assert_eq!(free_file_name("261105_RE", "pdf", []), "261105_RE.pdf");
        assert_eq!(
            free_file_name("261105_RE", "pdf", ["261105_re.PDF"]),
            "261105_RE-2.pdf"
        );
        assert_eq!(
            free_file_name("a", "pdf", ["a.pdf", "A-2.pdf", "other.pdf"]),
            "a-3.pdf"
        );
    }

    #[test]
    fn a_long_archive_cuts_the_longest_part_first_and_refuses_when_nothing_fits() {
        let deep = PathBuf::from(format!("/{}", "d".repeat(120)));
        let place = ArchivePlace::new(
            &deep,
            &"a".repeat(80),
            Some(&"c".repeat(80)),
            "U",
            day(),
            &"n".repeat(80),
            "pdf",
            &ENDINGS,
        )
        .unwrap();
        let full = place
            .folder(&deep)
            .join(format!("{}-999.{}", place.stem, place.ending));
        assert!(full.to_string_lossy().chars().count() < MAX_PATH_CHARS);
        let too_deep = PathBuf::from(format!("/{}", "d".repeat(230)));
        assert_eq!(
            ArchivePlace::new(&too_deep, "a", None, "U", day(), "n", "pdf", &ENDINGS),
            Err(PlaceError::TooLong)
        );
    }

    #[test]
    fn the_two_folders_may_not_overlap_or_be_kassensturz_own() {
        let inbox = Path::new("/home/sam/Inbox");
        let archive = Path::new("/home/sam/Archive");
        let data = Path::new("/home/sam/.local/share/kassensturz");
        assert_eq!(check_folders(inbox, archive, &[data]), Ok(()));
        assert_eq!(
            check_folders(inbox, inbox, &[]),
            Err(FoldersRefused::Nested)
        );
        assert_eq!(
            check_folders(inbox, &inbox.join("Filed"), &[]),
            Err(FoldersRefused::Nested)
        );
        assert_eq!(
            check_folders(&archive.join("New"), archive, &[]),
            Err(FoldersRefused::Nested)
        );
        assert_eq!(
            check_folders(data, archive, &[data]),
            Err(FoldersRefused::Nested)
        );
        // A neighbour whose name starts the same is no overlap.
        assert_eq!(
            check_folders(inbox, Path::new("/home/sam/Inbox2"), &[]),
            Ok(())
        );
    }

    proptest! {
        #[test]
        fn any_names_give_a_path_inside_the_archive_within_the_limits(
            account in "\\PC{0,120}",
            category in proptest::option::of("\\PC{0,120}"),
            name in "\\PC{0,120}",
            ending in proptest::sample::select(ENDINGS.to_vec()),
        ) {
            let archive = Path::new("/home/sam/Archive");
            let place = ArchivePlace::new(archive, &account, category.as_deref(), "Not sorted yet", day(), &name, ending, &ENDINGS).unwrap();
            for part in [&place.account, &place.category, &place.stem] {
                prop_assert!(!part.is_empty());
                prop_assert!(part.chars().count() <= MAX_COMPONENT_CHARS);
                prop_assert!(part != "." && part != "..");
                prop_assert!(!part.contains(['/', '\\']));
                prop_assert!(!part.chars().any(char::is_control));
                prop_assert!(!is_reserved(part));
                prop_assert!(!part.ends_with('.') && !part.ends_with(' '));
            }
            let full = place.folder(archive).join(free_file_name(&place.stem, &place.ending, []));
            prop_assert!(full.starts_with(archive));
            prop_assert_eq!(full.components().count(), archive.components().count() + 3);
            prop_assert!(full.to_string_lossy().chars().count() + NUMBER_ROOM < MAX_PATH_CHARS);
        }
    }
}
