// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use kassensturz_plugin_api::{MANIFEST_SECTION, PluginError};
use serde_json::json;

use crate::*;

/// A plugin file: the module written in WebAssembly's text format, with a
/// manifest section for the given extension points and permissions.
fn plugin(wat: &str, points: &[&str], permissions: &[&str]) -> Vec<u8> {
    let mut bytes = wat::parse_str(wat).unwrap();
    let manifest = json!({
        "manifest_version": 1,
        "id": "org.example.test",
        "name": "Test plugin",
        "version": "1.0.0",
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

fn importer(wat: &str) -> Vec<u8> {
    plugin(wat, &["importer"], &[])
}

/// A bump allocator from address 1024 and the exports given.
fn with_alloc(exports: &str) -> String {
    format!(
        r#"(module
            (memory (export "memory") 1)
            (global $next (mut i32) (i32.const 1024))
            (func (export "ks_alloc") (param $len i32) (result i32)
                (local $at i32)
                (local.set $at (global.get $next))
                (global.set $next (i32.add (global.get $next) (local.get $len)))
                (local.get $at))
            {exports})"#
    )
}

/// `echo(ptr, len)` hands the input back.
const ECHO: &str = r#"(func (export "echo") (param $ptr i32) (param $len i32) (result i64)
    (i64.or
        (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
        (i64.extend_i32_u (local.get $len))))"#;

fn load(bytes: &[u8]) -> Result<Plugin, HostError> {
    Plugin::load(bytes, false)
}

#[test]
fn passes_bytes_in_and_out() {
    let mut plugin = load(&importer(&with_alloc(ECHO))).unwrap();
    assert_eq!(plugin.manifest().id.as_str(), "org.example.test");
    assert_eq!(plugin.call("echo", b"hello").unwrap(), b"hello");
    assert_eq!(
        plugin.call("echo", "Grüße".as_bytes()).unwrap(),
        "Grüße".as_bytes()
    );
    assert_eq!(plugin.call("echo", b"").unwrap(), b"");
}

#[test]
fn hands_back_bytes_the_plugin_wrote_itself() {
    let wat = with_alloc(
        r#"(data (i32.const 16) "from the plugin")
        (func (export "fixed") (param i32 i32) (result i64)
            (i64.or (i64.shl (i64.const 16) (i64.const 32)) (i64.const 15)))"#,
    );
    let mut plugin = load(&importer(&wat)).unwrap();
    assert_eq!(
        plugin.call("fixed", b"ignored").unwrap(),
        b"from the plugin"
    );
}

#[test]
fn a_plugin_that_loops_forever_is_stopped() {
    let wat = with_alloc(
        r#"(func (export "spin") (param i32 i32) (result i64)
            (loop $again (br $again))
            (i64.const 0))
        "#,
    );
    let limits = Limits {
        fuel_per_call: 100_000,
        ..Limits::default()
    };
    let mut plugin = Plugin::load_with(&importer(&wat), false, limits).unwrap();
    assert_eq!(plugin.call("spin", b""), Err(HostError::RanTooLong));
    assert!(plugin.is_stopped());
    assert_eq!(plugin.call("spin", b""), Err(HostError::Stopped));
}

#[test]
fn every_call_gets_fresh_fuel() {
    // About 3 000 instructions per call, with fuel for about 10 000.
    let wat = with_alloc(
        r#"(func (export "count") (param i32 i32) (result i64)
            (local $i i32)
            (loop $again
                (local.set $i (i32.add (local.get $i) (i32.const 1)))
                (br_if $again (i32.lt_u (local.get $i) (i32.const 1000))))
            (i64.const 0))"#,
    );
    let limits = Limits {
        fuel_per_call: 10_000,
        ..Limits::default()
    };
    let mut plugin = Plugin::load_with(&importer(&wat), false, limits).unwrap();
    for _ in 0..5 {
        assert_eq!(plugin.call("count", b"").unwrap(), b"");
    }
}

#[test]
fn a_start_function_that_loops_forever_is_stopped_too() {
    let wat = with_alloc(
        r#"(func $spin (loop $again (br $again)))
        (start $spin)"#,
    );
    let limits = Limits {
        fuel_per_call: 100_000,
        ..Limits::default()
    };
    assert_eq!(
        Plugin::load_with(&importer(&wat), false, limits).unwrap_err(),
        HostError::RanTooLong
    );
}

