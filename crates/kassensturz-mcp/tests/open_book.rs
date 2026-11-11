// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The program reads the book the app has open, also when the app opens
//! another book while the AI tool keeps the program running.
//!
//! The case behind the bug: the AI tool started this program while the
//! user's own book was open; the user then tried an example book in the
//! app, and the AI still read the first book.

// Only Linux knows where the settings file is yet (D-177).
#![cfg(all(feature = "ai", target_os = "linux"))]

mod common;

use common::{Folder, Session, account};
use kassensturz_core::{AccountId, AccountKind};
use kassensturz_store::Book;
use serde_json::{Value, json};

/// The names of the accounts `list_accounts` answers with.
fn account_names(session: &mut Session) -> Vec<String> {
    let (answer, error) = session.call("list_accounts", json!({}));
    assert!(!error, "{answer}");
    answer["accounts"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|account: &Value| account["name"].as_str().expect("a name").to_owned())
        .collect()
}

/// Writes the settings as the app leaves them with `book` open.
fn open_in_app(folder: &Folder, book: &str) {
    folder.switch_ai(true);
    let settings = folder.path().join("kassensturzrc");
    let mut text = std::fs::read_to_string(&settings).expect("settings");
    text.push_str(&format!("\n[Books]\nLastBook[$e]=$HOME/{book}\n"));
    std::fs::write(settings, text).expect("settings written");
}

#[test]
fn a_book_opened_in_the_app_later_is_the_one_read() {
    let folder = Folder::new("open-book");
    folder.book();
    let mut example =
        Book::create(&folder.path().join("Example.ksz"), &folder.computer()).expect("book created");
    example.use_currency(common::EUR).expect("currency used");
    example
        .add_account(&account(
            AccountId::from_bytes([9; 16]),
            "Colony fund",
            AccountKind::Asset,
        ))
        .expect("account");
    drop(example);

    open_in_app(&folder, "Household.ksz");
    let mut session = Session::start_without_book(folder.path());
    session.initialize("2025-06-18");
    assert!(account_names(&mut session).contains(&"Checking".to_owned()));

    open_in_app(&folder, "Example.ksz");
    assert_eq!(account_names(&mut session), ["Colony fund"]);
    session.finish();
}

#[test]
fn no_answer_comes_from_a_book_the_settings_no_longer_name() {
    let folder = Folder::new("no-book");
    folder.book();
    open_in_app(&folder, "Household.ksz");
    let mut session = Session::start_without_book(folder.path());
    session.initialize("2025-06-18");
    assert!(account_names(&mut session).contains(&"Checking".to_owned()));

    // The settings no longer name a book, so none is read: not the one
    // read before either.
    folder.switch_ai(true);
    let (answer, error) = session.call("list_accounts", json!({}));
    assert!(error, "{answer}");
    assert_eq!(answer["error"], json!("mcp.no-book"), "{answer}");
    assert!(answer.get("accounts").is_none(), "{answer}");
    session.finish();
}

/// The test folder of the app's test profile, as the app makes it beside
/// the test's data folder (Q-399).
fn test_root(folder: &Folder) -> std::path::PathBuf {
    kassensturz_core::test_profile::root_in(&folder.path().join("data"))
}

/// A test profile as the app leaves it with its own example book open and
/// AI switched on or off there, and the real settings naming the real book
/// with AI on. Returns the test folder.
fn test_profile_with_example(folder: &Folder, ai: bool) -> std::path::PathBuf {
    folder.book();
    open_in_app(folder, "Household.ksz");
    let real = std::fs::read_to_string(folder.path().join("kassensturzrc")).expect("settings");
    let root = test_root(folder);
    let data = root.join("data");
    std::fs::create_dir_all(&data).expect("test data folder");
    std::fs::create_dir_all(root.join("config")).expect("test settings folder");
    let mut example =
        Book::create(&data.join("Example.ksz"), &folder.computer()).expect("book created");
    example.use_currency(common::EUR).expect("currency used");
    example
        .add_account(&account(
            AccountId::from_bytes([9; 16]),
            "Colony fund",
            AccountKind::Asset,
        ))
        .expect("account");
    drop(example);
    let text = real
        .replace("Enabled=true", &format!("Enabled={ai}"))
        .replace(
            "LastBook[$e]=$HOME/Household.ksz",
            &format!("LastBook[$e]={}", data.join("Example.ksz").display()),
        );
    std::fs::write(root.join("config/kassensturzrc"), text).expect("test settings written");
    root
}

#[test]
fn while_a_test_app_runs_its_book_is_read_and_never_the_real_one() {
    let folder = Folder::new("test-app-runs");
    let root = test_profile_with_example(&folder, true);
    let mark = kassensturz_core::test_profile::hold_mark(&root).expect("marked");

    let mut session = Session::start_without_book(folder.path());
    session.initialize("2025-06-18");
    assert_eq!(account_names(&mut session), ["Colony fund"]);

    // The test app ends: nothing is read until the AI tool starts the
    // server again, neither the test book nor the real one.
    drop(mark);
    let (answer, error) = session.call("list_accounts", json!({}));
    assert!(error, "{answer}");
    assert_eq!(answer["error"], json!("mcp.test-app-ended"), "{answer}");
    assert_eq!(
        answer["next_step"],
        json!("restart-this-ai-tool"),
        "{answer}"
    );
    assert!(answer.get("accounts").is_none(), "{answer}");
    session.finish();

    // Started again, it reads the real book.
    let mut session = Session::start_without_book(folder.path());
    session.initialize("2025-06-18");
    assert!(account_names(&mut session).contains(&"Checking".to_owned()));
    session.finish();
}

