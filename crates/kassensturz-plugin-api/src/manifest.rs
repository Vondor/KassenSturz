// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin manifest v1 (D-847): one JSON object that says who the plugin
//! is, which API version it targets, what it fills and what it asks for.
//!
//! The reader is strict. A key it doesn't know, a value of the wrong type,
//! a text with control or invisible characters, or a list with a word twice
//! refuses the whole manifest; nothing of it is used. That way, what the
//! Plugins tab shows is exactly what the plugin gets.

use serde_json::{Map, Value};

use crate::version::number;
use crate::{ApiVersion, ExtensionPoint, Permission, PluginError};

/// The manifest format this KassenSturz reads.
pub const MANIFEST_VERSION: i64 = 1;
/// The most bytes a manifest may have.
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024;
/// The most characters of a plugin's name and of its author.
pub const MAX_NAME_CHARS: usize = 80;
/// The most characters of a plugin's description.
pub const MAX_DESCRIPTION_CHARS: usize = 500;
/// The most characters of a plugin ID.
const MAX_ID_CHARS: usize = 128;
/// The most dot-separated parts of a plugin ID.
const MAX_ID_PARTS: usize = 8;
/// The most characters of the license field.
const MAX_LICENSE_CHARS: usize = 64;
/// The most characters of the homepage.
const MAX_HOMEPAGE_CHARS: usize = 200;

/// Every key a v1 manifest may have.
const KEYS: [&str; 11] = [
    "manifest_version",
    "id",
    "name",
    "version",
    "api",
    "extension_points",
    "permissions",
    "description",
    "author",
    "license",
    "homepage",
];

/// A plugin's ID: lowercase reverse-DNS, such as `org.example.csv-import`.
///
/// Two to eight parts of `a` to `z`, `0` to `9` and `-`, each at most 63
/// characters and not starting or ending with `-`, at most 128 characters in
/// all. Two plugins with the same ID are the same plugin.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginId(String);

impl PluginId {
    /// Reads an ID, or `None` if it doesn't follow the rules above.
    pub fn parse(text: &str) -> Option<Self> {
        let parts: Vec<&str> = text.split('.').collect();
        let valid = text.len() <= MAX_ID_CHARS
            && (2..=MAX_ID_PARTS).contains(&parts.len())
            && parts.iter().all(|part| {
                !part.is_empty()
                    && part.len() <= 63
                    && !part.starts_with('-')
                    && !part.ends_with('-')
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            });
        valid.then(|| Self(text.to_owned()))
    }

    /// The ID as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A plugin's own version, `MAJOR.MINOR.PATCH`, compared part by part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginVersion {
    /// The first part.
    pub major: u32,
    /// The second part.
    pub minor: u32,
    /// The third part.
    pub patch: u32,
}

impl PluginVersion {
    /// Reads `MAJOR.MINOR.PATCH` (digits only, no leading zeros, at most
    /// nine digits each), or `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        let version = Self {
            major: number(parts.next()?, 9)?,
            minor: number(parts.next()?, 9)?,
            patch: number(parts.next()?, 9)?,
        };
        parts.next().is_none().then_some(version)
    }
}

impl core::fmt::Display for PluginVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A checked plugin manifest (D-847).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Who the plugin is.
    pub id: PluginId,
    /// Its name as the Plugins tab shows it, as plain text.
    pub name: String,
    /// Its own version.
    pub version: PluginVersion,
    /// The plugin API version it targets, one this KassenSturz supports.
    pub api: ApiVersion,
    /// What it fills, in [`ExtensionPoint::ALL`] order, at least one.
    pub extension_points: Vec<ExtensionPoint>,
    /// What it asks for, in [`Permission::ALL`] order; each one allowed by
    /// at least one of its extension points.
    pub permissions: Vec<Permission>,
    /// A short description, as plain text.
    pub description: Option<String>,
    /// Who wrote it, as plain text.
    pub author: Option<String>,
    /// Its license, as the author states it (an SPDX expression, such as
    /// `GPL-3.0-or-later`). KassenSturz doesn't check it.
    pub license: Option<String>,
    /// Its web page, always `https://`. The Plugins tab shows it as text;
    /// KassenSturz never opens it by itself.
    pub homepage: Option<String>,
}

impl Manifest {
    /// Reads and checks a manifest. Refuses it whole on the first problem.
    pub fn parse(bytes: &[u8]) -> Result<Self, PluginError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(PluginError::ManifestTooBig {
                max_bytes: MAX_MANIFEST_BYTES,
            });
        }
        let value: Value =
            serde_json::from_slice(bytes).map_err(|_| PluginError::ManifestMalformed)?;
        let Value::Object(object) = value else {
            return Err(PluginError::ManifestMalformed);
        };

        // The format and the API version come first, so a manifest for a
        // newer KassenSturz says so instead of naming a key it doesn't know.
        let manifest_version = match object.get("manifest_version") {
            None => return Err(invalid("missing", "manifest_version")),
            Some(value) => value
                .as_i64()
                .ok_or_else(|| invalid("wrong-type", "manifest_version"))?,
        };
        if manifest_version != MANIFEST_VERSION {
            return Err(PluginError::UnsupportedManifestVersion {
                version: manifest_version,
            });
        }
        let api = ApiVersion::parse(required_str(&object, "api")?)
            .ok_or_else(|| invalid("malformed", "api"))?;
        api.check_supported()?;

        if let Some(key) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
            return Err(invalid("unknown-key", key));
        }

        let id = PluginId::parse(required_str(&object, "id")?)
            .ok_or_else(|| invalid("malformed", "id"))?;
        let name = text(required_str(&object, "name")?, "name", MAX_NAME_CHARS)?;
        let version = PluginVersion::parse(required_str(&object, "version")?)
            .ok_or_else(|| invalid("malformed", "version"))?;
        let extension_points = extension_points(&object)?;
        let permissions = permissions(&object, &extension_points)?;
        let description = optional_str(&object, "description")?
            .map(|value| text(value, "description", MAX_DESCRIPTION_CHARS))
            .transpose()?;
        let author = optional_str(&object, "author")?
            .map(|value| text(value, "author", MAX_NAME_CHARS))
            .transpose()?;
        let license = optional_str(&object, "license")?.map(license).transpose()?;
        let homepage = optional_str(&object, "homepage")?
            .map(homepage)
            .transpose()?;

        Ok(Self {
            id,
            name,
            version,
            api,
            extension_points,
            permissions,
            description,
            author,
            license,
            homepage,
        })
    }

    /// Whether the plugin belongs to the AI features: KassenSturz hides it
    /// and never loads it while AI is off (D-013, D-754).
    pub fn is_ai(&self) -> bool {
        self.extension_points.iter().any(|point| point.is_ai())
    }

    /// Whether the plugin is data only, without WASM code (D-756).
    pub fn is_data_only(&self) -> bool {
        self.extension_points
            .iter()
            .all(|point| point.is_data_only())
    }
}

