// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::{Value, json};

use crate::*;

fn compact() -> Value {
    json!({
        "density_version": 1,
        "switches": {"compact_lists": true, "hide_hints": true, "compact_tiles": false},
    })
}

fn parse(value: &Value) -> Result<Density, PluginError> {
    Density::parse(value.to_string().as_bytes())
}

fn invalid(reason: &'static str, key: &str) -> Result<Density, PluginError> {
    Err(PluginError::DensityInvalid {
        reason,
        key: key.into(),
    })
}

#[test]
fn reads_the_switches_and_a_left_out_one_is_off() {
    assert_eq!(
        parse(&compact()),
        Ok(Density {
            compact_lists: true,
            hide_hints: true,
            compact_tiles: false,
        })
    );
    let only_tiles = json!({"density_version": 1, "switches": {"compact_tiles": true}});
    assert_eq!(
        parse(&only_tiles),
        Ok(Density {
            compact_tiles: true,
            ..Density::default()
        })
    );
    let none = json!({"density_version": 1, "switches": {}});
    assert_eq!(parse(&none), Ok(Density::default()));
}

#[test]
fn refuses_anything_it_does_not_know() {
    let mut unknown_switch = compact();
    unknown_switch["switches"]["hide_warnings"] = json!(true);
    assert_eq!(
        parse(&unknown_switch),
        invalid("unknown-key", "switches.hide_warnings")
    );
    let mut unknown_key = compact();
    unknown_key["font_size"] = json!(9);
    assert_eq!(parse(&unknown_key), invalid("unknown-key", "font_size"));
    let mut later = compact();
    later["density_version"] = json!(2);
    assert_eq!(
        parse(&later),
        Err(PluginError::UnsupportedDensityVersion { version: 2 })
    );
}

#[test]
fn refuses_wrong_types_and_missing_keys() {
    let mut text = compact();
    text["switches"]["hide_hints"] = json!("yes");
    assert_eq!(parse(&text), invalid("wrong-type", "switches.hide_hints"));
    let mut list = compact();
    list["switches"] = json!(["compact_lists"]);
    assert_eq!(parse(&list), invalid("wrong-type", "switches"));
    let mut version = compact();
    version["density_version"] = json!("1");
    assert_eq!(parse(&version), invalid("wrong-type", "density_version"));
    let mut no_switches = compact();
    no_switches.as_object_mut().unwrap().remove("switches");
    assert_eq!(parse(&no_switches), invalid("missing", "switches"));
    let mut no_version = compact();
    no_version
        .as_object_mut()
        .unwrap()
        .remove("density_version");
    assert_eq!(parse(&no_version), invalid("missing", "density_version"));
}

#[test]
fn refuses_what_is_not_an_object_or_too_big() {
    assert_eq!(Density::parse(b"[]"), Err(PluginError::DensityMalformed));
    assert_eq!(
        Density::parse(b"{ not json"),
        Err(PluginError::DensityMalformed)
    );
    let big = vec![b' '; MAX_DENSITY_BYTES + 1];
    assert_eq!(
        Density::parse(&big),
        Err(PluginError::DensityTooBig {
            max_bytes: MAX_DENSITY_BYTES
        })
    );
}

#[test]
fn the_compact_example_plugin_is_a_valid_density_plugin() {
    let package = Package::read(include_bytes!(
        "../../../examples/plugins/compact-layout/compact-layout.wasm"
    ))
    .unwrap();
    assert!(!package.has_code);
    assert_eq!(
        package.manifest.id.as_str(),
        "io.github.vondor.kassensturz.example.compact"
    );
    assert_eq!(
        package.density,
        Some(Density {
            compact_lists: true,
            hide_hints: true,
            compact_tiles: true,
        })
    );
    // The file is built from the two JSON files beside it.
    let data: Value = serde_json::from_slice(include_bytes!(
        "../../../examples/plugins/compact-layout/density.json"
    ))
    .unwrap();
    assert_eq!(parse(&data), Ok(package.density.unwrap()));
}
