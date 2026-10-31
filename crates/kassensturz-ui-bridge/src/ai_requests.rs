// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Changes the user's own AI tools prepare through `kassensturz-mcp`
//! (M5.10, D-218), apart from Qt and apart from how they arrive.
//!
//! A request reaches the window through an [`Inbox`] (today the request
//! folder, D-216). The window never trusts what the AI tool's server
//! checked: it checks the call itself with [`prepare_named`]
//! (`propose_changes` or `propose_view`, D-352) against its own open book,
//! exactly as for a set from its chat, and keeps the set with the
//! chat's sets. From there it is the same set: the same review page, and
//! only its confirm button applies it (`ai_chat::confirm_in`, D-200). The
//! request is answered when the set is shown, and again when the user has
//! confirmed it, thrown it away, or the window forgot it. Nothing here
//! knows about files, so a later socket (Q-124) changes only the
//! [`Inbox`]. This module is compiled only in a build with AI (D-039).

use std::path::Path;

use kassensturz_ai_tools::requests::{Inbox, Reply, RequestId};
use kassensturz_ai_tools::{ChangeError, prepare_named};
use kassensturz_core::Report;
use kassensturz_core::ai_switch::Permissions;
use kassensturz_store::Book;

use crate::ai_chat::Sets;

/// A set that arrived and waits in `sets` for the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrived {
    /// Its ID among the window's sets.
    pub id: u64,
    /// How many changes it holds.
    pub count: usize,
}

/// Takes the requests that arrived for the book at `path`, open as `book`,
/// checks each call against it and keeps the sets in `sets`. Each request
/// is answered: shown, or failed with the check's code. A request for
/// another book is left for the app that has it open.
pub fn take_requests(
    inbox: &mut dyn Inbox,
    path: &Path,
    book: &Book,
    sets: &mut Sets,
    allowed: Permissions,
    today: kassensturz_core::Date,
) -> Vec<Arrived> {
    let mut arrived = Vec::new();
    for request in inbox.take_new(path) {
        let reply = match prepare_named(book, request.function, &request.arguments, allowed, today)
        {
            Ok(proposal) => {
                let count = proposal.changes.changes().len();
                let id = sets.add_requested(proposal.changes, request.id.clone());
                arrived.push(Arrived { id, count });
                Reply::Shown { changes: count }
            }
            Err(error) => failed(error.report().code),
        };
        // An answer that can't be written leaves the AI tool waiting until
        // it gives up; the set is shown all the same.
        let _ = inbox.reply(&request.id, &reply);
    }
    arrived
}

/// Tells the request `from` how the user's confirm ended: applied, or the
/// code of why nothing changed.
pub fn tell_confirmed(inbox: &mut dyn Inbox, from: &RequestId, result: &Result<(), ChangeError>) {
    let reply = match result {
        Ok(()) => Reply::Applied,
        Err(error) => failed(error.report().code),
    };
    let _ = inbox.reply(from, &reply);
}

/// Tells the request `from` that the user threw its set away.
pub fn tell_declined(inbox: &mut dyn Inbox, from: &RequestId) {
    let _ = inbox.reply(from, &Reply::Declined);
}

/// Tells the request `from` that the window forgot its set before the user
/// decided: a new conversation, another book, AI switched off, or the app
/// closing.
pub fn tell_dropped(inbox: &mut dyn Inbox, from: &RequestId) {
    let _ = inbox.reply(from, &failed("mcp-writes.dropped"));
}

