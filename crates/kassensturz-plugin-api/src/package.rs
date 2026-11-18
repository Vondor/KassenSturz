// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin file (D-849): one WebAssembly module that carries its
//! manifest in a custom section.
//!
//! This reader walks the module's sections without running or compiling
//! anything, so the Plugins tab can show a plugin's manifest before the host
//! ever loads it, and a data-only plugin (Theme, UI density) needs no
//! runtime at all.

use crate::{Manifest, PluginError};

/// The most bytes a plugin file may have.
pub const MAX_PLUGIN_BYTES: usize = 16 * 1024 * 1024;
/// The custom section that holds the manifest (D-847).
pub const MANIFEST_SECTION: &str = "kassensturz-manifest";
/// Custom sections whose name starts with this belong to KassenSturz; one
/// this version doesn't know refuses the file.
const OWN_PREFIX: &str = "kassensturz-";

/// WebAssembly's magic number and binary format version 1.
const HEADER: [u8; 8] = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
/// The section IDs that hold code: function, start and code.
const CODE_SECTIONS: [u8; 3] = [3, 8, 10];

/// A plugin file whose manifest has been read and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Its checked manifest.
    pub manifest: Manifest,
    /// Whether the module has any code (functions or a start function).
    pub has_code: bool,
}

impl Package {
    /// Reads a plugin file. Refuses it whole on the first problem.
    pub fn read(bytes: &[u8]) -> Result<Self, PluginError> {
        if bytes.len() > MAX_PLUGIN_BYTES {
            return Err(PluginError::FileTooBig {
                max_bytes: MAX_PLUGIN_BYTES,
            });
        }
        let Some(mut rest) = bytes.strip_prefix(&HEADER[..]) else {
            return Err(PluginError::NotAPlugin);
        };
        let mut manifest = None;
        let mut has_code = false;
        while !rest.is_empty() {
            let (&id, after_id) = rest.split_first().ok_or(PluginError::NotAPlugin)?;
            let (size, after_size) = leb_u32(after_id).ok_or(PluginError::NotAPlugin)?;
            let size = usize::try_from(size).map_err(|_| PluginError::NotAPlugin)?;
            if size > after_size.len() {
                return Err(PluginError::NotAPlugin);
            }
            let (content, next) = after_size.split_at(size);
            rest = next;
            if CODE_SECTIONS.contains(&id) {
                has_code = true;
            }
            if id != 0 {
                continue;
            }
            let (name, data) = custom_name(content).ok_or(PluginError::NotAPlugin)?;
            if name == MANIFEST_SECTION {
                if manifest.is_some() {
                    return Err(PluginError::ManifestTwice);
                }
                manifest = Some(data);
            } else if name.starts_with(OWN_PREFIX) {
                return Err(PluginError::UnknownSection {
                    name: name.to_owned(),
                });
            }
        }
        let manifest = Manifest::parse(manifest.ok_or(PluginError::ManifestMissing)?)?;
        if manifest.is_data_only() && has_code {
            return Err(PluginError::DataOnlyHasCode);
        }
        Ok(Self { manifest, has_code })
    }
}

/// An unsigned LEB128 number of at most 32 bits, and the bytes after it.
fn leb_u32(bytes: &[u8]) -> Option<(u32, &[u8])> {
    let mut value: u32 = 0;
    for (index, &byte) in bytes.iter().enumerate().take(5) {
        let bits = u32::from(byte & 0x7F);
        // The fifth byte may only hold the top four bits.
        if index == 4 && bits > 0x0F {
            return None;
        }
        value |= bits << (7 * index);
        if byte & 0x80 == 0 {
            return Some((value, &bytes[index + 1..]));
        }
    }
    None
}

/// A custom section's name (UTF-8, length first) and its data.
fn custom_name(content: &[u8]) -> Option<(&str, &[u8])> {
    let (length, rest) = leb_u32(content)?;
    let length = usize::try_from(length).ok()?;
    if length > rest.len() {
        return None;
    }
    let (name, data) = rest.split_at(length);
    Some((core::str::from_utf8(name).ok()?, data))
}
