// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of writing a dashboard file (M6d.3, D-441, D-443, D-458) with the
//! book's real accounts, categories and groups, made through its own store
//! functions.

use kassensturz_core::{GroupKind, GroupMember};
use serde_json::Value;

use super::tests::{Sample, sample};
use super::*;

/// An ID no book has.
const GONE: &str = "01890000-0000-7000-8000-000000000000";

/// The sample book with the group "Fixed costs" (Rent) and a dashboard that
/// uses accounts, categories and the group in its filter bar, a chart's
/// lists and a table's columns.
fn dashboard() -> (Sample, Document) {
    let mut sample = sample();
    let fixed = sample
        .book
        .add_category_group(
            "Fixed costs",
            GroupKind::Expense,
            &[GroupMember::Category(sample.rent)],
        )
        .expect("group added");
    let text = format!(
        r#"{{
  "format": "kassensturz-dashboard", "version": {VERSION}, "kind": "dashboard",
  "name": "Where the money goes",
  "filters": {{"period": "last-12-months", "accounts": ["{checking}", "{gone}"],
               "categories": ["{food}"]}},
  "components": [
    {{"id": "c1", "type": "chart.donut", "dataset": "spending_by_category",
     "params": {{"categories": ["{groceries}", "{fixed}"], "accounts": ["{savings}"]}},
     "props": {{"title": "Food and fixed"}},
     "frame": {{"col": 0, "row": 0, "cols": 6, "rows": 5}}}},
    {{"id": "c2", "type": "table.rows", "dataset": "spending_by_category",
     "params": {{"columns": [{{"name": "Home", "members": ["{rent}", "{fixed}", "{gone}"]}},
                             {{"name": "Eating", "members": ["{food}"]}}]}},
     "frame": {{"col": 6, "row": 0, "cols": 6, "rows": 5}}}},
    {{"id": "c3", "type": "heading", "props": {{"text": "Ünïcode ✓"}},
     "frame": {{"col": 0, "row": 5, "cols": 12, "rows": 1}}}}
  ]
}}"#,
        checking = sample.checking,
        savings = sample.savings,
        food = sample.food,
        groceries = sample.groceries,
        rent = sample.rent,
        gone = GONE,
    );
    let document = Document::read(&text).expect("the dashboard reads");
    (sample, document)
}

fn write(sample: &Sample, document: &Document) -> DashboardFile {
    write_dashboard_file(&sample.book, document, "KassenSturz 0.261101.8").expect("written")
}