fn failed(code: &str) -> Reply {
    Reply::Failed {
        code: code.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use kassensturz_ai_tools::requests::{RequestFolder, Standing};
    use kassensturz_store::{Action, Computer};
    use serde_json::{Value, json};

    use super::*;
    use crate::ai_chat::{ChangeView, confirm_in, views_of};

    /// The day the tests' requests arrive on.
    const DAY: kassensturz_core::Date = kassensturz_core::Date::constant(2026, 10, 25);
    use crate::platform::APP_ID;
    use crate::register::tests::TestLocale;
    use crate::test_folder::TestFolder;
    use kassensturz_ai_tools::propose;

    /// The synthetic Space Colony book, filled by the importer, the
    /// pipeline and the store, in a file as the window has it open.
    fn space_colony(folder: &TestFolder) -> (PathBuf, Book) {
        use kassensturz_import::{Imported, KmySqlite, Limits, import};
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kassensturz-import/tests/golden/kmy-sqlite/space-colony-v15.sqlite");
        let bytes = std::fs::read(source).expect("the Space Colony book is in the repository");
        let Imported::Book(imported) =
            import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
        else {
            panic!("a book");
        };
        let path = folder.join("Space Colony.ksz");
        let computer = Computer {
            name: "Laptop".into(),
            backup_folder: folder.join("backups"),
        };
        let mut book = Book::create(&path, &computer).expect("a book");
        kassensturz_pipeline::fill_book_from(&mut book, &imported, &HashMap::new())
            .expect("filled");
        (path, book)
    }

    /// The call an AI tool makes after a search: a new category, and the
    /// transactions the search found that can get one, by the IDs the real
    /// search result holds.
    fn call_from_search(book: &Book) -> Value {
        let found = kassensturz_ai_tools::call(
            book,
            "search_transactions",
            &json!({"limit": 50}),
            kassensturz_core::Date::constant(2026, 10, 25),
        )
        .expect("searched")
        .value;
        let candidates: Vec<String> = found["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .filter_map(|row| row["transaction_id"].as_str().map(str::to_owned))
            .collect();
        // Only some transactions can get a category; take those the check
        // lets through, as a model would after an error.
        let categorize: Vec<Value> = candidates
            .iter()
            .filter(|id| {
                propose(
                    book,
                    &json!({
                        "new_categories": [{"key": "t", "name": "Sorted by my AI tool"}],
                        "categorize": [{"transaction_id": id, "new_category": "t"}]
                    }),
                    Permissions::all(),
                )
                .is_ok()
            })
            .take(3)
            .map(|id| json!({"transaction_id": id, "new_category": "t"}))
            .collect();
        assert!(!categorize.is_empty(), "the search found nothing to sort");
        json!({
            "new_categories": [{"key": "t", "name": "Sorted by my AI tool"}],
            "categorize": categorize
        })
    }

    struct Ends {
        server: RequestFolder,
        app: RequestFolder,
    }

    fn ends(folder: &TestFolder) -> Ends {
        let place = folder.join("data").join(APP_ID).join("mcp-requests");
        Ends {
            server: RequestFolder::open(place.clone()).expect("the server's folder"),
            app: RequestFolder::open(place).expect("the app's folder"),
        }
    }

    /// The test where the steps meet: the call goes through the request
    /// folder as `kassensturz-mcp` writes it, the window checks it against
    /// the real book, shows the real figures, and the confirm applies it
    /// with the store's real operations as one change undo takes back.
    #[test]
    fn a_request_from_an_ai_tool_is_shown_confirmed_applied_and_answered() {
        let folder = TestFolder::new();
        let (path, mut book) = space_colony(&folder);
        let mut ends = ends(&folder);
        let call = call_from_search(&book);
        let id = ends.server.send(&path, &call).expect("sent");
        let mut sets = Sets::default();

        let arrived = take_requests(
            &mut ends.app,
            &path,
            &book,
            &mut sets,
            Permissions::all(),
            DAY,
        );

        assert_eq!(arrived.len(), 1);
        let count = arrived[0].count;
        assert_eq!(
            ends.server.standing(&id),
            Standing::Answered(Reply::Shown { changes: count })
        );
        let views = views_of(&book, sets.get(arrived[0].id).unwrap(), &TestLocale);
        assert_eq!(views.len(), count);
        assert!(views.iter().any(|view| matches!(
            view,
            ChangeView::NewCategory { name, .. } if name == "Sorted by my AI tool"
        )));
        assert!(views.iter().any(|view| matches!(
            view,
            ChangeView::Categorize { amount, after, .. }
                if amount.ends_with(" €") && after == "Sorted by my AI tool"
        )));
        let history = book.history().unwrap().len();

        // The window's confirm button.
        let waiting = sets.take(arrived[0].id).unwrap();
        assert_eq!(waiting.from.as_ref(), Some(&id));
        let result = confirm_in(&mut book, waiting.set, Permissions::all());
        tell_confirmed(&mut ends.app, &id, &result);

        assert!(result.is_ok());
        assert_eq!(
            ends.server.standing(&id),
            Standing::Answered(Reply::Applied)
        );
        assert_eq!(book.history().unwrap().len(), history + 1);
        assert_eq!(
            book.change_to_undo().unwrap().unwrap().action,
            Action::ApplyAiChanges
        );
        assert!(book.undo().unwrap().is_some());
        assert!(!book.categories().unwrap().iter().any(|entry| {
            entry
                .path
                .last()
                .is_some_and(|name| name == "Sorted by my AI tool")
        }));
    }

    #[test]
    fn a_set_thrown_away_or_forgotten_is_answered_so() {
        let folder = TestFolder::new();
        let (path, book) = space_colony(&folder);
        let mut ends = ends(&folder);
        let call = call_from_search(&book);
        let declined = ends.server.send(&path, &call).expect("sent");
        let forgotten = ends.server.send(&path, &call).expect("sent");
        let mut sets = Sets::default();
        let arrived = take_requests(
            &mut ends.app,
            &path,
            &book,
            &mut sets,
            Permissions::all(),
            DAY,
        );
        assert_eq!(arrived.len(), 2);

        let first = sets.take(arrived[0].id).unwrap();
        assert_eq!(first.from.as_ref(), Some(&declined));
        tell_declined(&mut ends.app, &declined);
        for from in sets.clear() {
            tell_dropped(&mut ends.app, &from);
        }

        assert_eq!(
            ends.server.standing(&declined),
            Standing::Answered(Reply::Declined)
        );
        assert_eq!(
            ends.server.standing(&forgotten),
            Standing::Answered(Reply::Failed {
                code: "mcp-writes.dropped".into()
            })
        );
    }

    #[test]
    fn the_window_checks_the_call_itself_and_shows_nothing_bad() {
        let folder = TestFolder::new();
        let (path, book) = space_colony(&folder);
        let mut ends = ends(&folder);
        // A call the server would have refused, written straight into the
        // folder: the window refuses it too.
        let id = ends
            .server
            .send(
                &path,
                &json!({"categorize": [{"transaction_id": "00000000-0000-0000-0000-00000000000f", "new_category": "x"}]}),
            )
            .expect("sent");
        let mut sets = Sets::default();
        assert!(
            take_requests(
                &mut ends.app,
                &path,
                &book,
                &mut sets,
                Permissions::all(),
                DAY
            )
            .is_empty()
        );
        assert!(sets.clear().is_empty());
        assert_eq!(
            ends.server.standing(&id),
            Standing::Answered(Reply::Failed {
                code: "ai-tools.unknown-transaction".into()
            })
        );
    }

    #[test]
    fn a_request_for_another_book_is_left_alone() {
        let folder = TestFolder::new();
        let (path, book) = space_colony(&folder);
        let mut ends = ends(&folder);
        let call = call_from_search(&book);
        let other = folder.join("Other.ksz");
        std::fs::write(&other, b"another book").unwrap();
        let id = ends.server.send(&other, &call).expect("sent");
        let mut sets = Sets::default();
        assert!(
            take_requests(
                &mut ends.app,
                &path,
                &book,
                &mut sets,
                Permissions::all(),
                DAY
            )
            .is_empty()
        );
        assert_eq!(ends.server.standing(&id), Standing::NotTaken);
    }

    #[test]
    fn the_app_and_the_mcp_server_look_in_the_same_folder() {
        // Both name KassenSturz's data folder by the app ID (D-049).
        assert_eq!(kassensturz_ai_tools::requests::APP_FOLDER, APP_ID);
    }
}
