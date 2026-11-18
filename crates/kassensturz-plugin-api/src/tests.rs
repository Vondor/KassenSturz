// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::{Value, json};

use crate::*;

/// A valid importer manifest, as an example plugin would ship it.
fn importer() -> Value {
    json!({
        "manifest_version": 1,
        "id": "org.example.csv-import",
        "name": "Example CSV import",
        "version": "1.2.3",
        "api": "1.0",
        "extension_points": ["importer"],
        "permissions": ["read_book"],
        "description": "Reads the CSV files of an invented bank.\nSecond line.",
        "author": "Example Author",
        "license": "GPL-3.0-or-later",
        "homepage": "https://example.org/csv-import"
    })
}

fn parse(value: &Value) -> Result<Manifest, PluginError> {
    Manifest::parse(value.to_string().as_bytes())
}

fn with(key: &str, value: Value) -> Value {
    let mut manifest = importer();
    manifest[key] = value;
    manifest
}

fn without(key: &str) -> Value {
    let mut manifest = importer();
    manifest.as_object_mut().unwrap().remove(key);
    manifest
}

fn invalid(reason: &'static str, key: &str) -> Result<Manifest, PluginError> {
    Err(PluginError::ManifestInvalid {
        reason,
        key: key.to_owned(),
    })
}

#[test]
fn reads_a_full_manifest() {
    let manifest = parse(&importer()).unwrap();
    assert_eq!(manifest.id.as_str(), "org.example.csv-import");
    assert_eq!(manifest.name, "Example CSV import");
    assert_eq!(manifest.version.to_string(), "1.2.3");
    assert_eq!(manifest.api, API_VERSION);
    assert_eq!(manifest.extension_points, [ExtensionPoint::Importer]);
    assert_eq!(manifest.permissions, [Permission::ReadBook]);
    assert_eq!(manifest.license.as_deref(), Some("GPL-3.0-or-later"));
    assert_eq!(
        manifest.homepage.as_deref(),
        Some("https://example.org/csv-import")
    );
    assert!(!manifest.is_ai());
    assert!(!manifest.is_data_only());
}

#[test]
fn optional_keys_can_be_left_out() {
    let mut manifest = importer();
    for key in ["description", "author", "license", "homepage"] {
        manifest.as_object_mut().unwrap().remove(key);
    }
    let manifest = parse(&manifest).unwrap();
    assert_eq!(manifest.description, None);
    assert_eq!(manifest.homepage, None);
}

#[test]
fn required_keys_must_be_there() {
    for key in [
        "id",
        "name",
        "version",
        "api",
        "extension_points",
        "permissions",
    ] {
        assert_eq!(parse(&without(key)), invalid("missing", key), "{key}");
    }
    assert_eq!(
        parse(&without("manifest_version")),
        invalid("missing", "manifest_version")
    );
}

#[test]
fn refuses_what_is_not_a_manifest() {
    for bytes in [&b"not json"[..], b"[]", b"\"text\"", b"", b"{\"a\":1} x"] {
        assert_eq!(
            Manifest::parse(bytes),
            Err(PluginError::ManifestMalformed),
            "{bytes:?}"
        );
    }
}

#[test]
fn refuses_a_manifest_that_is_too_big() {
    let manifest = with("description", json!("a".repeat(MAX_MANIFEST_BYTES)));
    assert_eq!(
        parse(&manifest),
        Err(PluginError::ManifestTooBig {
            max_bytes: MAX_MANIFEST_BYTES
        })
    );
}

#[test]
fn refuses_unknown_keys() {
    assert_eq!(
        parse(&with("run_on_start", json!(true))),
        invalid("unknown-key", "run_on_start")
    );
}

#[test]
fn a_newer_format_or_api_says_so_before_naming_unknown_keys() {
    let mut manifest = with("manifest_version", json!(2));
    manifest["new_key"] = json!(1);
    assert_eq!(
        parse(&manifest),
        Err(PluginError::UnsupportedManifestVersion { version: 2 })
    );

    let mut manifest = with("api", json!("1.1"));
    manifest["extension_points"] = json!(["wizard"]);
    assert_eq!(
        parse(&manifest),
        Err(PluginError::ApiTooNew {
            api: ApiVersion { major: 1, minor: 1 }
        })
    );
}