#[test]
fn the_file_names_what_the_dashboard_uses_and_holds_no_id() {
    let (sample, document) = dashboard();
    let file = write(&sample, &document);
    // Printed once from the code (AGENTS.md §9.4), then checked by eye
    // against `docs/DASHBOARD_FILE.md`.
    let expected = r#"{
  "format": "kassensturz-dashboard-file",
  "version": 1,
  "application": "KassenSturz 0.261101.8",
  "references": {
    "ref:1": {
      "kind": "account",
      "name": "Checking account"
    },
    "ref:2": {
      "kind": "category",
      "path": [
        "Food"
      ]
    },
    "ref:3": {
      "kind": "account",
      "name": "Savings"
    },
    "ref:4": {
      "kind": "category",
      "path": [
        "Food",
        "Groceries"
      ]
    },
    "ref:5": {
      "kind": "group",
      "name": "Fixed costs"
    },
    "ref:6": {
      "kind": "category",
      "path": [
        "Rent"
      ]
    }
  },
  "dashboard": {
    "format": "kassensturz-dashboard",
    "version": DOCUMENT_VERSION,
    "kind": "dashboard",
    "name": "Where the money goes",
    "filters": {
      "period": "last-12-months",
      "accounts": [
        "ref:1"
      ],
      "categories": [
        "ref:2"
      ]
    },
    "components": [
      {
        "id": "c1",
        "type": "chart.donut",
        "dataset": "spending_by_category",
        "params": {
          "accounts": [
            "ref:3"
          ],
          "categories": [
            "ref:4",
            "ref:5"
          ],
          "columns": [],
          "grouping": "category",
          "groups": [],
          "series": []
        },
        "props": {
          "legend": true,
          "period": "page",
          "show_hidden": false,
          "title": "Food and fixed"
        },
        "frame": {
          "col": 0,
          "row": 0,
          "cols": 6,
          "rows": 5
        }
      },
      {
        "id": "c2",
        "type": "table.rows",
        "dataset": "spending_by_category",
        "params": {
          "accounts": [],
          "categories": [],
          "columns": [
            {
              "name": "Home",
              "members": [
                "ref:6",
                "ref:5"
              ]
            },
            {
              "name": "Eating",
              "members": [
                "ref:2"
              ]
            }
          ],
          "grouping": "category",
          "groups": []
        },
        "props": {
          "period": "page",
          "show_hidden": false,
          "title": ""
        },
        "frame": {
          "col": 6,
          "row": 0,
          "cols": 6,
          "rows": 5
        }
      },
      {
        "id": "c3",
        "type": "heading",
        "props": {
          "text": "Ünïcode ✓"
        },
        "frame": {
          "col": 0,
          "row": 5,
          "cols": 12,
          "rows": 1
        }
      }
    ]
  }
}
"#;
    // The document's own version, whatever it is now (D-351, M6d.8).
    let expected = expected.replace("DOCUMENT_VERSION", &VERSION.to_string());
    assert_eq!(file.text, expected, "{}", file.text);
    assert_eq!(file.references, 6);
    // The ID in the filter bar and the one in the column.
    assert_eq!(file.left_out, 2);
    let ids = [
        sample.checking,
        sample.savings,
        sample.food,
        sample.groceries,
        sample.rent,
    ];
    for id in ids {
        assert!(!file.text.contains(&id.to_string()), "{id} in the file");
    }
    assert!(!file.text.contains(GONE));
    assert!(!file.text.starts_with('\u{feff}'));
}

#[test]
fn the_same_dashboard_gives_the_same_file_byte_for_byte() {
    let (sample, document) = dashboard();
    let first = write(&sample, &document);
    // Read back from the book's own text, as a later session would.
    let again = Document::read(&document.to_json()).expect("reads");
    assert_eq!(write(&sample, &again), first);
}

#[test]
fn the_files_dashboard_passes_the_documents_own_checks() {
    let (sample, document) = dashboard();
    let file = write(&sample, &document);
    let value: Value = serde_json::from_str(&file.text).expect("JSON");
    let inner = Document::read(&value["dashboard"].to_string()).expect("the document reads");
    assert_eq!(inner.accounts, ["ref:1"]);
    assert_eq!(inner.categories, ["ref:2"]);
    assert_eq!(inner.entries.len(), 3);
    // Every key listed is used, and every key used is listed.
    let references = value["references"].as_object().expect("references");
    for key in references.keys() {
        let quoted = format!("\"{key}\"");
        assert!(file.text.matches(&quoted).count() >= 2, "{key} unused");
    }
}

#[test]
fn a_list_of_only_gone_ids_is_left_out_of_the_filter_bar() {
    let sample = sample();
    let text = format!(
        r#"{{"format": "kassensturz-dashboard", "version": {VERSION}, "kind": "dashboard",
            "name": "Gone", "filters": {{"accounts": ["{GONE}"]}}, "components": []}}"#
    );
    let document = Document::read(&text).expect("reads");
    let file = write(&sample, &document);
    assert_eq!(file.left_out, 1);
    assert_eq!(file.references, 0);
    let value: Value = serde_json::from_str(&file.text).expect("JSON");
    assert_eq!(value["references"], serde_json::json!({}));
    assert!(value["dashboard"].get("filters").is_none(), "{}", file.text);
}