#[test]
fn a_trap_stops_the_plugin() {
    let wat = with_alloc(
        r#"(func (export "fail") (param i32 i32) (result i64) (unreachable))
        (func (export "divide") (param i32 i32) (result i64)
            (drop (i32.div_u (i32.const 1) (i32.const 0))) (i64.const 0))"#,
    );
    let mut plugin = load(&importer(&wat)).unwrap();
    assert_eq!(plugin.call("fail", b""), Err(HostError::Trapped));
    assert_eq!(plugin.call("divide", b""), Err(HostError::Stopped));
    let mut plugin = load(&importer(&wat)).unwrap();
    assert_eq!(plugin.call("divide", b""), Err(HostError::Trapped));
}

#[test]
fn deep_recursion_traps_instead_of_crashing_the_app() {
    let wat = with_alloc(
        r#"(func $down (param i32) (result i32)
            (call $down (i32.add (local.get 0) (i32.const 1))))
        (func (export "recurse") (param i32 i32) (result i64)
            (i64.extend_i32_u (call $down (i32.const 0))))"#,
    );
    let mut plugin = load(&importer(&wat)).unwrap();
    assert_eq!(plugin.call("recurse", b""), Err(HostError::Trapped));
}

#[test]
fn imports_are_refused() {
    for (module, name) in [
        ("wasi_snapshot_preview1", "fd_write"),
        ("kassensturz", "read_book"),
        ("env", "memory"),
    ] {
        let import = if name == "memory" {
            format!(r#"(import "{module}" "{name}" (memory 1))"#)
        } else {
            format!(r#"(import "{module}" "{name}" (func (param i32) (result i32)))"#)
        };
        let wat = format!(
            r#"(module {import}
                (memory (export "memory") 1)
                (func (export "ks_alloc") (param i32) (result i32) (i32.const 0)))"#
        );
        assert_eq!(
            load(&importer(&wat)).unwrap_err(),
            HostError::ImportNotAllowed {
                module: module.into(),
                name: name.into()
            }
        );
    }
}

#[test]
fn memory_stays_within_the_limit() {
    // 2 000 pages are 125 MiB, more than the 64 MiB the host allows.
    let wat = r#"(module
        (memory (export "memory") 2000)
        (func (export "ks_alloc") (param i32) (result i32) (i32.const 0)))"#;
    assert_eq!(load(&importer(wat)).unwrap_err(), HostError::StartFailed);

    // Growing past the limit fails inside the plugin: memory.grow gives -1.
    let wat = with_alloc(
        r#"(func (export "grow") (param i32 i32) (result i64)
            (i64.extend_i32_u (memory.grow (i32.const 2000))))"#,
    );
    let mut plugin = load(&importer(&wat)).unwrap();
    let packed = plugin.call("grow", b"");
    // -1 as the packed result reads as address 0, length 0xFFFF_FFFF.
    assert_eq!(
        packed,
        Err(HostError::OutputTooBig {
            max_bytes: Limits::default().max_output_bytes
        })
    );
    assert!(!plugin.is_stopped());
}

#[test]
fn a_second_memory_or_a_big_table_is_refused() {
    let wat = with_alloc(r#"(table 20000 funcref)"#);
    assert_eq!(load(&importer(&wat)).unwrap_err(), HostError::StartFailed);
}

#[test]
fn needs_memory_and_alloc_exports_of_the_right_type() {
    let no_memory = r#"(module (memory 1)
        (func (export "ks_alloc") (param i32) (result i32) (i32.const 0)))"#;
    assert_eq!(
        load(&importer(no_memory)).unwrap_err(),
        HostError::ExportMissing {
            name: "memory".into()
        }
    );
    let no_alloc = r#"(module (memory (export "memory") 1))"#;
    assert_eq!(
        load(&importer(no_alloc)).unwrap_err(),
        HostError::ExportMissing {
            name: "ks_alloc".into()
        }
    );
    let wrong_alloc = r#"(module (memory (export "memory") 1)
        (func (export "ks_alloc") (param i64) (result i32) (i32.const 0)))"#;
    assert_eq!(
        load(&importer(wrong_alloc)).unwrap_err(),
        HostError::ExportWrongType {
            name: "ks_alloc".into()
        }
    );
    let memory_is_a_global = r#"(module (global (export "memory") i32 (i32.const 0))
        (func (export "ks_alloc") (param i32) (result i32) (i32.const 0)))"#;
    assert_eq!(
        load(&importer(memory_is_a_global)).unwrap_err(),
        HostError::ExportWrongType {
            name: "memory".into()
        }
    );
}

