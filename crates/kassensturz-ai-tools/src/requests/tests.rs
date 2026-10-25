// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::json;

use super::*;

/// A folder for one test, removed afterwards.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "kassensturz-requests-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("test folder");
        Self(path)
    }

    /// A book file (any file will do: requests name it, nothing opens it).
    fn book(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, b"book").expect("book written");
        path
    }

    fn folder(&self) -> RequestFolder {
        RequestFolder::open(self.0.join("data").join(FOLDER_NAME)).expect("folder opened")
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.0.join("data").join(FOLDER_NAME))
            .expect("folder read")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .into_string()
                    .expect("UTF-8")
            })
            .collect();
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn call() -> Value {
    json!({"new_categories": [{"key": "b", "name": "Bakery"}]})
}

#[test]
fn a_request_goes_from_the_server_to_the_app_and_its_answers_come_back() {
    let scratch = Scratch::new("round-trip");
    let book = scratch.book("Household.ksz");
    let server = scratch.folder();
    let mut app = scratch.folder();

    let id = server.send(&book, &call()).expect("sent");
    assert_eq!(id.as_str().len(), 32);
    assert_eq!(server.standing(&id), Standing::NotTaken);

    let taken = app.take_new(&book);
    assert_eq!(
        taken,
        [Request {
            id: id.clone(),
            book: book.clone(),
            arguments: call(),
        }]
    );
    assert_eq!(server.standing(&id), Standing::Taken);
    // Taken once: the next look finds nothing new.
    assert!(app.take_new(&book).is_empty());
    // An app took it, so the server can't withdraw it any more.
    assert!(!server.withdraw(&id).expect("withdrawn"));

    app.reply(&id, &Reply::Shown { changes: 1 })
        .expect("answered");
    assert_eq!(
        server.standing(&id),
        Standing::Answered(Reply::Shown { changes: 1 })
    );
    app.reply(&id, &Reply::Applied).expect("answered");
    assert_eq!(server.standing(&id), Standing::Answered(Reply::Applied));

    server.finish(&id);
    assert_eq!(server.standing(&id), Standing::Unknown);
    assert!(scratch.names().is_empty(), "{:?}", scratch.names());
}

#[test]
fn every_answer_reads_back_as_written() {
    for reply in [
        Reply::Shown { changes: 3 },
        Reply::Applied,
        Reply::Declined,
        Reply::Failed {
            code: "ai-changes.out-of-date".into(),
        },
    ] {
        assert_eq!(Reply::from_json(&reply.to_json()), Some(reply.clone()));
        assert_eq!(
            reply.is_final(),
            !matches!(reply, Reply::Shown { .. }),
            "{reply:?}"
        );
    }
    assert_eq!(Reply::from_json(&json!({"status": "maybe"})), None);
    assert_eq!(Reply::from_json(&json!({"status": "shown"})), None);
    assert_eq!(Reply::from_json(&json!("applied")), None);
}

#[test]
fn a_request_withdrawn_before_any_app_took_it_is_gone() {
    let scratch = Scratch::new("withdrawn");
    let book = scratch.book("Household.ksz");
    let server = scratch.folder();
    let mut app = scratch.folder();
    let id = server.send(&book, &call()).expect("sent");
    assert!(server.withdraw(&id).expect("withdrawn"));
    assert_eq!(server.standing(&id), Standing::Unknown);
    assert!(app.take_new(&book).is_empty());
}

#[test]
fn an_app_with_another_book_leaves_the_request_for_the_right_one() {
    let scratch = Scratch::new("other-book");
    let book = scratch.book("Household.ksz");
    let other = scratch.book("Other.ksz");
    let server = scratch.folder();
    let mut other_app = scratch.folder();
    let mut app = scratch.folder();
    let id = server.send(&book, &call()).expect("sent");

    assert!(other_app.take_new(&other).is_empty());
    assert_eq!(server.standing(&id), Standing::NotTaken);
    // The same book by another path is the same book.
    let roundabout = scratch.0.join(".").join("Household.ksz");
    assert_eq!(app.take_new(&roundabout).len(), 1);
}

#[test]
fn only_one_of_two_apps_takes_a_request() {
    let scratch = Scratch::new("two-apps");
    let book = scratch.book("Household.ksz");
    let server = scratch.folder();
    let mut first = scratch.folder();
    let mut second = scratch.folder();
    server.send(&book, &call()).expect("sent");
    assert_eq!(first.take_new(&book).len(), 1);
    assert!(second.take_new(&book).is_empty());
}

