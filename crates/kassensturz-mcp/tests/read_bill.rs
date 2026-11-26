// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! End-to-end MCP parity tests for `bill_folder` bounded listing and
//! `read_bill` (`ContentBlock::text` + `ContentBlock::image`) (US10.5, #1416,
//! D-990, D-751, T-21, T-62, T-66, T-72).
//!
//! Synthetic fixtures and invented names only (#734).

#![cfg(all(feature = "ai", target_os = "linux"))]

mod common;

use std::fs;
use std::path::PathBuf;

use common::{Folder, Session};
use serde_json::{Value, json};

/// Minimal valid 2x2 synthetic RGB PNG.
const SYNTHETIC_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xFD, 0xD4, 0x9A,
    0x73, 0x00, 0x00, 0x00, 0x16, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x10, 0x50, 0x30, 0x70,
    0x08, 0x48, 0x60, 0x28, 0x68, 0x98, 0xB0, 0x60, 0xC3, 0x01, 0x00, 0x18, 0x1E, 0x04, 0xE1, 0xCC,
    0x7D, 0xF2, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// Minimal valid 2x2 synthetic uncompressed RGB TIFF (little-endian).
const SYNTHETIC_TIFF: &[u8] = &[
    0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x00, 0x01, 0x04, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x01, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00,
    0x00, 0x00, 0x02, 0x01, 0x03, 0x00, 0x03, 0x00, 0x00, 0x00, 0x92, 0x00, 0x00, 0x00, 0x03, 0x01,
    0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x11, 0x01, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00, 0x98, 0x00,
    0x00, 0x00, 0x15, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x16, 0x01,
    0x04, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x17, 0x01, 0x04, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, 0x1C, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x28, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x08, 0x00, 0x08, 0x00, 0x08, 0x00, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80,
    0x90, 0xA0, 0xB0, 0xC0,
];

/// Minimal valid 2x2 synthetic baseline JPEG.
const SYNTHETIC_JPEG: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x05, 0x03, 0x04, 0x04, 0x04, 0x03, 0x05,
    0x04, 0x04, 0x04, 0x05, 0x05, 0x05, 0x06, 0x07, 0x0C, 0x08, 0x07, 0x07, 0x07, 0x07, 0x0F, 0x0B,
    0x0B, 0x09, 0x0C, 0x11, 0x0F, 0x12, 0x12, 0x11, 0x0F, 0x11, 0x11, 0x13, 0x16, 0x1C, 0x17, 0x13,
    0x14, 0x1A, 0x15, 0x11, 0x11, 0x18, 0x21, 0x18, 0x1A, 0x1D, 0x1D, 0x1F, 0x1F, 0x1F, 0x13, 0x17,
    0x22, 0x24, 0x22, 0x1E, 0x24, 0x1C, 0x1E, 0x1F, 0x1E, 0xFF, 0xDB, 0x00, 0x43, 0x01, 0x05, 0x05,
    0x05, 0x07, 0x06, 0x07, 0x0E, 0x08, 0x08, 0x0E, 0x1E, 0x14, 0x11, 0x14, 0x1E, 0x1E, 0x1E, 0x1E,
    0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E,
    0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E,
    0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0x1E, 0xFF, 0xC0,
    0x00, 0x11, 0x08, 0x00, 0x02, 0x00, 0x02, 0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11,
    0x01, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
    0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05,
    0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21,
    0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23,
    0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17,
    0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A,
    0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A,
    0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A,
    0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99,
    0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7,
    0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5,
    0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1,
    0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xC4, 0x00, 0x1F, 0x01, 0x00, 0x03,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x11, 0x00,
    0x02, 0x01, 0x02, 0x04, 0x04, 0x03, 0x04, 0x07, 0x05, 0x04, 0x04, 0x00, 0x01, 0x02, 0x77, 0x00,
    0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71, 0x13,
    0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xA1, 0xB1, 0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0, 0x15,
    0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34, 0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19, 0x1A, 0x26, 0x27,
    0x28, 0x29, 0x2A, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88,
    0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6,
    0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4,
    0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE2,
    0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9,
    0xFA, 0xFF, 0xDA, 0x00, 0x0C, 0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3F, 0x00, 0xCC,
    0xA2, 0x8A, 0x2B, 0xF4, 0x53, 0xE1, 0x0F, 0xFF, 0xD9,
];

fn pdf_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'\\' => out.push_str("\\\\"),
            b'(' => out.push_str("\\("),
            b')' => out.push_str("\\)"),
            b => out.push(char::from(b)),
        }
    }
    out
}

