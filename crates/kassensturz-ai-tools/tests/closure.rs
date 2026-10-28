// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The closing checks of M5 (threats T-27, T-29, M5.12): they read the
//! whole repository, so a later change in any crate has to pass them. Only
//! the window's confirm button may make a `Confirmed`, and nothing a model
//! can call, no change set and nothing the bank link can ask for moves
//! money.

#![cfg(feature = "ai")]

use std::fs;
use std::path::{Path, PathBuf};

use kassensturz_ai_tools::{FUNCTIONS, PROPOSE_CHANGES};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository")
}

fn read(path: &Path) -> String {
    // Windows checks files out with CRLF line endings.
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .replace("\r\n", "\n")
}

/// Every file of `folder` with this extension, at any depth.
fn files(folder: &Path, extension: &str, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(folder).unwrap_or_else(|error| panic!("{folder:?}: {error}")) {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            files(&path, extension, found);
        } else if path.extension().is_some_and(|e| e == extension) {
            found.push(path);
        }
    }
}

/// The sources of every crate: `src` and `build.rs`, not `tests`.
fn sources(extension: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let crates = repository().join("crates");
    for entry in fs::read_dir(&crates).expect("the crates") {
        let src = entry.expect("a crate").path().join("src");
        if src.is_dir() {
            files(&src, extension, &mut found);
        }
    }
    assert!(found.len() > 50, "the sources were not found");
    found
}

#[test]
fn only_the_confirm_button_makes_a_confirmation() {
    let root = repository().join("crates");
    let mut callers = Vec::new();
    for file in sources("rs") {
        let relative = file.strip_prefix(&root).expect("under crates").to_owned();
        let calls = read(&file)
            .lines()
            .filter(|line| line.contains("by_the_user(") && !line.trim_start().starts_with("//"))
            .count();
        if calls > 0 {
            callers.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    callers.sort();
    // The definition and its own tests are in the change-set module; the
    // one call outside is the window's confirm (`ai_chat::confirm_in`).
    let allowed = |path: &str| {
        path.starts_with("kassensturz-ai-tools/src/changes")
            || path == "kassensturz-ui-bridge/src/ai_chat.rs"
    };
    let strangers: Vec<_> = callers.iter().filter(|c| !allowed(c)).collect();
    assert!(
        strangers.is_empty(),
        "another place makes a confirmation: {strangers:?}"
    );
    assert!(
        callers
            .iter()
            .any(|c| c == "kassensturz-ui-bridge/src/ai_chat.rs"),
        "the confirm button was not found: {callers:?}"
    );

    // QML and C++ cannot reach it, but must not say they confirm by hand.
    for extension in ["qml", "cpp", "h"] {
        let mut found = Vec::new();
        files(&repository().join("crates"), extension, &mut found);
        for file in found {
            assert!(!read(&file).contains("by_the_user"), "{}", file.display());
        }
    }
}

/// Words that would mean sending money.
const PAYMENT_WORDS: &[&str] = &[
    "send_money",
    "transfer_money",
    "make_payment",
    "pay_",
    "payment",
    "sepa",
    "pain.001",
    "pain.008",
    "direct_debit",
    "direct debit",
    "standing_order",
    "wire",
];

#[test]
fn nothing_a_model_can_call_sends_money() {
    let mut offered: Vec<_> = FUNCTIONS.iter().collect();
    offered.push(&PROPOSE_CHANGES);
    for function in offered {
        let text = format!(
            "{} {} {}",
            function.name, function.description, function.parameters
        )
        .to_lowercase();
        for word in PAYMENT_WORDS {
            assert!(!text.contains(word), "{} mentions {word}", function.name);
        }
    }
    // A change set holds ten kinds of change, none of them a payment (an
    // edit of a transaction's amount moves no money between accounts, D-257; a
    // cash entry only writes down what the user paid, D-258); a new kind
    // fails here and needs a row in the threat model first (D-236).
    let source = read(&repository().join("crates/kassensturz-ai-tools/src/changes.rs"));
    let start = source.find("pub enum ProposedChange").expect("the enum");
    let end = start + source[start..].find("\n}\n").expect("its end");
    let variants: Vec<_> = source[start..end]
        .lines()
        .filter(|line| line.trim_start().starts_with(char::is_uppercase))
        .map(|line| line.trim().split('(').next().unwrap().to_owned())
        .collect();
    assert_eq!(
        variants,
        [
            "NewCategory",
            "EditCategory",
            "NewRule",
            "EditRule",
            "MoveRule",
            "DeleteRule",
            "Categorize",
            "EditTransaction",
            "NewTransaction",
            "DeleteTransaction"
        ]
    );
}

#[test]
fn the_bank_link_can_only_read() {
    let root = repository();

    // The trait: set up and remove a login, list accounts, fetch, and take
    // the registration number typed for this run (D-260), which sends
    // nothing by itself.
    let source = read(&root.join("crates/kassensturz-bank/src/backend.rs"));
    let start = source.find("pub trait Bank {").expect("the trait");
    let end = start + source[start..].find("\n}\n").expect("its end");
    let mut names: Vec<_> = source[start..end]
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("fn "))
        .map(|rest| rest.split(['(', '<']).next().unwrap().to_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "accounts",
            "add_login",
            "availability",
            "directory",
            "fetch",
            "logins",
            "register",
            "remove_login",
            "set_dialogs"
        ]
    );

    // The bindings declare only commands that read.
    let bindings = read(&root.join("crates/kassensturz-aqbanking-sys/src/aqbanking.rs"));
    let commands: Vec<_> = bindings
        .lines()
        .filter_map(|line| line.strip_prefix("pub const AB_Transaction_Command"))
        .map(|rest| rest.split(':').next().unwrap().to_owned())
        .collect();
    assert!(commands.len() >= 8, "{commands:?}");
    for command in commands {
        assert!(
            command == "Unknown" || command == "None" || command.contains("Get"),
            "a command that is not for reading: {command}"
        );
    }
}

#[test]
fn the_book_has_no_place_for_ai_settings_or_consent() {
    // T-24: a book from someone else can neither switch AI on nor carry a
    // consent, because its layout has no place for either. The settings
    // live in the app's file outside the book (D-168).
    let store = repository().join("crates/kassensturz-store/src");
    let mut layouts = vec![store.join("layout.rs")];
    files(&store.join("layout"), "rs", &mut layouts);
    files(&store.join("layout"), "sql", &mut layouts);
    assert!(layouts.len() > 1, "the layouts were not found");
    for file in layouts {
        let text = read(&file).to_lowercase();
        for word in ["consent", "provider", "api_key", "ai_enabled", "mcp"] {
            assert!(!text.contains(word), "{} names {word}", file.display());
        }
    }
}
