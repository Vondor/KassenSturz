// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::json;

use crate::*;

const HEADER: [u8; 8] = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

fn leb(mut value: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = u8::try_from(value & 0x7F).unwrap();
        value >>= 7;
        if value == 0 {
            bytes.push(byte);
            return bytes;
        }
        bytes.push(byte | 0x80);
    }
}

fn section(id: u8, content: &[u8]) -> Vec<u8> {
    let mut bytes = vec![id];
    bytes.extend(leb(content.len()));
    bytes.extend(content);
    bytes
}

fn custom(name: &str, data: &[u8]) -> Vec<u8> {
    let mut content = leb(name.len());
    content.extend(name.as_bytes());
    content.extend(data);
    section(0, &content)
}

/// One function `() -> ()` that does nothing: type, function and code
/// sections.
fn code() -> Vec<u8> {
    let mut bytes = section(1, &[0x01, 0x60, 0x00, 0x00]);
    bytes.extend(section(3, &[0x01, 0x00]));
    bytes.extend(section(10, &[0x01, 0x02, 0x00, 0x0B]));
    bytes
}

fn manifest(points: &[&str], permissions: &[&str]) -> Vec<u8> {
    json!({
        "manifest_version": 1,
        "id": "org.example.test",
        "name": "Test",
        "version": "1.0.0",
        "api": "1.0",
        "extension_points": points,
        "permissions": permissions,
    })
    .to_string()
    .into_bytes()
}

fn module(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = HEADER.to_vec();
    for part in parts {
        bytes.extend(part);
    }
    bytes
}

#[test]
fn reads_the_manifest_of_a_module_with_code() {
    let file = module(&[
        custom("name", b"anything"),
        code(),
        custom(MANIFEST_SECTION, &manifest(&["importer"], &[])),
    ]);
    let package = Package::read(&file).unwrap();
    assert!(package.has_code);
    assert_eq!(package.manifest.id.as_str(), "org.example.test");
}

fn theme() -> Vec<u8> {
    custom(
        THEME_SECTION,
        json!({
            "theme_version": 1,
            "light": {"background": "#ffffff", "text": "#202020", "accent": "#2e7d32"},
            "dark": {"background": "#1b1b1b", "text": "#eeeeee", "accent": "#81c784"},
        })
        .to_string()
        .as_bytes(),
    )
}

#[test]
fn a_theme_is_a_module_without_code() {
    let file = module(&[
        custom(MANIFEST_SECTION, &manifest(&["theme"], &[])),
        theme(),
    ]);
    let package = Package::read(&file).unwrap();
    assert!(!package.has_code);
    assert!(package.manifest.is_data_only());
    let colours = package.theme.unwrap();
    assert_eq!(colours.light.accent.hex(), "#2e7d32");
    assert_eq!(colours.dark.text.hex(), "#eeeeee");

    let file = module(&[
        custom(MANIFEST_SECTION, &manifest(&["theme"], &[])),
        theme(),
        code(),
    ]);
    assert_eq!(Package::read(&file), Err(PluginError::DataOnlyHasCode));
}

#[test]
fn a_theme_plugin_has_its_colours_exactly_once_and_only_it_has_them() {
    let theme_manifest = custom(MANIFEST_SECTION, &manifest(&["theme"], &[]));
    assert_eq!(
        Package::read(&module(std::slice::from_ref(&theme_manifest))),
        Err(PluginError::ThemeMissing)
    );
    assert_eq!(
        Package::read(&module(&[theme_manifest.clone(), theme(), theme()])),
        Err(PluginError::ThemeTwice)
    );
    let density = custom(MANIFEST_SECTION, &manifest(&["ui_density"], &[]));
    assert_eq!(
        Package::read(&module(&[density, theme()])),
        Err(PluginError::ThemeUnexpected)
    );
    let importer = custom(MANIFEST_SECTION, &manifest(&["importer"], &[]));
    assert_eq!(
        Package::read(&module(&[importer, theme(), code()])),
        Err(PluginError::ThemeUnexpected)
    );
    // A theme section that breaks a rule refuses the file.
    let broken = custom(THEME_SECTION, b"[]");
    assert_eq!(
        Package::read(&module(&[theme_manifest, broken])),
        Err(PluginError::ThemeMalformed)
    );
}