#[test]
fn a_test_app_started_later_stops_every_answer_from_the_real_book() {
    let folder = Folder::new("test-app-later");
    let root = test_profile_with_example(&folder, true);
    let mut session = Session::start_without_book(folder.path());
    session.initialize("2025-06-18");
    assert!(account_names(&mut session).contains(&"Checking".to_owned()));

    let mark = kassensturz_core::test_profile::hold_mark(&root).expect("marked");
    let (answer, error) = session.call("list_accounts", json!({}));
    assert!(error, "{answer}");
    assert_eq!(answer["error"], json!("mcp.test-app-started"), "{answer}");
    assert_eq!(
        answer["next_step"],
        json!("restart-this-ai-tool"),
        "{answer}"
    );
    assert!(answer.get("accounts").is_none(), "{answer}");
    session.finish();
    drop(mark);
}

#[test]
fn in_the_test_profile_its_own_switch_and_folder_decide() {
    // AI off in the test app: the server doesn't start, although AI is on
    // in the real settings.
    let folder = Folder::new("test-app-ai-off");
    let root = test_profile_with_example(&folder, false);
    let mark = kassensturz_core::test_profile::hold_mark(&root).expect("marked");
    let output = common::run(folder.path(), &[]);
    assert_eq!(output.status.code(), Some(2), "{}", common::stderr(&output));
    drop(mark);

    // The test settings name the real book: it isn't read, nor is it with
    // --book.
    let folder = Folder::new("test-app-real-book");
    let root = test_profile_with_example(&folder, true);
    let settings = root.join("config/kassensturzrc");
    let text = std::fs::read_to_string(&settings).expect("settings");
    let real = folder.path().join("Household.ksz");
    let text = text.replace(
        &format!("{}", root.join("data/Example.ksz").display()),
        &format!("{}", real.display()),
    );
    std::fs::write(&settings, text).expect("settings written");
    let mark = kassensturz_core::test_profile::hold_mark(&root).expect("marked");
    for arguments in [&[][..], &["--book", real.to_str().expect("UTF-8")][..]] {
        let output = common::run(folder.path(), arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            common::stderr(&output).contains("test profile"),
            "{}",
            common::stderr(&output)
        );
    }
    drop(mark);
}

#[test]
fn a_link_in_the_test_folder_to_the_real_book_is_refused() {
    let folder = Folder::new("test-app-link");
    let root = test_profile_with_example(&folder, true);
    let link = root.join("data/Link.ksz");
    std::os::unix::fs::symlink(folder.path().join("Household.ksz"), &link).expect("link");
    let settings = root.join("config/kassensturzrc");
    let text = std::fs::read_to_string(&settings)
        .expect("settings")
        .replace(
            &format!("{}", root.join("data/Example.ksz").display()),
            &format!("{}", link.display()),
        );
    std::fs::write(&settings, text).expect("settings written");
    let mark = kassensturz_core::test_profile::hold_mark(&root).expect("marked");
    let output = common::run(folder.path(), &[]);
    assert_eq!(output.status.code(), Some(2), "{}", common::stderr(&output));
    assert!(common::stderr(&output).contains("test profile"));
    drop(mark);
}

#[test]
fn a_test_folder_named_from_outside_is_refused_unless_every_folder_is_in_it() {
    let folder = Folder::new("test-root-variable");
    folder.book();
    open_in_app(&folder, "Household.ksz");
    // The variable alone, with the real folders left as they are: no book
    // is read from them.
    let root = test_root(&folder);
    let output = common::program(folder.path(), &[])
        .env("KASSENSTURZ_MCP_TEST_ROOT", &root)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("runs");
    assert_eq!(output.status.code(), Some(2), "{}", common::stderr(&output));
    // The whole file system as the test folder, with every folder moved
    // under it.
    let mut command = common::program(folder.path(), &[]);
    command.env("KASSENSTURZ_MCP_TEST_ROOT", "/");
    for (name, value) in kassensturz_core::test_profile::variables(std::path::Path::new("/")) {
        command.env(name, value);
    }
    let output = command
        .stdin(std::process::Stdio::null())
        .output()
        .expect("runs");
    assert_eq!(output.status.code(), Some(2), "{}", common::stderr(&output));
}

/// The AI can read which build of these tools runs, for AI tools that show
/// it neither the handshake's version nor the instructions, and it can even
/// when no book can be read (M6-B225).
#[test]
fn server_info_names_the_build_even_when_no_book_is_read() {
    let folder = Folder::new("server-info");
    folder.book();
    open_in_app(&folder, "Household.ksz");
    let mut session = Session::start_without_book(folder.path());
    let handshake = session.initialize("2025-06-18");
    // The settings no longer name a book, so every book tool refuses.
    folder.switch_ai(true);
    let (refused, error) = session.call("list_accounts", json!({}));
    assert!(error, "{refused}");
    assert_eq!(refused["error"], json!("mcp.no-book"), "{refused}");

    let (answer, error) = session.call("server_info", json!({}));
    assert!(!error, "{answer}");
    assert_eq!(answer["app"], json!("KassenSturz"), "{answer}");
    let version = env!("KASSENSTURZ_BUILD_VERSION");
    assert_eq!(answer["version"], json!(version), "{answer}");
    assert_eq!(
        answer["version"], handshake["result"]["serverInfo"]["version"],
        "the same version as the handshake"
    );
    assert_eq!(answer["pre_release"], json!(true), "{answer}");
    let commit = env!("KASSENSTURZ_BUILD_COMMIT");
    if !commit.is_empty() {
        assert_eq!(answer["commit"], json!(commit), "{answer}");
    }
    session.finish();
}
