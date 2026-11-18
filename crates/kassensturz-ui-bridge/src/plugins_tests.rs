// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;

use kassensturz_plugin_api::{ExtensionPoint, MANIFEST_SECTION, Permission};
use serde_json::json;

use super::*;
use crate::test_folder::TestFolder;

/// A module that starts in the sandbox: one memory and `ks_alloc`.
const STARTS: &str = r#"(module
    (memory (export "memory") 1)
    (func (export "ks_alloc") (param i32) (result i32) (i32.const 1024)))"#;

/// A plugin file of `wat` with a manifest.
fn plugin(
    wat: &str,
    id: &str,
    name: &str,
    version: &str,
    points: &[&str],
    permissions: &[&str],
) -> Vec<u8> {
    let mut bytes = wat::parse_str(wat).unwrap();
    let manifest = json!({
        "manifest_version": 1,
        "id": id,
        "name": name,
        "version": version,
        "api": "1.0",
        "extension_points": points,
        "permissions": permissions,
    })
    .to_string();
    let mut content = vec![u8::try_from(MANIFEST_SECTION.len()).unwrap()];
    content.extend(MANIFEST_SECTION.as_bytes());
    content.extend(manifest.as_bytes());
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
    bytes
}

fn importer(id: &str, name: &str, version: &str) -> Vec<u8> {
    plugin(STARTS, id, name, version, &["importer"], &["read_book"])
}

fn ai_provider() -> Vec<u8> {
    plugin(
        STARTS,
        "org.example.ai",
        "Example AI",
        "1.0.0",
        &["ai_provider"],
        &["network"],
    )
}

fn names(listed: &[Listed]) -> Vec<String> {
    listed
        .iter()
        .map(|listed| match &listed.entry {
            Ok(manifest) => manifest.name.clone(),
            Err(error) => format!("{}: {}", listed.file, error.code),
        })
        .collect()
}

#[test]
fn adds_a_copy_named_by_its_id_and_lists_it() {
    let folder = TestFolder::new();
    let plugins = folder.join("plugins");
    let file = importer("org.example.csv", "Example CSV", "1.0.0");
    let (manifest, added) = add(&plugins, &file, false, false).unwrap();
    assert_eq!(added, Added::New);
    assert_eq!(manifest.extension_points, [ExtensionPoint::Importer]);
    assert_eq!(manifest.permissions, [Permission::ReadBook]);
    assert_eq!(
        fs::read(plugins.join("org.example.csv.wasm")).unwrap(),
        file
    );
    let listed = list(&plugins, false);
    assert_eq!(names(&listed), ["Example CSV"]);
    assert_eq!(listed[0].file, "org.example.csv.wasm");
    // Nothing is left behind next to it.
    assert_eq!(fs::read_dir(&plugins).unwrap().count(), 1);
}

#[test]
fn the_same_id_is_replaced_only_when_confirmed() {
    let folder = TestFolder::new();
    let plugins = folder.path();
    add(
        plugins,
        &importer("org.example.csv", "Example CSV", "1.0.0"),
        false,
        false,
    )
    .unwrap();
    let newer = importer("org.example.csv", "Example CSV", "1.1.0");
    let (_, added) = add(plugins, &newer, false, false).unwrap();
    assert_eq!(
        added,
        Added::Replaces {
            installed: PluginVersion::parse("1.0.0")
        }
    );
    // Not confirmed: the old copy stays.
    assert_eq!(
        list(plugins, false)[0]
            .entry
            .as_ref()
            .unwrap()
            .version
            .to_string(),
        "1.0.0"
    );
    add(plugins, &newer, false, true).unwrap();
    let listed = list(plugins, false);
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].entry.as_ref().unwrap().version.to_string(),
        "1.1.0"
    );
}

