// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! End-to-end built-in chat bill inspection, multimodal image transport,
//! change-set proposal (`attach_bills` / `file_bills`), and permission gating
//! (US10.4, #1415, D-990, D-991, D-997, D-751, D-052, T-21, T-22, T-72).
//!
//! Synthetic fixtures and invented names only (#734).

#![cfg(feature = "ai")]

mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use common::{Script, TODAY, calls, says, settings};
use kassensturz_ai::{ImageMediaType, KeyUse, PreparedRequest, Role};
use kassensturz_ai_chat::{BuildInfo, ChatSession, Hooks, LogEntry, NOT_OFFERED, NoHooks};
use kassensturz_ai_tools::{
    BILL_FOLDER, Folders, INBOX, PROPOSE_CHANGES, ProposedChange, READ_BILL, TotalCheck,
};
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Split, Transaction,
    TransactionId,
};
use kassensturz_store::Book;
use serde_json::{Value, json};

/// Minimal valid 1x1 RGB PNG (synthetic fixture).
const SYNTHETIC_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// Builds a minimal valid 1x1 uncompressed 8-bit grayscale little-endian TIFF.
fn synthetic_tiff() -> Vec<u8> {
    let mut out = Vec::new();
    // Header: little-endian ("II"), magic 42, IFD offset 8.
    out.extend_from_slice(b"II");
    out.extend_from_slice(&42u16.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    let tags: [(u16, u16, u32, u32); 9] = [
        (256, 4, 1, 1),   // ImageWidth = 1
        (257, 4, 1, 1),   // ImageLength = 1
        (258, 3, 1, 8),   // BitsPerSample = 8
        (259, 3, 1, 1),   // Compression = None
        (262, 3, 1, 1),   // PhotometricInterpretation = BlackIsZero
        (273, 4, 1, 122), // StripOffsets = 8 + 2 + 9*12 + 4 = 122
        (277, 3, 1, 1),   // SamplesPerPixel = 1
        (278, 4, 1, 1),   // RowsPerStrip = 1
        (279, 4, 1, 1),   // StripByteCounts = 1
    ];
    out.extend_from_slice(&u16::try_from(tags.len()).unwrap().to_le_bytes());
    for (tag, kind, count, value) in tags {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // Next IFD = 0
    assert_eq!(out.len(), 122);
    out.push(0x80); // 1 grayscale pixel
    out
}

/// Builds a minimal valid PDF 1.4 with embedded Helvetica text lines.
fn synthetic_text_pdf(lines: &[&str]) -> Vec<u8> {
    let escape = |text: &str| {
        text.replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
    };
    let mut stream = String::from("BT\n/F1 12 Tf\n72 760 Td\n16 TL\n");
    for (idx, line) in lines.iter().enumerate() {
        if idx > 0 {
            stream.push_str("T*\n");
        }
        stream.push_str(&format!("({}) Tj\n", escape(line)));
    }
    stream.push_str("ET\n");
    let stream_bytes = stream.into_bytes();
    let mut content_obj = format!("<< /Length {} >>\nstream\n", stream_bytes.len()).into_bytes();
    content_obj.extend_from_slice(&stream_bytes);
    content_obj.extend_from_slice(b"endstream");

    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [ 4 0 R ] /Count 1 >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
           /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R >>"
            .to_vec(),
        content_obj,
    ];
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (idx, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", idx + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Temporary folder layout with `Bills/`, `Inbox/`, and `Archive/`.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "kassensturz-ai-chat-bills-{tag}-{}",
            TransactionId::generate()
        ));
        for dir in ["Bills", "Inbox", "Archive"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        Self { root }
    }

    fn folders(&self) -> Folders {
        Folders {
            bills: Some(self.root.join("Bills")),
            inbox: Some(self.root.join("Inbox")),
            archive: Some(self.root.join("Archive")),
        }
    }

    fn write(&self, rel: &str, bytes: &[u8]) {
        fs::write(self.root.join(rel), bytes).unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Synthetic book with three checking-account payments using invented names.
fn bill_book() -> (Book, TransactionId, TransactionId, TransactionId) {
    let eur = CurrencyCode::new("EUR").unwrap();
    let mut book = Book::create_in_memory().unwrap();
    book.use_currency(eur).unwrap();
    let checking = Account {
        id: AccountId::generate(),
        name: "Everyday Checking".into(),
        kind: AccountKind::Asset,
        currency: eur,
        account_type: Some(AccountType::Checking),
        parent: None,
        closed: false,
        number: None,
        notes: None,
        institution: None,
    };
    let supplies = Account {
        id: AccountId::generate(),
        name: "Supplies".into(),
        kind: AccountKind::Expense,
        account_type: None,
        ..checking.clone()
    };
    book.add_account(&checking).unwrap();
    book.add_account(&supplies).unwrap();

    let mut add_tx = |payee: &str, cents: i64| -> TransactionId {
        let tx = Transaction::new(
            TransactionId::generate(),
            Date::constant(2026, 10, 20),
            Some(payee.into()),
            vec![
                Split::new(checking.id, Money::from_minor_units(-cents, eur)),
                Split::new(supplies.id, Money::from_minor_units(cents, eur)),
            ],
        )
        .unwrap();
        let id = tx.id();
        book.add_transaction(&tx).unwrap();
        id
    };
    let tx_attach = add_tx("Kupferfink Versand", 4_250);
    let tx_match = add_tx("Papierladen Nord", 1_990);
    let tx_mismatch = add_tx("Werkzeugwelt Ost", 3_100);
    (book, tx_attach, tx_match, tx_mismatch)
}

/// Captures both exact wire previews and display previews of each outgoing request.
#[derive(Default)]
struct PreviewRecorder {
    wire_bodies: Arc<Mutex<Vec<String>>>,
    display_bodies: Arc<Mutex<Vec<String>>>,
}

impl Hooks for PreviewRecorder {
    fn review(&mut self, request: &mut PreparedRequest) -> bool {
        self.wire_bodies
            .lock()
            .unwrap()
            .push(request.preview().to_owned());
        self.display_bodies
            .lock()
            .unwrap()
            .push(request.display_preview());
        true
    }
}

#[test]
fn built_in_chat_reads_pdf_and_image_bills_and_proposes_attach_and_file() {
    let sandbox = Sandbox::new("e2e");
    sandbox.write(
        "Bills/RE-2026-42.pdf",
        &synthetic_text_pdf(&[
            "Kupferfink Versand",
            "Invoice RE-2026-42",
            "Bill-only ref 778811",
            "Total 42.50 EUR",
        ]),
    );
    sandbox.write("Inbox/receipt-nord.png", SYNTHETIC_PNG);
    sandbox.write("Inbox/receipt-ost.tiff", &synthetic_tiff());

    let (book, tx_attach, tx_match, tx_mismatch) = bill_book();
    let propose_args = json!({
        "attach_bills": [
            {"transaction_id": tx_attach.to_string(), "file": "RE-2026-42.pdf", "title": "Kupferfink Invoice"}
        ],
        "file_bills": [
            {"transaction_id": tx_match.to_string(), "file": "receipt-nord.png", "bill_total": "19.90", "bill_currency": "EUR"},
            {"transaction_id": tx_mismatch.to_string(), "file": "receipt-ost.tiff", "bill_total": "29.00", "bill_currency": "EUR"},
        ],
    })
    .to_string();

    let (provider, seen) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![
            calls(&[("c1", BILL_FOLDER.name, "{}"), ("c2", INBOX.name, "{}")]),
            calls(&[
                (
                    "c3",
                    READ_BILL.name,
                    r#"{"source":"bill_folder","file":"RE-2026-42.pdf"}"#,
                ),
                (
                    "c4",
                    READ_BILL.name,
                    r#"{"source":"inbox","file":"receipt-nord.png"}"#,
                ),
                (
                    "c5",
                    READ_BILL.name,
                    r#"{"source":"inbox","file":"receipt-ost.tiff"}"#,
                ),
            ]),
            calls(&[("c6", PROPOSE_CHANGES.name, &propose_args)]),
            says(
                "I read the bills (including bill-only ref 778811) and prepared the links and \
                 filings for your review.",
            ),
            // Second turn verifies old bill images are cleared when history is shortened.
            says("All clear."),
        ],
    );

    let mut chat = ChatSession::new(BuildInfo::new("0.261125.0", "abc1234"))
        .with_permissions(Permissions::all())
        .with_folders(sandbox.folders());
    let mut recorder = PreviewRecorder::default();

    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "Check my bill folder and inbox and prepare attaching and filing.",
            TODAY,
            &mut recorder,
        )
        .unwrap();

    // 1. All bill tools were offered on every round.
    let requests = seen.seen();
    assert_eq!(requests.len(), 4);
    for req in &requests {
        assert!(req.offered.iter().any(|name| name == BILL_FOLDER.name));
        assert!(req.offered.iter().any(|name| name == INBOX.name));
        assert!(req.offered.iter().any(|name| name == READ_BILL.name));
        assert!(req.offered.iter().any(|name| name == PROPOSE_CHANGES.name));
    }

    // 2. Round 2 results: PDF text has no images; PNG and TIFF carry PNG image attachments.
    let round3_messages = &requests[2].messages;
    let pdf_msg = round3_messages
        .iter()
        .find(|m| m.role == Role::Tool && m.call_id.as_deref() == Some("c3"))
        .expect("pdf read_bill tool message");
    assert!(pdf_msg.content.contains("Kupferfink Versand"));
    assert!(pdf_msg.content.contains("pdf_text"));
    assert!(pdf_msg.images.is_empty());

    let png_msg = round3_messages
        .iter()
        .find(|m| m.role == Role::Tool && m.call_id.as_deref() == Some("c4"))
        .expect("png read_bill tool message");
    assert!(png_msg.content.contains("receipt-nord.png"));
    assert_eq!(png_msg.images.len(), 1);
    assert_eq!(png_msg.images[0].media_type, ImageMediaType::Png);

    let tiff_msg = round3_messages
        .iter()
        .find(|m| m.role == Role::Tool && m.call_id.as_deref() == Some("c5"))
        .expect("tiff read_bill tool message");
    assert!(tiff_msg.content.contains("receipt-ost.tiff"));
    assert_eq!(tiff_msg.images.len(), 1);
    assert_eq!(tiff_msg.images[0].media_type, ImageMediaType::Png);

    // 3. Outgoing wire request carries `image_url` data URIs, while the
    //    display preview and tool-call log summarize them cleanly (T-22).
    let wire_bodies = recorder.wire_bodies.lock().unwrap();
    let wire_r3: Value = serde_json::from_str(&wire_bodies[2]).unwrap();
    assert!(
        wire_bodies[2].contains("data:image/png;base64,"),
        "wire body must carry multimodal data URI"
    );
    let image_msg = wire_r3["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| {
            m["role"] == "user"
                && m["content"]
                    .as_array()
                    .is_some_and(|parts| parts.iter().any(|p| p["type"] == "image_url"))
        })
        .expect("follow-up user message with image_url parts");
    let image_parts = image_msg["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["type"] == "image_url")
        .count();
    assert_eq!(image_parts, 2, "PNG and converted TIFF both sent");

    let sent_log_r3 = chat
        .log()
        .iter()
        .find_map(|entry| match entry {
            LogEntry::Sent { round: 3, body } => Some(body.as_str()),
            _ => None,
        })
        .expect("round 3 sent log");
    assert!(
        sent_log_r3.contains("[image: image/png,"),
        "tool-call log should summarize image payload: {sent_log_r3}"
    );
    assert!(
        !sent_log_r3.contains("data:image/png;base64,"),
        "tool-call log must not embed raw base64"
    );

    // 4. D-052: numbers from `read_bill` are NOT authoritative book figures.
    assert!(
        answer.unverified.iter().any(|u| u.text == "778811"),
        "bill-only number must stay unverified (D-052): {:?}",
        answer.unverified
    );

    // 5. Prepared ChangeSet: attach_bills + matching file_bill kept; mismatched
    //    file_bill starts left_out (D-751).
    assert_eq!(answer.changes.len(), 1);
    let set = &answer.changes[0];
    assert!(
        set.changes()
            .iter()
            .any(|c| matches!(c, ProposedChange::AttachBill(b) if b.file_name == "RE-2026-42.pdf"))
    );
    assert!(set.changes().iter().any(|c| matches!(
        c,
        ProposedChange::FileBill(b)
            if b.file_name == "receipt-nord.png" && b.check == TotalCheck::Matches
    )));
    let left_out: Vec<_> = set.left_out().collect();
    assert_eq!(left_out.len(), 1, "mismatched total starts unticked");
    assert!(matches!(
        left_out[0].1,
        ProposedChange::FileBill(b)
            if b.file_name == "receipt-ost.tiff" && b.check == TotalCheck::Differs
    ));

    // 6. On the next question, earlier tool results have their images cleared (D-944).
    chat.ask(&book, &provider, None, "Thanks!", TODAY, &mut NoHooks)
        .unwrap();
    let next_req = seen.seen().pop().unwrap();
    assert!(
        next_req.messages.iter().all(|m| m.images.is_empty()),
        "earlier turn bill images must be cleared on follow-up questions"
    );
}