#[test]
fn a_request_that_cant_be_read_is_answered_as_bad() {
    let scratch = Scratch::new("bad");
    let book = scratch.book("Household.ksz");
    let server = scratch.folder();
    let mut app = scratch.folder();
    let bad = |text: &[u8]| {
        let id = RequestId::new();
        fs::write(server.path().join(id.file(REQUEST)), text).expect("written");
        id
    };
    let not_json = bad(b"{not json");
    let other_version = bad(json!({"version": 2, "book": "x", "arguments": {}})
        .to_string()
        .as_bytes());
    let too_large = bad(&vec![
        b' ';
        usize::try_from(MAX_REQUEST_BYTES).expect("fits") + 1
    ]);

    assert!(app.take_new(&book).is_empty());
    for id in [not_json, other_version, too_large] {
        assert_eq!(
            server.standing(&id),
            Standing::Answered(Reply::Failed {
                code: "mcp-writes.bad-request".into()
            })
        );
    }
}

#[test]
fn other_names_in_the_folder_are_left_alone() {
    let scratch = Scratch::new("other-names");
    let book = scratch.book("Household.ksz");
    let mut app = scratch.folder();
    for name in ["notes.txt", "ABC.request.json", "1234.request.json"] {
        fs::write(app.path().join(name), b"{}").expect("written");
    }
    fs::create_dir(app.path().join(format!("{}{REQUEST}", "a".repeat(32)))).expect("made");
    assert!(app.take_new(&book).is_empty());
    assert_eq!(
        scratch.names(),
        [
            "1234.request.json",
            "ABC.request.json",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.request.json",
            "notes.txt"
        ]
    );
}

#[test]
fn old_files_are_removed_and_new_ones_kept() {
    let scratch = Scratch::new("old");
    let book = scratch.book("Household.ksz");
    let other = scratch.book("Other.ksz");
    let server = scratch.folder();
    let mut app = scratch.folder();
    let id = server.send(&other, &call()).expect("sent");
    fs::write(app.path().join("notes.txt"), b"mine").expect("written");

    let now = SystemTime::now();
    assert!(app.take_new_at(&book, now).is_empty());
    assert_eq!(server.standing(&id), Standing::NotTaken);

    let later = now + MAX_AGE + Duration::from_secs(60);
    assert!(app.take_new_at(&book, later).is_empty());
    assert_eq!(server.standing(&id), Standing::Unknown);
    // Only its own files.
    assert_eq!(scratch.names(), ["notes.txt"]);
}

#[test]
fn ids_are_only_lowercase_hexadecimal() {
    assert!(RequestId::parse(&"0123456789abcdef".repeat(2)).is_some());
    assert!(RequestId::parse(&"A".repeat(32)).is_none());
    assert!(RequestId::parse(&"a".repeat(31)).is_none());
    assert!(RequestId::parse("../../../../etc/passwd/aaaaaaaaaaa").is_none());
    assert_ne!(RequestId::new(), RequestId::new());
}

#[cfg(unix)]
#[test]
fn only_the_user_may_enter_the_folder_and_read_its_files() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new("permissions");
    let book = scratch.book("Household.ksz");
    let place = scratch.0.join("data").join(FOLDER_NAME);
    fs::create_dir_all(&place).expect("made");
    fs::set_permissions(&place, fs::Permissions::from_mode(0o755)).expect("opened up");

    let server = RequestFolder::open(place.clone()).expect("folder opened");
    let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode(&place), 0o700);
    let id = server.send(&book, &call()).expect("sent");
    assert_eq!(mode(&place.join(id.file(REQUEST))), 0o600);
    let mut app = scratch.folder();
    assert_eq!(app.take_new(&book).len(), 1);
    app.reply(&id, &Reply::Declined).expect("answered");
    assert_eq!(mode(&place.join(id.file(ANSWER))), 0o600);
}

#[cfg(unix)]
#[test]
fn a_link_is_neither_the_folder_nor_a_request() {
    let scratch = Scratch::new("links");
    let book = scratch.book("Household.ksz");
    let elsewhere = scratch.0.join("elsewhere");
    fs::create_dir_all(&elsewhere).expect("made");
    fs::create_dir_all(scratch.0.join("data")).expect("made");
    let place = scratch.0.join("data").join(FOLDER_NAME);
    std::os::unix::fs::symlink(&elsewhere, &place).expect("linked");
    assert!(RequestFolder::open(place.clone()).is_err());
    fs::remove_file(&place).expect("removed");

    let mut app = scratch.folder();
    let target = scratch.0.join("target.json");
    fs::write(
        &target,
        json!({"version": 1, "book": book, "arguments": call()}).to_string(),
    )
    .expect("written");
    let id = RequestId::new();
    std::os::unix::fs::symlink(&target, app.path().join(id.file(REQUEST))).expect("linked");
    assert!(app.take_new(&book).is_empty());
}

#[test]
fn the_default_place_is_in_the_data_folder() {
    if cfg!(target_os = "linux") {
        let place = RequestFolder::default_place().expect("a place on Linux");
        assert!(place.ends_with("io.github.vondor.KassenSturz/mcp-requests"));
    } else {
        assert_eq!(RequestFolder::default_place(), None);
    }
}