#[test]
fn calls_need_an_export_of_the_right_type() {
    let wat = with_alloc(
        r#"(func (export "wrong") (param i32) (result i32) (local.get 0))
        (global (export "value") i32 (i32.const 0))"#,
    );
    let mut plugin = load(&importer(&wat)).unwrap();
    assert_eq!(
        plugin.call("missing", b""),
        Err(HostError::ExportMissing {
            name: "missing".into()
        })
    );
    for name in ["wrong", "value"] {
        assert_eq!(
            plugin.call(name, b""),
            Err(HostError::ExportWrongType { name: name.into() })
        );
    }
    assert!(!plugin.is_stopped());
}

#[test]
fn input_must_fit_where_the_plugin_says() {
    let wat = r#"(module (memory (export "memory") 1)
        (func (export "ks_alloc") (param i32) (result i32) (i32.const 65530))
        (func (export "echo") (param i32 i32) (result i64) (i64.const 0)))"#;
    let mut plugin = load(&importer(wat)).unwrap();
    assert_eq!(plugin.call("echo", b"12345").unwrap(), b"");
    assert_eq!(
        plugin.call("echo", b"far too long"),
        Err(HostError::InputNotAccepted)
    );
    assert!(plugin.is_stopped());

    let limits = Limits {
        max_input_bytes: 4,
        ..Limits::default()
    };
    let mut plugin = Plugin::load_with(&importer(&with_alloc(ECHO)), false, limits).unwrap();
    assert_eq!(
        plugin.call("echo", b"12345"),
        Err(HostError::InputTooBig { max_bytes: 4 })
    );
    assert!(!plugin.is_stopped());
}

#[test]
fn output_must_lie_inside_the_plugin_memory() {
    let wat = with_alloc(
        r#"(func (export "outside") (param i32 i32) (result i64)
            (i64.or (i64.shl (i64.const 65530) (i64.const 32)) (i64.const 100)))"#,
    );
    let mut plugin = load(&importer(&wat)).unwrap();
    assert_eq!(plugin.call("outside", b""), Err(HostError::OutputInvalid));

    let limits = Limits {
        max_output_bytes: 3,
        ..Limits::default()
    };
    let mut plugin = Plugin::load_with(&importer(&with_alloc(ECHO)), false, limits).unwrap();
    assert_eq!(
        plugin.call("echo", b"1234"),
        Err(HostError::OutputTooBig { max_bytes: 3 })
    );
}

#[test]
fn an_ai_plugin_is_not_loaded_while_ai_is_off() {
    let file = plugin(&with_alloc(ECHO), &["ai_provider"], &["network"]);
    assert_eq!(Plugin::load(&file, false).unwrap_err(), HostError::AiOff);
    let mut loaded = Plugin::load(&file, true).unwrap();
    assert_eq!(loaded.call("echo", b"on").unwrap(), b"on");
}

#[test]
fn a_theme_loads_without_code_and_takes_no_calls() {
    let mut theme = load(&plugin("(module)", &["theme"], &[])).unwrap();
    assert!(theme.manifest().is_data_only());
    assert_eq!(theme.call("echo", b""), Err(HostError::NoCode));
}

#[test]
fn a_plugin_with_code_but_nothing_in_it_still_needs_its_exports() {
    assert_eq!(
        load(&importer("(module)")).unwrap_err(),
        HostError::ExportMissing {
            name: "memory".into()
        }
    );
}

#[test]
fn package_errors_come_through() {
    assert_eq!(
        load(b"not a plugin").unwrap_err(),
        HostError::Package(PluginError::NotAPlugin)
    );
    let no_manifest = wat::parse_str(with_alloc(ECHO)).unwrap();
    assert_eq!(
        load(&no_manifest).unwrap_err(),
        HostError::Package(PluginError::ManifestMissing)
    );
}

#[test]
fn invalid_webassembly_is_refused() {
    let mut file = importer(&with_alloc(ECHO));
    // A section with an ID WebAssembly doesn't have.
    file.extend([99, 1, 0]);
    assert_eq!(load(&file).unwrap_err(), HostError::ModuleInvalid);
}
