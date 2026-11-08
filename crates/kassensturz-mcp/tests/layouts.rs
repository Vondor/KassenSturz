// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A book in every layout, read through the running program as an AI tool
//! reads it (M6-B194): the current layout answers, and every other one is
//! refused with the book's layout, the one the program reads and what the
//! user does next, so the AI can tell the user how to fix it.
//!
//! The case behind the bug: the AI tool started this program, KassenSturz
//! was updated, and the new app updated the book while the old program
//! kept running. The layout is changed while the program runs, as the app
//! does, by writing SQLite's `user_version` in the file's header.

// Only Linux knows where the settings file is yet (D-177); elsewhere the
// switch reads as off and the program never starts.
#![cfg(all(feature = "ai", target_os = "linux"))]

mod common;

use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use common::{Folder, Session};
use kassensturz_store::{FIRST_LAYOUT, LAYOUT};
use serde_json::{Value, json};

/// Where SQLite keeps `user_version` in a database file: four bytes, big
/// endian, at this offset of its header (sqlite.org/fileformat.html,
/// "The Database Header").
const USER_VERSION_OFFSET: u64 = 60;

/// Gives the book at `path` the layout number `layout`, as a KassenSturz
/// with that layout leaves it. Every call of the program opens the book
/// again, so it reads the new header.
fn set_layout(path: &Path, layout: i32) {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("book opened");
    file.seek(SeekFrom::Start(USER_VERSION_OFFSET))
        .expect("header");
    file.write_all(&layout.to_be_bytes()).expect("written");
    file.sync_all().expect("written");
}

/// The answer to `list_accounts` and whether it is an error.
fn list_accounts(session: &mut Session) -> (Value, bool) {
    session.call("list_accounts", json!({}))
}

#[test]
fn every_layout_is_read_or_refused_with_the_next_step() {
    let folder = Folder::new("every-layout");
    folder.switch_ai(true);
    let book = folder.book();
    let mut session = Session::start(folder.path(), &book);
    session.initialize("2025-06-18");

    let (answer, failed) = list_accounts(&mut session);
    assert!(!failed, "the current layout {LAYOUT} is read: {answer}");

    // Every older layout a book of the Beta may have: the app updates it.
    for older in FIRST_LAYOUT..LAYOUT {
        set_layout(&book, older);
        let (answer, failed) = list_accounts(&mut session);
        assert!(failed, "layout {older}: {answer}");
        assert_eq!(
            answer,
            json!({
                "error": "ai-tools.book-unreadable",
                "cause": "store.older-layout",
                "found": older,
                "current": LAYOUT,
                "next_step": "open-the-book-in-kassensturz-once",
            }),
            "layout {older}"
        );
    }

    // A newer KassenSturz updated the book: this program is the old one.
    set_layout(&book, LAYOUT + 1);
    let (answer, failed) = list_accounts(&mut session);
    assert!(failed, "{answer}");
    assert_eq!(
        answer,
        json!({
            "error": "ai-tools.book-unreadable",
            "cause": "store.unsupported-layout",
            "found": LAYOUT + 1,
            "current": LAYOUT,
            "next_step": "update-kassensturz-and-restart-this-ai-tool",
        })
    );

    // A development build of the Alpha wrote it.
    set_layout(&book, FIRST_LAYOUT - 1);
    let (answer, failed) = list_accounts(&mut session);
    assert!(failed, "{answer}");
    assert_eq!(answer["cause"], json!("store.development-layout"));
    assert_eq!(
        answer["next_step"],
        json!("save-the-whole-book-with-the-old-build")
    );

    // Back at the current layout, it answers again without a restart.
    set_layout(&book, LAYOUT);
    let (answer, failed) = list_accounts(&mut session);
    assert!(!failed, "{answer}");
    session.finish();
}

#[test]
fn a_book_from_a_newer_kassensturz_asks_for_an_update_at_the_start() {
    let folder = Folder::new("newer-at-start");
    folder.switch_ai(true);
    let book = folder.book();
    set_layout(&book, LAYOUT + 1);
    let output = common::run(folder.path(), &["--book", book.to_str().expect("UTF-8")]);
    assert_eq!(output.status.code(), Some(common::REFUSED));
    let errors = common::stderr(&output);
    assert!(errors.contains("store.unsupported-layout"), "{errors}");
    assert!(
        errors.contains("Update KassenSturz, then restart this AI tool."),
        "{errors}"
    );
}

#[test]
fn the_instructions_explain_every_next_step() {
    let folder = Folder::new("next-steps-explained");
    folder.switch_ai(true);
    let book = folder.book();
    let mut session = Session::start(folder.path(), &book);
    let answer = session.initialize("2025-06-18");
    let text = answer["result"]["instructions"]
        .as_str()
        .expect("instructions");
    for step in [
        "update-kassensturz-and-restart-this-ai-tool",
        "open-the-book-in-kassensturz-once",
        "save-the-whole-book-with-the-old-build",
    ] {
        assert!(text.contains(step), "{step}: {text}");
    }
    session.finish();
}