fn invalid(reason: &'static str, key: &str) -> PluginError {
    PluginError::ManifestInvalid {
        reason,
        key: key.to_owned(),
    }
}

fn required_str<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, PluginError> {
    optional_str(object, key)?.ok_or_else(|| invalid("missing", key))
}

fn optional_str<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a str>, PluginError> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(invalid("wrong-type", key)),
    }
}

/// The words of a list, each a string, at most `max` of them, none twice.
fn words<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    max: usize,
) -> Result<Vec<&'a str>, PluginError> {
    let Some(value) = object.get(key) else {
        return Err(invalid("missing", key));
    };
    let Value::Array(items) = value else {
        return Err(invalid("wrong-type", key));
    };
    if items.len() > max {
        return Err(invalid("too-many", key));
    }
    let mut words: Vec<&str> = Vec::with_capacity(items.len());
    for item in items {
        let Value::String(word) = item else {
            return Err(invalid("wrong-type", key));
        };
        if words.contains(&word.as_str()) {
            return Err(invalid("duplicate", key));
        }
        words.push(word);
    }
    Ok(words)
}

fn extension_points(object: &Map<String, Value>) -> Result<Vec<ExtensionPoint>, PluginError> {
    let key = "extension_points";
    let mut points = Vec::new();
    for word in words(object, key, ExtensionPoint::ALL.len())? {
        let point =
            ExtensionPoint::from_key(word).ok_or_else(|| PluginError::UnknownExtensionPoint {
                name: word.to_owned(),
            })?;
        points.push(point);
    }
    if points.is_empty() {
        return Err(invalid("empty", key));
    }
    // A data-only plugin runs no code, so it can't also fill a place that
    // runs code (D-756).
    let data_only = points.iter().filter(|point| point.is_data_only()).count();
    if data_only != 0 && data_only != points.len() {
        return Err(PluginError::DataOnlyMixed);
    }
    points.sort();
    Ok(points)
}

fn permissions(
    object: &Map<String, Value>,
    points: &[ExtensionPoint],
) -> Result<Vec<Permission>, PluginError> {
    let mut permissions = Vec::new();
    for word in words(object, "permissions", Permission::ALL.len())? {
        let permission =
            Permission::from_key(word).ok_or_else(|| PluginError::UnknownPermission {
                name: word.to_owned(),
            })?;
        if !points
            .iter()
            .any(|point| point.allowed_permissions().contains(&permission))
        {
            return Err(PluginError::PermissionNotAllowed { permission });
        }
        permissions.push(permission);
    }
    permissions.sort();
    Ok(permissions)
}

/// Characters a shown text may not hold: they are invisible or reorder the
/// text around them, so a name could look like another one (AGENTS.md §8).
fn hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{061C}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

/// A text the Plugins tab shows: not empty, no hidden characters, no space
/// at either end, at most `max` characters. A description may have line
/// breaks; nothing else may.
fn text(value: &str, key: &str, max: usize) -> Result<String, PluginError> {
    if value.trim().is_empty() {
        return Err(invalid("empty", key));
    }
    if value.chars().count() > max {
        return Err(invalid("too-long", key));
    }
    let line_breaks = key == "description";
    if value.trim() != value
        || value
            .chars()
            .any(|c| hidden(c) && !(line_breaks && c == '\n'))
    {
        return Err(invalid("bad-characters", key));
    }
    Ok(value.to_owned())
}

fn license(value: &str) -> Result<String, PluginError> {
    let key = "license";
    if value.is_empty() {
        return Err(invalid("empty", key));
    }
    if value.len() > MAX_LICENSE_CHARS {
        return Err(invalid("too-long", key));
    }
    let spdx = value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-.+:() ".contains(&b));
    if !spdx || value.trim() != value {
        return Err(invalid("bad-characters", key));
    }
    Ok(value.to_owned())
}

fn homepage(value: &str) -> Result<String, PluginError> {
    let key = "homepage";
    if value.len() > MAX_HOMEPAGE_CHARS {
        return Err(invalid("too-long", key));
    }
    let host = value.strip_prefix("https://").unwrap_or_default();
    let valid =
        !host.is_empty() && !host.starts_with('/') && value.bytes().all(|b| b.is_ascii_graphic());
    if !valid {
        return Err(invalid("malformed", key));
    }
    Ok(value.to_owned())
}
