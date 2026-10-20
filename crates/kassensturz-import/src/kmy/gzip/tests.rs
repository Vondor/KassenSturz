// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;

use super::*;

fn compressed(content: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(content).expect("written to memory");
    encoder.finish().expect("finished in memory")
}

#[test]
fn gzip_is_uncompressed_and_anything_else_passed_on() {
    let xml = b"<?xml version=\"1.0\"?>\n<ROOT/>\n";
    let limits = Limits::default();
    assert_eq!(
        uncompressed(&compressed(xml), &limits).as_deref(),
        Ok(&xml[..])
    );
    assert!(matches!(
        uncompressed(xml, &limits),
        Ok(std::borrow::Cow::Borrowed(_))
    ));
}

#[test]
fn a_small_file_cant_unpack_past_the_limit() {
    // A megabyte of one letter compresses to about a kilobyte.
    let content = vec![b'x'; 1 << 20];
    let file = compressed(&content);
    let limits = Limits {
        max_file_bytes: 1 << 19,
        ..Limits::default()
    };
    assert!(file.len() < 1 << 19);
    assert_eq!(
        uncompressed(&file, &limits),
        Err(ImportError::FileTooLarge { max_bytes: 1 << 19 })
    );
    let exactly = Limits {
        max_file_bytes: 1 << 20,
        ..Limits::default()
    };
    assert_eq!(
        uncompressed(&file, &exactly).map(|xml| xml.len()),
        Ok(1 << 20)
    );
}

#[test]
fn a_damaged_stream_is_malformed() {
    let mut file = compressed(b"<ROOT/>");
    // The last eight bytes hold the checksum and the length.
    let checksum = file.len() - 8;
    file[checksum] ^= 0xff;
    assert_eq!(
        uncompressed(&file, &Limits::default()),
        Err(ImportError::Malformed { line: None })
    );
    let cut = compressed(b"<ROOT/>");
    assert_eq!(
        uncompressed(&cut[..cut.len() / 2], &Limits::default()),
        Err(ImportError::Malformed { line: None })
    );
}

#[test]
fn the_start_of_a_file_is_enough_to_recognize_it() {
    let content: Vec<u8> = (0..20_000_u32)
        .flat_map(|number| number.to_string().into_bytes())
        .collect();
    let file = compressed(&content);
    let start = &file[..file.len() / 2];
    let uncompressed = uncompressed_start(start, 100).expect("a start");
    assert_eq!(uncompressed, content[..100]);
    assert_eq!(uncompressed_start(b"<ROOT/>", 100), None);
    assert_eq!(uncompressed_start(&MAGIC, 100), None);
}
