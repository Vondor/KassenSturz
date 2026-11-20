// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A UI density plugin's data (D-756, D-927): which of KassenSturz's own
//! layout switches it turns on. It is data only, in the custom section
//! [`DENSITY_SECTION`] of the plugin file; a density plugin can't run
//! anything, and it can only pick from the switches KassenSturz defines.
//!
//! No switch can hide a warning, an error or a question before a change:
//! `hide_hints` hides only the grey explanation texts, which never carry
//! one (a test in the UI checks that).

use serde_json::Value;

use crate::PluginError;

/// The custom section that holds a density plugin's data.
pub const DENSITY_SECTION: &str = "kassensturz-density";
/// The density format this KassenSturz reads.
pub const DENSITY_VERSION: i64 = 1;
/// The most bytes a density plugin's data may have.
pub const MAX_DENSITY_BYTES: usize = 1024;

/// The keys of the density object.
const KEYS: [&str; 2] = ["density_version", "switches"];
/// The switches of density format 1.
const SWITCHES: [&str; 3] = ["compact_lists", "hide_hints", "compact_tiles"];

/// A density plugin's switches, read and checked. A switch the data leaves
/// out is off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Density {
    /// Tighter rows in lists and registers.
    pub compact_lists: bool,
    /// Hides the grey explanation texts under settings and forms.
    pub hide_hints: bool,
    /// Smaller gaps between the start screen's tiles.
    pub compact_tiles: bool,
}

impl Density {
    /// Reads and checks a density plugin's data. Refuses it whole on the
    /// first problem.
    pub fn parse(bytes: &[u8]) -> Result<Self, PluginError> {
        if bytes.len() > MAX_DENSITY_BYTES {
            return Err(PluginError::DensityTooBig {
                max_bytes: MAX_DENSITY_BYTES,
            });
        }
        let value: Value =
            serde_json::from_slice(bytes).map_err(|_| PluginError::DensityMalformed)?;
        let Value::Object(object) = value else {
            return Err(PluginError::DensityMalformed);
        };
        let version = match object.get("density_version") {
            None => return Err(invalid("missing", "density_version")),
            Some(value) => value
                .as_i64()
                .ok_or_else(|| invalid("wrong-type", "density_version"))?,
        };
        if version != DENSITY_VERSION {
            return Err(PluginError::UnsupportedDensityVersion { version });
        }
        if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
            return Err(invalid("unknown-key", key));
        }
        let switches = match object.get("switches") {
            None => return Err(invalid("missing", "switches")),
            Some(Value::Object(switches)) => switches,
            Some(_) => return Err(invalid("wrong-type", "switches")),
        };
        if let Some(key) = switches
            .keys()
            .find(|key| !SWITCHES.contains(&key.as_str()))
        {
            return Err(invalid("unknown-key", &format!("switches.{key}")));
        }
        let switch = |key: &str| match switches.get(key) {
            None => Ok(false),
            Some(Value::Bool(on)) => Ok(*on),
            Some(_) => Err(invalid("wrong-type", &format!("switches.{key}"))),
        };
        Ok(Self {
            compact_lists: switch("compact_lists")?,
            hide_hints: switch("hide_hints")?,
            compact_tiles: switch("compact_tiles")?,
        })
    }
}

fn invalid(reason: &'static str, key: &str) -> PluginError {
    PluginError::DensityInvalid {
        reason,
        key: key.to_owned(),
    }
}