#[test]
fn a_plugin_that_can_t_start_is_never_added() {
    let folder = TestFolder::new();
    let plugins = folder.join("plugins");
    // No `ks_alloc`.
    let file = plugin(
        r#"(module (memory (export "memory") 1))"#,
        "org.example.broken",
        "Broken",
        "1.0.0",
        &["importer"],
        &[],
    );
    let refused = add(&plugins, &file, false, false).unwrap_err();
    assert_eq!(error_text(&refused).code, "plugin-host.export-missing");
    assert_eq!(
        error_text(&refused).params,
        [("name", "ks_alloc".to_owned())]
    );
    assert!(!plugins.exists());

    let refused = add(&plugins, b"not a plugin", false, false).unwrap_err();
    assert_eq!(error_text(&refused).code, "plugin.not-a-plugin");
}

#[test]
fn ai_plugins_are_hidden_and_refused_while_ai_is_off() {
    let folder = TestFolder::new();
    let plugins = folder.path();
    let refused = add(plugins, &ai_provider(), false, false).unwrap_err();
    assert_eq!(error_text(&refused).code, "plugin-host.ai-off");
    add(plugins, &ai_provider(), true, false).unwrap();
    add(
        plugins,
        &importer("org.example.csv", "Example CSV", "1.0.0"),
        false,
        false,
    )
    .unwrap();
    assert_eq!(names(&list(plugins, true)), ["Example AI", "Example CSV"]);
    assert_eq!(names(&list(plugins, false)), ["Example CSV"]);
}

#[test]
fn lists_by_name_with_unusable_copies_last() {
    let folder = TestFolder::new();
    let plugins = folder.path();
    add(
        plugins,
        &importer("org.example.b", "beta import", "1.0.0"),
        false,
        false,
    )
    .unwrap();
    add(
        plugins,
        &importer("org.example.a", "Alpha import", "1.0.0"),
        false,
        false,
    )
    .unwrap();
    fs::write(plugins.join("damaged.wasm"), b"\0asm cut").unwrap();
    fs::write(plugins.join("notes.txt"), b"not a plugin").unwrap();
    fs::create_dir(plugins.join("folder.wasm")).unwrap();
    assert_eq!(
        names(&list(plugins, false)),
        [
            "Alpha import",
            "beta import",
            "damaged.wasm: plugin.not-a-plugin"
        ]
    );
    assert!(list(&folder.join("missing"), false).is_empty());
}

#[test]
fn removes_only_a_plugin_s_own_copy() {
    let folder = TestFolder::new();
    let plugins = folder.join("plugins");
    add(
        &plugins,
        &importer("org.example.csv", "Example CSV", "1.0.0"),
        false,
        false,
    )
    .unwrap();
    fs::write(folder.join("book.ksz"), b"keep").unwrap();
    for name in [
        "../book.ksz",
        "book.ksz",
        "",
        "/etc/passwd",
        "org.example.csv",
    ] {
        assert!(!remove(&plugins, name), "{name}");
    }
    assert!(folder.join("book.ksz").exists());
    assert!(remove(&plugins, "org.example.csv.wasm"));
    assert!(list(&plugins, false).is_empty());
    assert!(!remove(&plugins, "org.example.csv.wasm"));
}

#[test]
fn would_add_reads_the_installed_version_without_running_it() {
    let folder = TestFolder::new();
    let plugins = folder.path();
    let file = importer("org.example.csv", "Example CSV", "2.0.0");
    let manifest = check(&file, false).unwrap();
    assert_eq!(would_add(plugins, &manifest), Added::New);
    // A copy that only describes itself, without the code a start needs.
    let described_only = plugin(
        "(module)",
        "org.example.csv",
        "Example CSV",
        "1.5.0",
        &["importer"],
        &[],
    );
    fs::write(plugins.join("org.example.csv.wasm"), described_only).unwrap();
    assert_eq!(
        would_add(plugins, &manifest),
        Added::Replaces {
            installed: PluginVersion::parse("1.5.0")
        }
    );
}

#[test]
fn a_file_that_is_too_big_is_not_read_whole() {
    let folder = TestFolder::new();
    let path = folder.join("big.wasm");
    fs::write(&path, vec![0; MAX_PLUGIN_BYTES + 1]).unwrap();
    assert_eq!(
        error_text(&read(&path).unwrap_err()).code,
        "plugin.file-too-big"
    );
    assert_eq!(read(&folder.join("missing.wasm")), Err(Refused::Unreadable));
}
