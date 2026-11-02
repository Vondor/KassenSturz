// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Hostile chart descriptions through the real UI path (M6e.4, T-38): the
//! panel's check (`chart_spec::check`), then the object QML reads
//! (`view_object`), read back the way QML reads it; and hostile files
//! through `chart_spec::read_file`. Every description here is written by
//! hand (D-280). The parser's own cases and its time and memory bounds are in
//! `kassensturz-report/tests/hostile_specs.rs`.

use cxx_qt_lib::{QString, QUrl};
use kassensturz_report::ReportError;
use kassensturz_report::spec::MAX_SPEC_BYTES;

use crate::book_session::local_path;
use crate::chart_spec::{self, Check};
use crate::chart_spec_model::view_object;
use crate::variant::read_back::{flag_of, keys, list_of, number_of, object_of, text_of};

/// A bar chart of spending by category, written for these tests.
const BAR: &str = r#"{"data":{"name":"spending_by_category"},"mark":"bar","encoding":{"x":{"field":"category","type":"nominal"},"y":{"field":"total","type":"quantitative"}}}"#;

/// `BAR` with `rest` added at the end of its top level.
fn bar_with(rest: &str) -> String {
    format!("{},{rest}}}", &BAR[..BAR.len() - 1])
}

/// What the panel shows for a refused text: the code and its reason or
/// limit, read back as QML reads them.
fn refused(text: &str) -> (String, String) {
    let view = view_object(&chart_spec::check(text), "", 0);
    assert!(!flag_of(&view, "ok"), "{text:.60}");
    assert!(!flag_of(&view, "canDraw"));
    assert_eq!(number_of(&view, "count"), 0);
    assert!(list_of(&view, "unsupported").is_empty());
    assert_eq!(text_of(&view, "dataset"), "");
    let error = object_of(&view, "error");
    let code = text_of(&error, "code");
    let detail = if code == "report.spec-too-big" {
        text_of(&error, "limit")
    } else {
        format!("{} {}", text_of(&error, "reason"), text_of(&error, "path"))
            .trim_end()
            .to_owned()
    };
    (code, detail)
}

/// What the panel lists for a kept text: whether it can draw, and the parts
/// as `path reason`.
fn listed(text: &str) -> (bool, Vec<String>) {
    let view = view_object(&chart_spec::check(text), "", 0);
    assert!(flag_of(&view, "ok"), "{text:.60}");
    assert!(!keys(&view).contains(&"error".to_owned()));
    let parts: Vec<String> = list_of(&view, "unsupported")
        .iter()
        .map(|part| format!("{} {}", text_of(part, "path"), text_of(part, "reason")))
        .collect();
    assert_eq!(
        usize::try_from(number_of(&view, "count")).ok(),
        Some(parts.len())
    );
    (flag_of(&view, "canDraw"), parts)
}