#[test]
fn checks_the_api_version() {
    assert_eq!(
        parse(&with("api", json!("2.0"))),
        Err(PluginError::ApiUnsupported {
            api: ApiVersion { major: 2, minor: 0 }
        })
    );
    assert_eq!(
        parse(&with("api", json!("0.9"))),
        Err(PluginError::ApiUnsupported {
            api: ApiVersion { major: 0, minor: 9 }
        })
    );
    for api in ["1", "1.0.0", "01.0", "1.00", "1.x", " 1.0", "1.", "99999.0"] {
        assert_eq!(parse(&with("api", json!(api))), invalid("malformed", "api"));
    }
    assert_eq!(parse(&with("api", json!(1))), invalid("wrong-type", "api"));
}

#[test]
fn manifest_version_must_be_a_whole_number() {
    for value in [json!("1"), json!(1.0), json!(null)] {
        assert_eq!(
            parse(&with("manifest_version", value.clone())),
            invalid("wrong-type", "manifest_version"),
            "{value}"
        );
    }
}

#[test]
fn checks_the_plugin_id() {
    for id in [
        "org.example.a",
        "a.b",
        "org.example-1.csv-import",
        "a.b.c.d.e.f.g.h",
    ] {
        assert!(parse(&with("id", json!(id))).is_ok(), "{id}");
    }
    for id in [
        "example",
        "Org.example",
        "org..example",
        "org.-example",
        "org.example-",
        "org.exämple",
        "org.example_a",
        "org.example.",
        "a.b.c.d.e.f.g.h.i",
        "",
    ] {
        assert_eq!(
            parse(&with("id", json!(id))),
            invalid("malformed", "id"),
            "{id}"
        );
    }
    let long = format!("org.{}.{}", "a".repeat(63), "b".repeat(63));
    assert_eq!(parse(&with("id", json!(long))), invalid("malformed", "id"));
    let part = format!("org.{}", "a".repeat(64));
    assert_eq!(parse(&with("id", json!(part))), invalid("malformed", "id"));
}

#[test]
fn checks_the_plugin_version_and_orders_it_by_parts() {
    for version in [
        "1.2",
        "1.2.3.4",
        "1.02.3",
        "v1.2.3",
        "1.2.-3",
        "1234567890.0.0",
    ] {
        assert_eq!(
            parse(&with("version", json!(version))),
            invalid("malformed", "version"),
            "{version}"
        );
    }
    let old = PluginVersion::parse("1.9.0").unwrap();
    let new = PluginVersion::parse("1.10.0").unwrap();
    assert!(old < new);
    assert_eq!(
        PluginVersion::parse("0.0.0").map(|v| v.to_string()),
        Some("0.0.0".into())
    );
}

#[test]
fn shown_texts_have_no_hidden_characters() {
    for name in [
        "Bank\u{202E}gnihtemos",
        "Bank\u{200B}",
        "Bank\tImport",
        "Bank\nImport",
        "A\u{0}B",
        "\u{2066}Bank",
    ] {
        assert_eq!(
            parse(&with("name", json!(name))),
            invalid("bad-characters", "name"),
            "{name:?}"
        );
    }
    assert_eq!(
        parse(&with("name", json!(" Bank"))),
        invalid("bad-characters", "name")
    );
    assert_eq!(parse(&with("name", json!("  "))), invalid("empty", "name"));
    // Letters of every script are fine.
    assert!(parse(&with("name", json!("Kassenbuch-Import für Ärzte 日本"))).is_ok());
    // Only the description may have line breaks.
    assert!(parse(&with("description", json!("One.\nTwo."))).is_ok());
    assert_eq!(
        parse(&with("author", json!("A\nB"))),
        invalid("bad-characters", "author")
    );
}

#[test]
fn shown_texts_have_limits_in_characters() {
    let name = "ä".repeat(MAX_NAME_CHARS);
    assert!(parse(&with("name", json!(name))).is_ok());
    let name = "ä".repeat(MAX_NAME_CHARS + 1);
    assert_eq!(
        parse(&with("name", json!(name))),
        invalid("too-long", "name")
    );
    let description = "a".repeat(MAX_DESCRIPTION_CHARS + 1);
    assert_eq!(
        parse(&with("description", json!(description))),
        invalid("too-long", "description")
    );
}

#[test]
fn checks_license_and_homepage() {
    assert!(parse(&with("license", json!("MIT OR Apache-2.0"))).is_ok());
    assert_eq!(
        parse(&with("license", json!("<b>MIT</b>"))),
        invalid("bad-characters", "license")
    );
    for homepage in [
        "http://example.org",
        "https://",
        "https:///path",
        "javascript:alert(1)",
        "https://example.org/a b",
        "file:///etc/passwd",
    ] {
        assert_eq!(
            parse(&with("homepage", json!(homepage))),
            invalid("malformed", "homepage"),
            "{homepage}"
        );
    }
}

