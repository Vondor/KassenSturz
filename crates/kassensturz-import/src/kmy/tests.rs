// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The importer as a whole; what it makes of each file is in its golden
//! files (`tests/golden/kmy/`).

use proptest::prelude::*;

use super::*;
use crate::importer::{RECOGNITION_BYTES, import};

/// The project's KMyMoney sample, compressed as KMyMoney saves it (D-102).
const SAMPLE: &[u8] = include_bytes!("../../tests/golden/kmy/kassensturz-sample.kmy");

/// A small book written by hand, uncompressed.
const FEATURES: &[u8] = include_bytes!("../../tests/golden/kmy/features.kmy");

fn start(file: &[u8]) -> &[u8] {
    &file[..file.len().min(RECOGNITION_BYTES)]
}

#[test]
fn a_kmymoney_file_is_recognized_compressed_or_not() {
    assert_eq!(Kmy.recognize(start(SAMPLE)), Recognition::Yes);
    assert_eq!(Kmy.recognize(start(FEATURES)), Recognition::Yes);
    for other in [
        &b""[..],
        b"<?xml version=\"1.0\"?>\n<OFX></OFX>",
        b"{\"format\": \"kassensturz-book\"}",
        &[0x1f, 0x8b],
    ] {
        assert_eq!(Kmy.recognize(other), Recognition::No, "{other:?}");
    }
    let info = Kmy.info();
    assert_eq!(
        (info.id, info.kind, info.extensions),
        ("kmy", ImportKind::Book, &["kmy"][..])
    );
}

#[test]
fn the_sample_is_read_whole() {
    let Ok(Imported::Book(book)) = import(&Kmy, SAMPLE, &Limits::default()) else {
        panic!("the sample is a book");
    };
    assert_eq!(book.accounts.len(), 57);
    assert_eq!(book.transactions.len(), 538);
    let parts: usize = book
        .transactions
        .iter()
        .map(|transaction| transaction.parts.len())
        .sum();
    assert_eq!(parts, 540);
}

#[test]
fn a_book_keeps_the_limits() {
    let limits = Limits {
        max_entries: 5,
        ..Limits::default()
    };
    assert_eq!(
        import(&Kmy, FEATURES, &limits),
        Err(ImportError::TooManyEntries { max: 5 })
    );
    // Unpacked, the sample is about 350 KiB.
    let small = Limits {
        max_file_bytes: 64 * 1024,
        ..Limits::default()
    };
    assert_eq!(
        import(&Kmy, SAMPLE, &small),
        Err(ImportError::FileTooLarge {
            max_bytes: 64 * 1024
        })
    );
}

#[test]
fn a_file_cut_off_anywhere_is_refused_not_misread() {
    let xml = std::str::from_utf8(FEATURES).expect("the file is UTF-8");
    let whole = import(&Kmy, FEATURES, &Limits::default());
    assert!(whole.is_ok());
    // Every fifth place keeps the test quick and still cuts every element
    // and attribute somewhere.
    for (cut, _) in xml.char_indices().skip(1).step_by(5) {
        let result = import(&Kmy, &FEATURES[..cut], &Limits::default());
        // Only whitespace after the root's end may be cut off.
        if xml[cut..].trim().is_empty() {
            assert_eq!(result, whole, "cut at {cut}");
        } else {
            assert!(result.is_err(), "cut at {cut}");
        }
    }
}

proptest! {
    #[test]
    fn no_input_makes_reading_panic(input in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = Kmy.recognize(&input);
        let _ = import(&Kmy, &input, &Limits::default());
    }

    #[test]
    fn no_change_to_a_book_makes_reading_panic(
        at in 0..FEATURES.len(),
        removed in 0_usize..64,
        inserted in proptest::collection::vec(any::<u8>(), 0..16),
    ) {
        let end = (at + removed).min(FEATURES.len());
        let mut input = FEATURES[..at].to_vec();
        input.extend_from_slice(&inserted);
        input.extend_from_slice(&FEATURES[end..]);
        let _ = import(&Kmy, &input, &Limits::default());
    }
}
