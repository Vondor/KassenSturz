// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Small plugin files for the tests (#791): a module in WebAssembly's text
//! format with a manifest section.

use std::fmt::Write as _;

use kassensturz_plugin_api::{MANIFEST_SECTION, THEME_SECTION};
use serde_json::json;

/// A plugin file of `wat` with a manifest of this ID, name and extension
/// points, asking for no permission.
pub(crate) fn file(id: &str, name: &str, points: &[&str], wat: &str) -> Vec<u8> {
    file_asking(id, name, points, &[], wat)
}

/// A plugin file like [`file`] that asks for `permissions`.
pub(crate) fn file_asking(
    id: &str,
    name: &str,
    points: &[&str],
    permissions: &[&str],
    wat: &str,
) -> Vec<u8> {
    let mut bytes = wat::parse_str(wat).unwrap();
    let manifest = json!({
        "manifest_version": 1,
        "id": id,
        "name": name,
        "version": "1.0.0",
        "api": "1.0",
        "extension_points": points,
        "permissions": permissions,
    })
    .to_string();
    push_custom(&mut bytes, MANIFEST_SECTION, manifest.as_bytes());
    bytes
}

/// A theme plugin of this ID and name, with `theme` as its theme section.
pub(crate) fn theme_file(id: &str, name: &str, theme: &serde_json::Value) -> Vec<u8> {
    let mut bytes = file(id, name, &["theme"], "(module)");
    push_custom(&mut bytes, THEME_SECTION, theme.to_string().as_bytes());
    bytes
}

/// Appends a custom section to a module.
fn push_custom(bytes: &mut Vec<u8>, name: &str, data: &[u8]) {
    let mut content = vec![u8::try_from(name.len()).unwrap()];
    content.extend(name.as_bytes());
    content.extend(data);
    bytes.push(0);
    let mut size = content.len();
    loop {
        let byte = u8::try_from(size & 0x7F).unwrap();
        size >>= 7;
        if size == 0 {
            bytes.push(byte);
            break;
        }
        bytes.push(byte | 0x80);
    }
    bytes.extend(content);
}

/// A module whose exports each hand back a fixed answer, or, for `None`,
/// the input itself.
pub(crate) fn answering(calls: &[(&str, Option<&str>)]) -> String {
    let mut wat = String::from(
        r#"(module
            (memory (export "memory") 32)
            (global $next (mut i32) (i32.const 1048576))
            (func (export "ks_alloc") (param $len i32) (result i32)
                (local $at i32)
                (local.set $at (global.get $next))
                (global.set $next (i32.add (global.get $next) (local.get $len)))
                (local.get $at))"#,
    );
    let mut at = 65_536_u32;
    for (name, answer) in calls {
        match answer {
            Some(answer) => {
                let mut data = String::new();
                for byte in answer.bytes() {
                    write!(data, "\\{byte:02x}").unwrap();
                }
                let length = u64::try_from(answer.len()).unwrap();
                write!(
                    wat,
                    r#"(data (i32.const {at}) "{data}")
                    (func (export "{name}") (param i32 i32) (result i64)
                        (i64.const {result}))"#,
                    result = (u64::from(at) << 32) | length,
                )
                .unwrap();
                at += u32::try_from(answer.len()).unwrap() + 16;
            }
            None => write!(
                wat,
                r#"(func (export "{name}") (param $ptr i32) (param $len i32) (result i64)
                    (i64.or
                        (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
                        (i64.extend_i32_u (local.get $len))))"#
            )
            .unwrap(),
        }
    }
    wat.push(')');
    wat
}

/// A plugin importer of files ending in `.bank` that is sure of every file
/// (`recognition`), reading one statement of one entry of -12.34 EUR.
pub(crate) fn bank_importer(recognition: &str) -> Vec<u8> {
    let recognize = format!(r#"{{"recognition": "{recognition}"}}"#);
    file(
        "org.example.bank-text",
        "Example bank text",
        &["importer"],
        &answering(&[
            (
                "ks_importer_info",
                Some(r#"{"extensions": ["bank", "csv"]}"#),
            ),
            ("ks_importer_recognize", Some(&recognize)),
            (
                "ks_importer_read",
                Some(
                    r#"{"statements": [{"account": {}, "currency": "EUR", "entries": [
                        {"status": "booked", "booking_date": "2026-11-05", "amount": "-1234",
                         "other_party_name": "Example Bakery"}]}]}"#,
                ),
            ),
        ]),
    )
}

/// A plugin exporter of `.json` files that writes what it gets.
pub(crate) fn echo_exporter() -> Vec<u8> {
    file(
        "org.example.echo-export",
        "Example echo export",
        &["exporter"],
        &answering(&[
            ("ks_exporter_info", Some(r#"{"extension": "json"}"#)),
            ("ks_exporter_write", None),
        ]),
    )
}