#[test]
fn lists_must_be_lists_of_words_without_repeats() {
    assert_eq!(
        parse(&with("permissions", json!("read_book"))),
        invalid("wrong-type", "permissions")
    );
    assert_eq!(
        parse(&with("permissions", json!([1]))),
        invalid("wrong-type", "permissions")
    );
    assert_eq!(
        parse(&with("permissions", json!(["read_book", "read_book"]))),
        invalid("duplicate", "permissions")
    );
    assert_eq!(
        parse(&with("extension_points", json!([]))),
        invalid("empty", "extension_points")
    );
    assert_eq!(
        parse(&with("extension_points", json!(vec!["importer"; 8]))),
        invalid("too-many", "extension_points")
    );
}

#[test]
fn unknown_words_are_named() {
    assert_eq!(
        parse(&with("extension_points", json!(["storage"]))),
        Err(PluginError::UnknownExtensionPoint {
            name: "storage".into()
        })
    );
    assert_eq!(
        parse(&with("permissions", json!(["files"]))),
        Err(PluginError::UnknownPermission {
            name: "files".into()
        })
    );
}

#[test]
fn lists_come_back_in_a_fixed_order() {
    let manifest = with(
        "extension_points",
        json!(["automation", "importer", "report"]),
    );
    let manifest = Manifest::parse(
        with_permissions(manifest, json!(["propose_changes", "read_book"]))
            .to_string()
            .as_bytes(),
    )
    .unwrap();
    assert_eq!(
        manifest.extension_points,
        [
            ExtensionPoint::Importer,
            ExtensionPoint::Report,
            ExtensionPoint::Automation
        ]
    );
    assert_eq!(
        manifest.permissions,
        [Permission::ReadBook, Permission::ProposeChanges]
    );
}

fn with_permissions(mut manifest: Value, permissions: Value) -> Value {
    manifest["permissions"] = permissions;
    manifest
}

/// Every pair of extension point and permission: allowed exactly as D-848
/// says, and refused otherwise.
#[test]
fn permissions_follow_the_extension_points() {
    let allowed = |point: ExtensionPoint, permission: Permission| match point {
        ExtensionPoint::Importer | ExtensionPoint::Exporter | ExtensionPoint::Report => {
            permission == Permission::ReadBook
        }
        ExtensionPoint::Automation => permission != Permission::Network,
        ExtensionPoint::AiProvider => permission == Permission::Network,
        ExtensionPoint::Theme | ExtensionPoint::UiDensity => false,
    };
    for point in ExtensionPoint::ALL {
        for permission in Permission::ALL {
            let manifest = with_permissions(
                with("extension_points", json!([point.key()])),
                json!([permission.key()]),
            );
            let result = parse(&manifest);
            if allowed(point, permission) {
                assert!(result.is_ok(), "{point:?} {permission:?}: {result:?}");
            } else {
                assert_eq!(
                    result,
                    Err(PluginError::PermissionNotAllowed { permission }),
                    "{point:?} {permission:?}"
                );
            }
        }
    }
}

#[test]
fn network_is_only_for_an_ai_provider() {
    let manifest = with_permissions(
        with("extension_points", json!(["importer", "automation"])),
        json!(["network"]),
    );
    assert_eq!(
        parse(&manifest),
        Err(PluginError::PermissionNotAllowed {
            permission: Permission::Network
        })
    );
    let manifest = with_permissions(
        with("extension_points", json!(["ai_provider"])),
        json!(["network"]),
    );
    let manifest = parse(&manifest).unwrap();
    assert!(manifest.is_ai());
}

#[test]
fn data_only_plugins_run_no_code_and_ask_for_nothing() {
    let theme = with_permissions(
        with("extension_points", json!(["theme", "ui_density"])),
        json!([]),
    );
    let manifest = parse(&theme).unwrap();
    assert!(manifest.is_data_only());
    assert!(manifest.permissions.is_empty());

    let mixed = with_permissions(
        with("extension_points", json!(["theme", "importer"])),
        json!([]),
    );
    assert_eq!(parse(&mixed), Err(PluginError::DataOnlyMixed));
}

#[test]
fn every_key_round_trips_through_its_word() {
    for point in ExtensionPoint::ALL {
        assert_eq!(ExtensionPoint::from_key(point.key()), Some(point));
    }
    for permission in Permission::ALL {
        assert_eq!(Permission::from_key(permission.key()), Some(permission));
    }
    assert_eq!(ExtensionPoint::from_key("Importer"), None);
}

#[test]
fn this_kassensturz_offers_api_1_0() {
    assert_eq!(API_VERSION.to_string(), "1.0");
    assert_eq!(API_VERSION.check_supported(), Ok(()));
}
