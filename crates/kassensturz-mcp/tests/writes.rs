// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The write tools of the running program (M5.10, D-216, D-217): offered
//! only while the user turned them on, and every prepared set goes to the
//! app as a request file, never into the book. The app's side here is the
//! real request folder and the real check of the call (`propose`); the
//! window's confirm and apply are tested in `kassensturz-ui-bridge`.

// Only Linux knows where the settings and the request folder are yet.
#![cfg(all(feature = "ai", target_os = "linux"))]

mod common;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Folder, Session};
use kassensturz_ai_tools::requests::{FOLDER_NAME, Inbox, Reply, RequestFolder};
use kassensturz_store::Book;
use serde_json::{Value, json};

/// A call that prepares one new category.
fn bakery() -> Value {
    json!({"new_categories": [{"key": "b", "name": "Bakery"}]})
}

fn tool_names(session: &mut Session) -> Vec<String> {
    let answer = session.request("tools/list", json!({}));
    answer["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name").to_owned())
        .collect()
}

fn request_folder(folder: &Folder) -> std::path::PathBuf {
    folder
        .data()
        .join("io.github.vondor.KassenSturz")
        .join(FOLDER_NAME)
}

fn files_in(path: &Path) -> Vec<String> {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .map(|entry| {
                    entry
                        .expect("entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_write_tools_are_off_until_the_user_turns_them_on() {
    let folder = Folder::new("writes-off");
    folder.switch_write_tools(false);
    let book = folder.book();
    let mut session = Session::start(folder.path(), &book);
    let answer = session.initialize("2025-06-18");
    let instructions = answer["result"]["instructions"].as_str().expect("text");
    assert!(
        instructions.contains("can't change anything"),
        "{instructions}"
    );
    let names = tool_names(&mut session);
    assert!(
        !names.iter().any(|name| name == "propose_changes"),
        "{names:?}"
    );
    let (answer, is_error) = session.call("propose_changes", bakery());
    assert!(is_error);
    assert_eq!(answer, json!({"error": "mcp-writes.off"}));
    assert!(files_in(&request_folder(&folder)).is_empty());

    // Turned on while the server runs: offered at the next look.
    folder.switch_write_tools(true);
    let names = tool_names(&mut session);
    assert_eq!(
        names[names.len() - 2..],
        ["propose_changes", "check_proposed_changes"]
    );
    assert!(session.finish().status.success());
}

#[test]
fn the_user_chooses_which_lists_the_ai_tool_may_use_and_it_is_seen_at_once() {
    let folder = Folder::new("writes-which-lists");
    folder.allow(&[]);
    let book = folder.book();
    let mut session = Session::start(folder.path(), &book);
    session.initialize("2025-06-18");
    let schema_of = |session: &mut Session| {
        let answer = session.request("tools/list", json!({}));
        let tool = answer["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool["name"] == "propose_changes")
            .cloned();
        tool.map(|tool| {
            let mut lists: Vec<String> = tool["inputSchema"]["properties"]
                .as_object()
                .expect("properties")
                .keys()
                .cloned()
                .collect();
            lists.sort();
            lists
        })
    };
    // Nothing chosen yet: only adding is allowed, and of a transaction only
    // what isn't locked at first (the note and the account) may change.
    assert_eq!(
        schema_of(&mut session),
        Some(vec![
            "categorize".into(),
            "edit_transactions".into(),
            "new_categories".into(),
            "new_rules".into()
        ])
    );
    // With those two locked as well, the list of edits is not offered (D-257).
    folder.allow(&["LockNote=true", "LockAccount=true"]);
    let lists = schema_of(&mut session).expect("still offered");
    assert!(
        !lists.contains(&"edit_transactions".to_owned()),
        "{lists:?}"
    );
    assert!(lists.contains(&"categorize".to_owned()), "{lists:?}");
    folder.allow(&[]);
    // A list that is not offered is refused when called anyway.
    let (answer, is_error) = session.call(
        "propose_changes",
        json!({"delete_rules": [{"rule_id": "00000000-0000-0000-0000-000000000001"}]}),
    );
    assert!(is_error);
    assert_eq!(answer["error"], "ai-tools.not-allowed", "{answer}");
    assert_eq!(answer["argument"], "delete_rules", "{answer}");
    assert!(files_in(&request_folder(&folder)).is_empty());

    // The user ticks one more box: seen at the next look, no restart.
    folder.allow(&["AllowDeleteRules=true", "AllowNewRules=false"]);
    let lists = schema_of(&mut session).expect("still offered");
    assert!(lists.contains(&"delete_rules".to_owned()), "{lists:?}");
    assert!(!lists.contains(&"new_rules".to_owned()), "{lists:?}");

    // Nothing allowed: the write tools are not offered at all.
    folder.allow(&[
        "AllowNewCategories=false",
        "AllowNewRules=false",
        "AllowSortTransactions=false",
    ]);
    let names = tool_names(&mut session);
    assert!(
        !names.iter().any(|name| name.contains("propose")),
        "{names:?}"
    );
    let (answer, is_error) = session.call("propose_changes", bakery());
    assert!(is_error);
    assert_eq!(answer, json!({"error": "mcp-writes.off"}));
    assert!(session.finish().status.success());
}

#[test]
fn a_bad_call_is_refused_at_once_and_reaches_no_app() {
    let folder = Folder::new("writes-bad-call");
    folder.switch_write_tools(true);
    let book = folder.book();
    let mut session = Session::start(folder.path(), &book);
    session.initialize("2025-06-18");
    let (answer, is_error) = session.call(
        "propose_changes",
        json!({"categorize": [{"transaction_id": "00000000-0000-0000-0000-00000000000f", "new_category": "x"}]}),
    );
    assert!(is_error);
    assert_eq!(answer["error"], "ai-tools.unknown-transaction", "{answer}");
    assert!(files_in(&request_folder(&folder)).is_empty());
    assert!(session.finish().status.success());
}

#[test]
fn without_a_running_app_the_request_is_taken_back() {
    let folder = Folder::new("writes-no-app");
    folder.switch_write_tools(true);
    let book = folder.book();
    let mut session = Session::start(folder.path(), &book);
    session.initialize("2025-06-18");
    let (answer, is_error) = session.call("propose_changes", bakery());
    assert!(is_error);
    assert_eq!(answer, json!({"error": "mcp-writes.app-not-running"}));
    assert!(files_in(&request_folder(&folder)).is_empty());
    // The book is as it was.
    let book = Book::open_to_read(&book).expect("read");
    assert!(
        !book
            .accounts(true)
            .expect("accounts")
            .iter()
            .any(|account| account.name == "Bakery")
    );
    assert!(session.finish().status.success());
}

/// Plays the running app: looks into the request folder like the window
/// does, checks each call against the book with the real `propose`, says
/// it shows the set, then answers `decision`.
fn app(
    folder: &Path,
    book: &Path,
    decision: Reply,
    done: Arc<AtomicBool>,
) -> Arc<Mutex<Vec<Value>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::clone(&seen);
    let (folder, book) = (folder.to_path_buf(), book.to_path_buf());
    std::thread::spawn(move || {
        let mut inbox = RequestFolder::open(folder).expect("the app's folder");
        while !done.load(Ordering::SeqCst) {
            for request in inbox.take_new(&book) {
                let read = Book::open_to_read(&book).expect("read");
                let proposal = kassensturz_ai_tools::propose(
                    &read,
                    &request.arguments,
                    kassensturz_core::ai_switch::Permissions::all(),
                )
                .expect("proposed");
                inbox
                    .reply(
                        &request.id,
                        &Reply::Shown {
                            changes: proposal.changes.changes().len(),
                        },
                    )
                    .expect("answered");
                calls.lock().expect("calls").push(request.arguments);
                std::thread::sleep(Duration::from_millis(300));
                inbox.reply(&request.id, &decision).expect("answered");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    seen
}

#[test]
fn the_app_shows_the_set_and_the_ai_tool_hears_the_users_decision() {
    let folder = Folder::new("writes-decided");
    folder.switch_write_tools(true);
    let book = folder.book();
    let place = request_folder(&folder);
    let done = Arc::new(AtomicBool::new(false));
    let calls = app(&place, &book, Reply::Declined, Arc::clone(&done));

    let mut session = Session::start(folder.path(), &book);
    let answer = session.initialize("2025-06-18");
    let instructions = answer["result"]["instructions"].as_str().expect("text");
    assert!(
        instructions.contains("Confirm applies them"),
        "{instructions}"
    );
    let (answer, is_error) = session.call("propose_changes", bakery());
    assert!(!is_error, "{answer}");
    assert_eq!(answer["status"], "declined", "{answer}");
    assert_eq!(answer["request"].as_str().map(str::len), Some(32));
    // The app got the call exactly as the AI tool sent it.
    assert_eq!(*calls.lock().expect("calls"), [bakery()]);
    // The final answer was read, so no file is left.
    assert!(files_in(&place).is_empty(), "{:?}", files_in(&place));
    // Only the user's confirm in the window changes the book; nothing did.
    let read = Book::open_to_read(&book).expect("read");
    assert!(
        !read
            .accounts(true)
            .expect("accounts")
            .iter()
            .any(|account| account.name == "Bakery")
    );
    done.store(true, Ordering::SeqCst);

    let (answer, is_error) = session.call(
        "check_proposed_changes",
        json!({"request": answer["request"]}),
    );
    assert!(is_error);
    assert_eq!(answer, json!({"error": "mcp-writes.unknown-request"}));
    assert!(session.finish().status.success());
}

#[test]
fn an_applied_set_is_reported_as_applied() {
    let folder = Folder::new("writes-applied");
    folder.switch_write_tools(true);
    let book = folder.book();
    let done = Arc::new(AtomicBool::new(false));
    app(
        &request_folder(&folder),
        &book,
        Reply::Applied,
        Arc::clone(&done),
    );
    let mut session = Session::start(folder.path(), &book);
    session.initialize("2025-06-18");
    let (answer, is_error) = session.call("propose_changes", bakery());
    done.store(true, Ordering::SeqCst);
    assert!(!is_error, "{answer}");
    assert_eq!(answer["status"], "applied", "{answer}");
    assert!(session.finish().status.success());
}

#[cfg(unix)]
#[test]
fn the_request_folder_is_the_users_only() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new("writes-permissions");
    folder.switch_write_tools(true);
    let book = folder.book();
    let place = request_folder(&folder);
    let mut session = Session::start(folder.path(), &book);
    session.initialize("2025-06-18");
    // The server made the folder for its request.
    session.call("propose_changes", bakery());
    let mode = std::fs::metadata(&place)
        .expect("made")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
    assert!(session.finish().status.success());
}

#[test]
fn the_program_names_the_request_folder_only_with_the_write_tools_on() {
    let folder = Folder::new("writes-folder-flag");
    folder.switch_write_tools(false);
    let refused = common::run(folder.path(), &["--request-folder"]);
    assert_eq!(refused.status.code(), Some(common::REFUSED));
    assert!(!request_folder(&folder).exists());

    folder.switch_write_tools(true);
    let named = common::run(folder.path(), &["--request-folder"]);
    assert!(named.status.success(), "{}", common::stderr(&named));
    let place = request_folder(&folder);
    assert_eq!(
        common::stderr(&named).trim(),
        format!("request folder: {}", place.display())
    );
    assert!(place.is_dir());
}