#[test]
fn bill_tools_are_refused_when_permissions_are_off() {
    let sandbox = Sandbox::new("perms-off");
    sandbox.write("Bills/RE-1.pdf", &synthetic_text_pdf(&["Invoice"]));
    sandbox.write("Inbox/receipt.png", SYNTHETIC_PNG);

    let (book, _, _, _) = bill_book();
    let (provider, seen) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![
            calls(&[
                ("c1", BILL_FOLDER.name, "{}"),
                ("c2", INBOX.name, "{}"),
                (
                    "c3",
                    READ_BILL.name,
                    r#"{"source":"bill_folder","file":"RE-1.pdf"}"#,
                ),
            ]),
            says("Please switch on bill permissions in Settings › AI."),
        ],
    );
    let no_bill_perms = Permissions::all()
        .with(Permission::AttachBills, false)
        .with(Permission::FileBills, false);
    let mut chat = ChatSession::new(BuildInfo::new("0.261125.0", "abc1234"))
        .with_permissions(no_bill_perms)
        .with_folders(sandbox.folders());

    let answer = chat
        .ask(
            &book,
            &provider,
            None,
            "Read my bills.",
            TODAY,
            &mut NoHooks,
        )
        .unwrap();
    assert!(answer.changes.is_empty());
    assert!(chat.log().contains(&LogEntry::NotOffered { calls: 3 }));

    let requests = seen.seen();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1]
            .messages
            .iter()
            .any(|m| m.content == NOT_OFFERED)
    );
    for req in &requests {
        assert!(
            req.offered
                .iter()
                .all(|n| n != BILL_FOLDER.name && n != INBOX.name && n != READ_BILL.name)
        );
    }
    let system = &requests[0].messages[0].content;
    assert!(system.contains("Settings › AI"));
    assert!(system.contains("Bills linked to transactions"));
    assert!(system.contains("Bills filed from the inbox"));
    assert!(!system.contains("over MCP"));
}