fn assemble_pdf(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (idx, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        let id = idx + 1;
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
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

fn synthetic_text_pdf(pages: &[&[&str]]) -> Vec<u8> {
    let mut kids = String::new();
    for i in 0..pages.len() {
        kids.push_str(&format!("{} 0 R ", 4 + 2 * i));
    }
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!("<< /Type /Pages /Kids [ {kids}] /Count {} >>", pages.len()).into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    for (i, lines) in pages.iter().enumerate() {
        let content_obj = 5 + 2 * i;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {content_obj} 0 R >>"
            )
            .into_bytes(),
        );
        let mut stream = String::from("BT\n/F1 12 Tf\n72 760 Td\n16 TL\n");
        for (line_idx, line) in lines.iter().enumerate() {
            if line_idx > 0 {
                stream.push_str("T*\n");
            }
            stream.push_str(&format!("({}) Tj\n", pdf_escape(line)));
        }
        stream.push_str("ET\n");
        let stream_bytes = stream.into_bytes();
        let mut obj = format!("<< /Length {} >>\nstream\n", stream_bytes.len()).into_bytes();
        obj.extend_from_slice(&stream_bytes);
        obj.extend_from_slice(b"endstream");
        objects.push(obj);
    }
    assemble_pdf(&objects)
}

fn synthetic_image_only_pdf(page_count: usize) -> Vec<u8> {
    let img_w = 16usize;
    let img_h = 16usize;
    let mut raw_rgb = Vec::with_capacity(img_w * img_h * 3);
    for y in 0..img_h {
        for x in 0..img_w {
            let r = u8::try_from((x * 15) % 256).unwrap();
            let g = u8::try_from((y * 15) % 256).unwrap();
            raw_rgb.extend_from_slice(&[r, g, 180]);
        }
    }
    let mut kids = String::new();
    for i in 0..page_count {
        kids.push_str(&format!("{} 0 R ", 4 + 2 * i));
    }
    let mut img_obj = format!(
        "<< /Type /XObject /Subtype /Image /Width {img_w} /Height {img_h} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n",
        raw_rgb.len()
    )
    .into_bytes();
    img_obj.extend_from_slice(&raw_rgb);
    img_obj.extend_from_slice(b"\nendstream");

    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!("<< /Type /Pages /Kids [ {kids}] /Count {page_count} >>").into_bytes(),
        img_obj,
    ];
    for i in 0..page_count {
        let content_obj = 5 + 2 * i;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] \
                 /Resources << /XObject << /Im0 3 0 R >> >> /Contents {content_obj} 0 R >>"
            )
            .into_bytes(),
        );
        let stream = b"q 200 0 0 200 50 100 cm /Im0 Do Q\n";
        let mut obj = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        obj.extend_from_slice(stream);
        obj.extend_from_slice(b"endstream");
        objects.push(obj);
    }
    assemble_pdf(&objects)
}

/// Decodes standard base64 so tests can verify MCP `ContentBlock::image` bytes
/// without adding dev-dependencies.
fn decode_base64(input: &str) -> Vec<u8> {
    let val = |byte: u8| -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let bytes = input.as_bytes();
    let (chunks, remainder) = bytes.as_chunks::<4>();
    assert!(
        remainder.is_empty(),
        "base64 length must be a multiple of 4"
    );
    let mut out = Vec::with_capacity(chunks.len() * 3);
    for chunk in chunks {
        let a = val(chunk[0]).expect("base64 digit");
        let b = val(chunk[1]).expect("base64 digit");
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            let c = val(chunk[2]).expect("base64 digit");
            out.push((b << 4) | (c >> 2));
            if chunk[3] != b'=' {
                let d = val(chunk[3]).expect("base64 digit");
                out.push((c << 6) | d);
            }
        }
    }
    out
}

fn assert_valid_image_block(block: &Value) {
    assert_eq!(block["type"], json!("image"), "{block}");
    let mime = block["mimeType"].as_str().expect("mimeType");
    let data = block["data"].as_str().expect("base64 data");
    let decoded = decode_base64(data);
    match mime {
        "image/png" => assert!(decoded.starts_with(b"\x89PNG\r\n\x1a\n"), "valid PNG magic"),
        "image/jpeg" => assert!(decoded.starts_with(b"\xFF\xD8\xFF"), "valid JPEG magic"),
        other => panic!("unexpected image mimeType: {other}"),
    }
}

fn tool_names(session: &mut Session) -> Vec<String> {
    session.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("name").to_owned())
        .collect()
}