#[test]
fn needs_exactly_one_manifest() {
    assert_eq!(
        Package::read(&module(&[code()])),
        Err(PluginError::ManifestMissing)
    );
    let one = custom(MANIFEST_SECTION, &manifest(&["importer"], &[]));
    assert_eq!(
        Package::read(&module(&[one.clone(), one])),
        Err(PluginError::ManifestTwice)
    );
}

#[test]
fn passes_on_a_manifest_error() {
    let file = module(&[custom(MANIFEST_SECTION, b"{}")]);
    assert_eq!(
        Package::read(&file),
        Err(PluginError::ManifestInvalid {
            reason: "missing",
            key: "manifest_version".into()
        })
    );
}

#[test]
fn refuses_unknown_kassensturz_sections_but_not_others() {
    let manifest = custom(MANIFEST_SECTION, &manifest(&["importer"], &[]));
    let file = module(&[manifest.clone(), custom("kassensturz-wizard", b"")]);
    assert_eq!(
        Package::read(&file),
        Err(PluginError::UnknownSection {
            name: "kassensturz-wizard".into()
        })
    );
    let file = module(&[manifest, custom("producers", b"x")]);
    assert!(Package::read(&file).is_ok());
}

#[test]
fn refuses_what_is_not_a_module() {
    let manifest = custom(MANIFEST_SECTION, &manifest(&["importer"], &[]));
    let good = module(std::slice::from_ref(&manifest));
    let mut wrong_version = good.clone();
    wrong_version[4] = 2;
    let cut = &good[..good.len() - 1];
    let mut long_leb = HEADER.to_vec();
    long_leb.extend([0, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00]);
    let mut big_leb = HEADER.to_vec();
    big_leb.extend([0, 0xFF, 0xFF, 0xFF, 0xFF, 0x1F]);
    let mut bad_name = HEADER.to_vec();
    bad_name.extend(section(0, &[0x02, 0xFF, 0xFE]));
    let mut name_too_long = HEADER.to_vec();
    name_too_long.extend(section(0, &[0x09, b'a']));
    for file in [
        &b""[..],
        b"\0asm",
        b"MZ\x90\x00 not wasm",
        &wrong_version,
        cut,
        &long_leb,
        &big_leb,
        &bad_name,
        &name_too_long,
    ] {
        assert_eq!(
            Package::read(file),
            Err(PluginError::NotAPlugin),
            "{file:?}"
        );
    }
}

#[test]
fn refuses_a_file_that_is_too_big() {
    let file = vec![0; MAX_PLUGIN_BYTES + 1];
    assert_eq!(
        Package::read(&file),
        Err(PluginError::FileTooBig {
            max_bytes: MAX_PLUGIN_BYTES
        })
    );
}

/// The green example theme (#791): the committed plugin file is exactly
/// its JSON files packed by `tools/make-data-plugin.py`, and KassenSturz
/// adds it.
#[test]
fn the_green_example_theme_is_its_json_packed_and_is_accepted() {
    let manifest = include_bytes!("../../../examples/plugins/green-theme/manifest.json");
    let theme = include_bytes!("../../../examples/plugins/green-theme/theme.json");
    let file = include_bytes!("../../../examples/plugins/green-theme/green-theme.wasm");
    assert_eq!(
        file.as_slice(),
        module(&[
            custom(MANIFEST_SECTION, manifest),
            custom(THEME_SECTION, theme)
        ]),
        "run tools/make-data-plugin.py again"
    );
    let package = Package::read(file).unwrap();
    assert_eq!(package.manifest.name, "Green");
    assert!(!package.has_code);
    let theme = package.theme.unwrap();
    assert_eq!(theme.light.accent.hex(), "#2e7d32");
    assert_eq!(theme.corner_radius, Some(8));
}