#[test]
fn per_turn_bill_image_budget_caps_scanned_pages_and_resets_next_turn() {
    let sandbox = Sandbox::new("image-budget");
    for idx in 1..=5 {
        sandbox.write(&format!("Inbox/scan-{idx}.png"), SYNTHETIC_PNG);
    }

    let (book, _, _, _) = bill_book();
    let (provider, seen) = Script::provider(
        settings(true, None, KeyUse::None),
        vec![
            // Turn 1: try to dump 5 scanned receipt PNGs at once; 4 succeed
            // (`MAX_TURN_BILL_IMAGES`) and the 5th is cleanly refused without
            // touching disk or attaching an image.
            calls(&[
                (
                    "c1",
                    READ_BILL.name,
                    r#"{"source":"inbox","file":"scan-1.png"}"#,
                ),
                (
                    "c2",
                    READ_BILL.name,
                    r#"{"source":"inbox","file":"scan-2.png"}"#,
                ),
                (
                    "c3",
                    READ_BILL.name,
                    r#"{"source":"inbox","file":"scan-3.png"}"#,
                ),
                (
                    "c4",
                    READ_BILL.name,
                    r#"{"source":"inbox","file":"scan-4.png"}"#,
                ),
                (
                    "c5",
                    READ_BILL.name,
                    r#"{"source":"inbox","file":"scan-5.png"}"#,
                ),
            ]),
            says("I read the first four scanned bills; say when to continue with scan-5.png."),
            // Turn 2: budget resets cleanly so scan-5.png succeeds.
            calls(&[(
                "c6",
                READ_BILL.name,
                r#"{"source":"inbox","file":"scan-5.png"}"#,
            )]),
            says("I read scan-5.png."),
        ],
    );

    let mut chat = ChatSession::new(BuildInfo::new("0.261127.0", "abc1234"))
        .with_permissions(Permissions::all())
        .with_folders(sandbox.folders());

    chat.ask(
        &book,
        &provider,
        None,
        "Read all my scanned receipts in the inbox.",
        TODAY,
        &mut NoHooks,
    )
    .unwrap();

    let requests = seen.seen();
    let round2 = &requests[1].messages;
    for id in ["c1", "c2", "c3", "c4"] {
        let msg = round2
            .iter()
            .find(|m| m.role == Role::Tool && m.call_id.as_deref() == Some(id))
            .expect("accepted scan tool result");
        assert_eq!(msg.images.len(), 1, "{id} should carry its scanned image");
    }
    let refused = round2
        .iter()
        .find(|m| m.role == Role::Tool && m.call_id.as_deref() == Some("c5"))
        .expect("refused 5th scan tool result");
    assert!(refused.images.is_empty(), "refused call carries no images");
    let refused_json: Value = serde_json::from_str(&refused.content).unwrap();
    assert_eq!(refused_json["error"], json!("ai-tools.bill-too-big"));
    assert_eq!(refused_json["reason"], json!("turn-budget"));
    assert_eq!(refused_json["bills_read"], json!(4));
    assert_eq!(
        refused_json["hint"],
        json!(kassensturz_ai_chat::BILL_BUDGET_HINT)
    );

    // Turn 2 resets the per-turn budget cleanly.
    chat.ask(&book, &provider, None, "Continue.", TODAY, &mut NoHooks)
        .unwrap();
    let turn2_requests = seen.seen();
    let turn2_round2 = &turn2_requests[3].messages;
    let scan5 = turn2_round2
        .iter()
        .find(|m| m.role == Role::Tool && m.call_id.as_deref() == Some("c6"))
        .expect("turn 2 scan-5 result");
    assert_eq!(scan5.images.len(), 1);
    assert!(scan5.content.contains("scan-5.png"));
}