fn setup_bill_dirs(folder: &Folder) -> (PathBuf, PathBuf, PathBuf) {
    let bills = folder.path().join("Bills");
    let inbox = folder.path().join("Inbox");
    let archive = folder.path().join("Archive");
    let elsewhere = folder.path().join("Elsewhere");
    for dir in [
        bills.join("2026"),
        inbox.clone(),
        archive.clone(),
        elsewhere.clone(),
    ] {
        fs::create_dir_all(dir).expect("dir");
    }

    let digital_pdf = synthetic_text_pdf(&[&[
        "Copper Finch Goods",
        "Invoice INV-2026-8801",
        "Total: 42.50 EUR",
        "IBAN DE89 3704 0044 0532 0130 00",
    ]]);
    let scanned_pdf = synthetic_image_only_pdf(3);

    fs::write(bills.join("digital.pdf"), &digital_pdf).expect("digital");
    fs::write(bills.join("photo.png"), SYNTHETIC_PNG).expect("png");
    fs::write(bills.join("scan.jpg"), SYNTHETIC_JPEG).expect("jpg");
    fs::write(bills.join("fax.tiff"), SYNTHETIC_TIFF).expect("tiff");
    fs::write(bills.join("notes.txt"), b"not a bill").expect("txt");
    fs::write(bills.join("2026/nested.pdf"), &digital_pdf).expect("nested");
    fs::write(elsewhere.join("secret.pdf"), &digital_pdf).expect("secret");
    std::os::unix::fs::symlink(elsewhere.join("secret.pdf"), bills.join("escape.pdf"))
        .expect("symlink");

    fs::write(inbox.join("scanned-receipt.pdf"), &scanned_pdf).expect("scanned");
    fs::write(inbox.join("slip.tif"), SYNTHETIC_TIFF).expect("tif");

    (bills, inbox, archive)
}

#[test]
fn bill_folder_lists_regular_bills_and_read_bill_returns_text_and_image_blocks() {
    let folder = Folder::new("read-bill-parity");
    let (bills, inbox, archive) = setup_bill_dirs(&folder);
    folder.allow(&[
        "AllowAttachBills=true",
        "AllowFileBills=true",
        &format!("BillFolder={}", bills.display()),
        &format!("InboxFolder={}", inbox.display()),
        &format!("ArchiveFolder={}", archive.display()),
    ]);
    let book = folder.book();
    let mut session = Session::start(folder.path(), &book);
    session.initialize("2025-06-18");

    let names = tool_names(&mut session);
    assert!(names.contains(&"bill_folder".to_owned()), "{names:?}");
    assert!(names.contains(&"inbox".to_owned()), "{names:?}");
    assert!(names.contains(&"read_bill".to_owned()), "{names:?}");

    // 1. `bill_folder` returns bounded non-recursive regular bill listing,
    // including `.tiff` and skipping `.txt`, subdirectories and symlinks.
    let (listed, is_error) = session.call("bill_folder", json!({}));
    assert!(!is_error, "{listed}");
    assert_eq!(
        listed["file_types"],
        json!(["pdf", "png", "jpg", "jpeg", "tif", "tiff"])
    );
    assert_eq!(listed["truncated"], json!(false));
    let listed_names: Vec<&str> = listed["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|entry| entry["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        listed_names,
        ["digital.pdf", "fax.tiff", "photo.png", "scan.jpg"]
    );

    // 2. Digital PDF returns 1 text block (no image blocks), framed as untrusted
    // data with IBANs masked.
    let raw = session.request(
        "tools/call",
        json!({
            "name": "read_bill",
            "arguments": {"source": "bill_folder", "file": "digital.pdf"}
        }),
    );
    assert_ne!(raw["result"]["isError"], json!(true), "{raw}");
    let content = raw["result"]["content"].as_array().expect("content");
    assert_eq!(
        content.len(),
        1,
        "digital PDF returns text block only: {raw}"
    );
    assert_eq!(content[0]["type"], json!("text"));
    let payload: Value =
        serde_json::from_str(content[0]["text"].as_str().expect("text")).expect("JSON");
    assert_eq!(payload["mode"], json!("pdf_text"));
    assert_eq!(payload["source"], json!("bill_folder"));
    assert_eq!(payload["file"], json!("digital.pdf"));
    let text = payload["text"].as_str().expect("extracted text");
    assert!(text.contains("Copper Finch Goods"), "{text}");
    assert!(text.contains("INV-2026-8801"), "{text}");
    assert!(text.contains("[IBAN left out]"), "{text}");
    assert!(!text.contains("DE89"), "{text}");

    // 3. Scanned 3-page PDF from `inbox` returns text + 2 MCP image blocks on
    // page 1 and text + 1 MCP image block on page 3.
    let scanned_p1 = session.request(
        "tools/call",
        json!({
            "name": "read_bill",
            "arguments": {"source": "inbox", "file": "scanned-receipt.pdf"}
        }),
    );
    assert_ne!(scanned_p1["result"]["isError"], json!(true), "{scanned_p1}");
    let p1_content = scanned_p1["result"]["content"].as_array().expect("content");
    assert_eq!(p1_content.len(), 3, "1 text + 2 rendered page images");
    let p1_meta: Value =
        serde_json::from_str(p1_content[0]["text"].as_str().expect("text")).expect("JSON");
    assert_eq!(p1_meta["mode"], json!("pdf_images"));
    assert_eq!(p1_meta["pages_returned"], json!(2));
    assert_eq!(p1_meta["total_pages"], json!(3));
    assert_eq!(p1_meta["has_more_pages"], json!(true));
    assert_valid_image_block(&p1_content[1]);
    assert_valid_image_block(&p1_content[2]);

    let scanned_p3 = session.request(
        "tools/call",
        json!({
            "name": "read_bill",
            "arguments": {"source": "inbox", "file": "scanned-receipt.pdf", "page": 3}
        }),
    );
    assert_ne!(scanned_p3["result"]["isError"], json!(true), "{scanned_p3}");
    let p3_content = scanned_p3["result"]["content"].as_array().expect("content");
    assert_eq!(p3_content.len(), 2, "1 text + 1 remaining page image");
    let p3_meta: Value =
        serde_json::from_str(p3_content[0]["text"].as_str().expect("text")).expect("JSON");
    assert_eq!(p3_meta["pages_returned"], json!(1));
    assert_eq!(p3_meta["has_more_pages"], json!(false));
    assert_valid_image_block(&p3_content[1]);

    // 4. PNG, JPEG and TIFF files return text + 1 valid MCP image block
    // (TIFF converted to PNG/JPEG).
    for (source, file) in [
        ("bill_folder", "photo.png"),
        ("bill_folder", "scan.jpg"),
        ("bill_folder", "fax.tiff"),
        ("inbox", "slip.tif"),
    ] {
        let resp = session.request(
            "tools/call",
            json!({
                "name": "read_bill",
                "arguments": {"source": source, "file": file}
            }),
        );
        assert_ne!(resp["result"]["isError"], json!(true), "{file}: {resp}");
        let blocks = resp["result"]["content"].as_array().expect("content");
        assert_eq!(blocks.len(), 2, "{file}: 1 text + 1 image");
        let meta: Value =
            serde_json::from_str(blocks[0]["text"].as_str().expect("text")).expect("JSON");
        assert_eq!(meta["mode"], json!("image"));
        assert_valid_image_block(&blocks[1]);
    }

    assert!(session.finish().status.success());
}

