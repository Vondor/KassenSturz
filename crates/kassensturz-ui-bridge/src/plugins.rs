// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugins the user added (#791 step 3, AGENTS.md §7, D-851): copies of
//! plugin files in a `plugins` folder inside KassenSturz's own data folder.
//!
//! The folder is the whole record: a plugin is added by copying its file
//! there as `<id>.wasm` and removed by deleting that copy; nothing goes into
//! the settings or the book. Before a file is copied it is checked whole: its
//! manifest (D-847 to D-849) and a trial start in the sandbox (D-850), so a
//! plugin that can't start is never added. The original file stays where
//! the user downloaded it.
//!
//! While AI is off, a plugin with the AI provider extension point is not
//! listed and can't be added (D-013, D-754).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use kassensturz_core::{ErrorReport, Report};
use kassensturz_plugin_api::{
    MAX_PLUGIN_BYTES, Manifest, Package, PluginError, PluginId, PluginVersion,
};
use kassensturz_plugin_host::Plugin;

use crate::errors::{self, ErrorText};

/// The folder inside the app's data folder that holds the plugins.
pub const FOLDER: &str = "plugins";
/// The ending of a plugin's copy in that folder.
const ENDING: &str = "wasm";

/// The plugin folder, or `None` without a data folder.
pub fn folder() -> Option<PathBuf> {
    crate::platform::app_folder().map(|folder| folder.join(FOLDER))
}

/// A plugin in the folder: its file name and its manifest, or why it can't
/// be used any more (for example after an update of KassenSturz refused its
/// API version).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The copy's file name in the folder.
    pub file: String,
    /// Its manifest, or the code of why it can't be used.
    pub entry: Result<Manifest, ErrorText>,
}

/// What adding a file would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Added {
    /// The plugin is added (or was added, after `add`).
    New,
    /// A plugin with the same ID is already there, in this version; adding
    /// replaces it, which the user must confirm.
    Replaces {
        /// The version that is there now, or `None` if that copy can't be
        /// read.
        installed: Option<PluginVersion>,
    },
}

/// Every plugin in `folder`, sorted by name (unusable ones by file name, at
/// the end). AI plugins are left out while AI is off.
pub fn list(folder: &Path, ai_enabled: bool) -> Vec<Listed> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut listed: Vec<Listed> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| entry.path().extension().and_then(|ending| ending.to_str()) == Some(ENDING))
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let bytes = match read(&entry.path()) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Some(Listed {
                        file,
                        entry: Err(error_text(&error)),
                    });
                }
            };
            // An AI plugin is not even started while AI is off; its
            // manifest is read without running anything to know that.
            if !ai_enabled && Package::read(&bytes).is_ok_and(|package| package.manifest.is_ai()) {
                return None;
            }
            let entry = check(&bytes, ai_enabled).map_err(|error| error_text(&error));
            Some(Listed { file, entry })
        })
        .collect();
    listed.sort_by(|a, b| match (&a.entry, &b.entry) {
        (Ok(a), Ok(b)) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        (Ok(_), Err(_)) => core::cmp::Ordering::Less,
        (Err(_), Ok(_)) => core::cmp::Ordering::Greater,
        (Err(_), Err(_)) => a.file.cmp(&b.file),
    });
    listed
}

/// Why a file can't be read or added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The plugin file or the sandbox refused it.
    Plugin(ErrorReport),
    /// The file can't be read.
    Unreadable,
    /// There is no data folder to keep plugins in.
    NoFolder,
    /// The copy couldn't be written.
    NotSaved,
}

/// The file's bytes, at most [`MAX_PLUGIN_BYTES`].
pub fn read(path: &Path) -> Result<Vec<u8>, Refused> {
    let file = fs::File::open(path).map_err(|_| Refused::Unreadable)?;
    let limit = u64::try_from(MAX_PLUGIN_BYTES).unwrap_or(u64::MAX);
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Refused::Unreadable)?;
    if bytes.len() > MAX_PLUGIN_BYTES {
        return Err(Refused::Plugin(
            PluginError::FileTooBig {
                max_bytes: MAX_PLUGIN_BYTES,
            }
            .report(),
        ));
    }
    Ok(bytes)
}

/// Checks a plugin file whole: its manifest and a trial start in the
/// sandbox. Returns its manifest.
pub fn check(bytes: &[u8], ai_enabled: bool) -> Result<Manifest, Refused> {
    Plugin::load(bytes, ai_enabled)
        .map(|plugin| plugin.manifest().clone())
        .map_err(|error| Refused::Plugin(error.report()))
}

/// What adding `manifest` to `folder` would do.
pub fn would_add(folder: &Path, manifest: &Manifest) -> Added {
    let copy = copy_path(folder, &manifest.id);
    if !copy.exists() {
        return Added::New;
    }
    // Read without running anything: only its version is wanted.
    let installed = read(&copy)
        .ok()
        .and_then(|bytes| Package::read(&bytes).ok())
        .map(|package| package.manifest.version);
    Added::Replaces { installed }
}

/// Adds the plugin file `bytes` to `folder`. It is checked again first. A
/// plugin with the same ID is replaced only when `replace` is true;
/// otherwise the answer is [`Added::Replaces`] and nothing changes.
pub fn add(
    folder: &Path,
    bytes: &[u8],
    ai_enabled: bool,
    replace: bool,
) -> Result<(Manifest, Added), Refused> {
    let manifest = check(bytes, ai_enabled)?;
    let added = would_add(folder, &manifest);
    if added != Added::New && !replace {
        return Ok((manifest, added));
    }
    fs::create_dir_all(folder).map_err(|_| Refused::NotSaved)?;
    let copy = copy_path(folder, &manifest.id);
    // Written next to it first, so a cut-off write never leaves half a
    // plugin under its name.
    let partial = folder.join(format!("{}.partial", manifest.id.as_str()));
    let _ = fs::remove_file(&partial);
    let written = fs::write(&partial, bytes).and_then(|()| fs::rename(&partial, &copy));
    if written.is_err() {
        let _ = fs::remove_file(&partial);
        return Err(Refused::NotSaved);
    }
    Ok((manifest, added))
}

/// Removes the copy `file` from `folder`. Only a plugin's own file name is
/// taken, never a path. Returns whether it is gone.
pub fn remove(folder: &Path, file: &str) -> bool {
    let own_name = Path::new(file).file_name().and_then(|name| name.to_str()) == Some(file)
        && Path::new(file)
            .extension()
            .and_then(|ending| ending.to_str())
            == Some(ENDING);
    own_name && fs::remove_file(folder.join(file)).is_ok()
}

fn copy_path(folder: &Path, id: &PluginId) -> PathBuf {
    folder.join(format!("{}.{ENDING}", id.as_str()))
}

/// The code and parameters QML shows (D-034). Plugin errors have only
/// numbers, keywords and text from the file as parameters.
pub fn error_text(refused: &Refused) -> ErrorText {
    match refused {
        Refused::Plugin(report) => ErrorText {
            code: report.code,
            params: report
                .params
                .iter()
                .map(|(name, value)| (*name, errors::param_without_book(value)))
                .collect(),
        },
        Refused::Unreadable => ErrorText::plain("input.plugin-file-unreadable"),
        Refused::NoFolder => ErrorText::plain("input.plugin-no-folder"),
        Refused::NotSaved => ErrorText::plain("input.plugin-not-saved"),
    }
}

#[cfg(test)]
#[path = "plugins_tests.rs"]
mod tests;