#[test]
fn a_hostile_paste_is_refused_with_the_code_the_panel_shows() {
    let too_big = "report.spec-too-big".to_owned();
    let invalid = "report.spec-invalid".to_owned();
    for (text, expected) in [
        (
            "[".repeat(MAX_SPEC_BYTES),
            (too_big.clone(), "depth".into()),
        ),
        (
            format!("{BAR}{}", " ".repeat(MAX_SPEC_BYTES + 1 - BAR.len())),
            (too_big.clone(), "bytes".into()),
        ),
        (
            bar_with(r#""mark":"line""#),
            (invalid.clone(), "duplicate-key mark".into()),
        ),
        (
            BAR.replace("spending_by_category", "transactions"),
            (invalid.clone(), "unknown-dataset data.name".into()),
        ),
        (
            BAR.replacen(r#""field":"category""#, r#""field":"iban""#, 1),
            (invalid.clone(), "unknown-field encoding.x.field".into()),
        ),
        (
            bar_with(r#""width":1e3"#),
            (invalid.clone(), "fraction-number width".into()),
        ),
        ("{\"\u{e9}".into(), (invalid.clone(), "not-json".into())),
        (
            bar_with(r#""title":"\ud800""#),
            (invalid, "not-json".into()),
        ),
    ] {
        assert_eq!(refused(&text), expected, "{text:.60}");
    }
}

#[test]
fn what_a_hostile_paste_asks_for_is_listed_and_never_fetched_or_run() {
    // Only a URL: kept, listed, nothing to draw.
    let (draws, parts) = listed(r#"{"data":{"url":"file:///etc/passwd"},"mark":"bar"}"#);
    assert!(!draws);
    assert_eq!(parts, ["data.url data-source"]);
    // Expressions are listed and the rest is drawn only if chosen.
    let (draws, parts) = listed(&bar_with(
        r#""transform":[{"filter":"datum.total > 0"},{"calculate":"1","as":"x"}],"params":[{"name":"p","expr":"now()"}]"#,
    ));
    assert!(draws);
    assert_eq!(
        parts,
        [
            "transform[0].filter expression",
            "transform[1].calculate expression",
            "params composition"
        ]
    );
    // Keys that hide or turn the text reach QML escaped (D-522), so the list
    // shows what was written.
    let (_, parts) = listed(&bar_with("\"\u{202e}fdp.\":1,\"a\\u0000b\":2"));
    assert_eq!(
        parts,
        [r#"["\u202efdp."] option"#, r#"["a\u0000b"] option"#]
    );
    // The largest list there may be: every entry reaches QML.
    let mut text = String::from("{");
    let mut n = 0_u32;
    while text.len() + 16 < MAX_SPEC_BYTES {
        text.push_str(&format!("\"{n:x}\":0,"));
        n += 1;
    }
    text.push_str("\"z\":0}");
    let (draws, parts) = listed(&text);
    assert!(!draws);
    assert_eq!(parts.len(), usize::try_from(n).expect("count") + 1);
    assert_eq!(parts.last().map(String::as_str), Some("z option"));
}

/// Hostile files. Named pipes, links and `/dev/zero` are Unix ones.
#[cfg(unix)]
mod files {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::test_folder::TestFolder;

    /// Reads `path` on another thread and gives up after five seconds, so a read
    /// that blocks fails the test instead of hanging it.
    fn read_within_time(path: std::path::PathBuf) -> Result<String, ReportError> {
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(chart_spec::read_file(&path));
        });
        receive
            .recv_timeout(Duration::from_secs(5))
            .expect("the read returns instead of waiting")
    }

    fn reason_of(result: Result<String, ReportError>) -> &'static str {
        match result {
            Err(ReportError::SpecInvalid { reason, .. }) => reason,
            Err(ReportError::SpecTooBig { limit }) => limit,
            Ok(text) => panic!("read: {text:.60}"),
            Err(other) => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_hostile_file_is_refused_without_waiting_or_reading_it_whole() {
        let folder = TestFolder::new();
        // A named pipe: opening it would wait for a writer forever (M6-B92).
        let pipe = folder.join("pipe.vl.json");
        let made = std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .expect("mkfifo runs");
        assert!(made.success());
        assert_eq!(reason_of(read_within_time(pipe.clone())), "unreadable");
        // A link to it, and a device that never ends.
        let link = folder.join("link.vl.json");
        std::os::unix::fs::symlink(&pipe, &link).expect("a link");
        assert_eq!(reason_of(read_within_time(link)), "unreadable");
        assert_eq!(
            reason_of(read_within_time("/dev/zero".into())),
            "unreadable"
        );
        // Text in another encoding than UTF-8, and UTF-8 that is cut short or
        // written too long.
        for (name, bytes) in [
            ("utf16.json", b"\xff\xfe{\0}\0".as_slice()),
            ("cut.json", b"{\"title\":\"\xe2\x82\"}".as_slice()),
            ("overlong.json", b"{\"title\":\"\xc0\xaf\"}".as_slice()),
            ("surrogate.json", b"{\"title\":\"\xed\xa0\x80\"}".as_slice()),
        ] {
            let path = folder.join(name);
            std::fs::write(&path, bytes).expect("a file");
            assert_eq!(reason_of(read_within_time(path)), "not-utf8", "{name}");
        }
        // A NUL byte is UTF-8: the file is read, and the check refuses the text.
        let path = folder.join("nul.json");
        std::fs::write(&path, b"{\0}").expect("a file");
        let text = read_within_time(path).expect("read");
        let failed = view_object(&chart_spec::check(&text), &text, 1);
        assert_eq!(text_of(&object_of(&failed, "error"), "reason"), "not-json");
        // A file that is a hostile description is read and checked like a paste.
        let path = folder.join("deep.vl.json");
        std::fs::write(&path, "[".repeat(MAX_SPEC_BYTES)).expect("a file");
        let text = read_within_time(path).expect("read");
        assert_eq!(
            refused(&text),
            ("report.spec-too-big".into(), "depth".into())
        );
    }
}

#[test]
fn only_a_local_file_is_opened() {
    for url in [
        "https://example.invalid/chart.vl.json",
        "ftp://example.invalid/chart.vl.json",
        "data:application/json,{}",
        "",
    ] {
        assert_eq!(local_path(&QUrl::from(&QString::from(url))), None, "{url}");
    }
    // The panel then shows `unreadable` and keeps the field as it was.
    let failed = Check {
        refused: Some(ReportError::SpecInvalid {
            reason: "unreadable",
            path: String::new(),
        }),
        dataset: String::new(),
        can_draw: false,
        unsupported: Vec::new(),
    };
    let view = view_object(&failed, "", 0);
    assert_eq!(text_of(&view, "text"), "");
    assert_eq!(text_of(&object_of(&view, "error"), "reason"), "unreadable");
}