#[test]
fn read_bill_refuses_when_permissions_are_off_or_path_escapes_folder() {
    let folder = Folder::new("read-bill-refusals");
    let (bills, inbox, archive) = setup_bill_dirs(&folder);
    // Only AttachBills is on; FileBills stays off.
    folder.allow(&[
        "AllowAttachBills=true",
        "AllowFileBills=false",
        &format!("BillFolder={}", bills.display()),
        &format!("InboxFolder={}", inbox.display()),
        &format!("ArchiveFolder={}", archive.display()),
    ]);
    let book = folder.book();
    let mut session = Session::start(folder.path(), &book);
    session.initialize("2025-06-18");

    // Cross-folder permission check: AttachBills must not allow reading inbox.
    let (answer, is_error) = session.call(
        "read_bill",
        json!({"source": "inbox", "file": "scanned-receipt.pdf"}),
    );
    assert!(is_error, "{answer}");
    assert_eq!(answer["error"], json!("ai-tools.not-allowed"), "{answer}");

    // Path traversal, absolute path, escaping symlink, and disallowed extension.
    for (file, expected_error) in [
        ("../Elsewhere/secret.pdf", "ai-tools.bill-outside-folder"),
        ("/etc/passwd", "ai-tools.bill-outside-folder"),
        ("escape.pdf", "ai-tools.bill-outside-folder"),
        ("notes.txt", "ai-tools.bill-type"),
    ] {
        let (answer, is_error) =
            session.call("read_bill", json!({"source": "bill_folder", "file": file}));
        assert!(is_error, "{file}: {answer}");
        assert_eq!(answer["error"], json!(expected_error), "{file}: {answer}");
    }

    // Switching AttachBills off hides `read_bill` from `tools/list` AND
    // refuses direct calls on the running server.
    folder.allow(&[
        "AllowAttachBills=false",
        "AllowFileBills=false",
        &format!("BillFolder={}", bills.display()),
        &format!("InboxFolder={}", inbox.display()),
        &format!("ArchiveFolder={}", archive.display()),
    ]);
    let names = tool_names(&mut session);
    assert!(!names.contains(&"read_bill".to_owned()), "{names:?}");
    let (answer, is_error) = session.call(
        "read_bill",
        json!({"source": "bill_folder", "file": "digital.pdf"}),
    );
    assert!(is_error, "{answer}");
    assert_eq!(answer["error"], json!("ai-tools.not-allowed"), "{answer}");

    assert!(session.finish().status.success());
}
