// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const SMALL: Limits = Limits {
    max_file_bytes: 10,
    max_depth: 3,
    max_entries: 2,
    max_text_bytes: 4,
};

#[test]
fn each_limit_allows_up_to_and_including_its_value() {
    assert_eq!(SMALL.check_file_size(10), Ok(()));
    assert_eq!(
        SMALL.check_file_size(11),
        Err(ImportError::FileTooLarge { max_bytes: 10 })
    );
    assert_eq!(SMALL.check_depth(3, None), Ok(()));
    assert_eq!(
        SMALL.check_depth(4, Some(9)),
        Err(ImportError::TooDeeplyNested {
            max: 3,
            line: Some(9)
        })
    );
    assert_eq!(SMALL.check_entries(2), Ok(()));
    assert_eq!(
        SMALL.check_entries(3),
        Err(ImportError::TooManyEntries { max: 2 })
    );
    assert_eq!(SMALL.check_text("abcd", None), Ok(()));
    assert_eq!(
        SMALL.check_text("abcde", Some(2)),
        Err(ImportError::TextTooLong {
            max_bytes: 4,
            line: Some(2)
        })
    );
}

#[test]
fn text_is_measured_in_bytes() {
    // Two letters, four bytes in UTF-8.
    assert_eq!(SMALL.check_text("ßß", None), Ok(()));
    assert!(SMALL.check_text("ßßß", None).is_err());
}

#[test]
fn the_defaults_leave_room_for_any_household() {
    let limits = Limits::default();
    assert_eq!(limits.max_file_bytes, 128 * 1024 * 1024);
    assert_eq!(limits.max_depth, 64);
    assert_eq!(limits.max_entries, 1_000_000);
    assert_eq!(limits.max_text_bytes, 16 * 1024);
}