#[test]
fn the_built_in_overview_can_be_saved_without_references() {
    let sample = sample();
    let overview = Document::default_overview().expect("built in");
    let file = write(&sample, &overview);
    assert_eq!((file.references, file.left_out), (0, 0));
    assert!(
        file.text.contains("  \"references\": {},\n"),
        "{}",
        file.text
    );
    assert!(file.text.ends_with("}\n"));
}

#[test]
fn the_application_is_plain_text_on_one_line() {
    let sample = sample();
    let overview = Document::default_overview().expect("built in");
    let long = format!("Kassen\nSturz {}", "9".repeat(200));
    let file = write_dashboard_file(&sample.book, &overview, &long).expect("written");
    let value: Value = serde_json::from_str(&file.text).expect("JSON");
    let application = value["application"].as_str().expect("text");
    assert!(application.starts_with("KassenSturz 999"));
    assert_eq!(application.chars().count(), MAX_APPLICATION_CHARS);
}

#[test]
fn the_suggested_name_is_one_a_file_system_takes() {
    assert_eq!(
        dashboard_file_name("Where the money goes"),
        "Where the money goes.dashboard.json"
    );
    assert_eq!(
        dashboard_file_name("Rent / Food: 50%?"),
        "Rent _ Food_ 50%_.dashboard.json"
    );
    assert_eq!(dashboard_file_name(".."), "_.dashboard.json");
    assert_eq!(dashboard_file_name("a\tb"), "a_b.dashboard.json");
}

/// The round trip (D-442, the second of M6d.3 and M6d.4 to merge): M6d.3's
/// real file, read by M6d.4's reader into the same book, gives the same
/// dashboard with the same IDs, less the ID the book no longer had.
#[test]
fn a_saved_file_reads_back_into_the_same_book_as_the_same_dashboard() {
    let (sample, document) = dashboard();
    let file = write(&sample, &document);
    let incoming = IncomingDashboardFile::read(&file.text).expect("the reader takes it");
    assert_eq!(incoming.name(), "Where the money goes");
    let names = BookNames::read(&sample.book).expect("names");
    let resolved = incoming.resolve(&names).expect("resolved");
    assert!(resolved.missing.is_empty(), "{:?}", resolved.missing);
    assert!(resolved.shows_more.is_empty(), "{:?}", resolved.shows_more);
    assert_eq!(resolved.found.len(), file.references);
    // The gone ID is never first in its list.
    let without_gone = document.to_json().replace(&format!(",\"{GONE}\""), "");
    let expected = Document::read(&without_gone).expect("reads");
    assert_eq!(resolved.document, expected);
}

/// The same file read into another book with the same names but other IDs,
/// which lacks the group: the names find the new IDs, and the group is
/// listed and left out, never guessed.
#[test]
fn a_saved_file_reads_into_another_book_by_name_and_lists_what_it_lacks() {
    let (from, document) = dashboard();
    let file = write(&from, &document);
    let other = sample();
    let incoming = IncomingDashboardFile::read(&file.text).expect("the reader takes it");
    let resolved = incoming
        .resolve(&BookNames::read(&other.book).expect("names"))
        .expect("resolved");
    let missing: Vec<&Reference> = resolved
        .missing
        .iter()
        .map(|name| &name.reference)
        .collect();
    assert_eq!(missing, [&Reference::Group("Fixed costs".into())]);
    let inner = &resolved.document;
    assert_eq!(inner.accounts, [other.checking.to_string()]);
    assert_eq!(inner.categories, [other.food.to_string()]);
    let Item::Component(table) = &inner.entries[1].item else {
        panic!("a table")
    };
    let Some(Setting::Columns(columns)) = table.get("columns") else {
        panic!("columns")
    };
    assert_eq!(columns[0].members, [other.rent.to_string()]);
    assert_eq!(columns[1].members, [other.food.to_string()]);
    // No ID of the first book reached the second.
    let text = inner.to_json();
    for id in [from.checking, from.food, from.rent, from.groceries] {
        assert!(!text.contains(&id.to_string()), "{id}");
    }
}
