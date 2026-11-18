// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin API's version (D-753, D-847).

use crate::PluginError;

/// The plugin API version this KassenSturz offers.
///
/// A plugin that targets the same major version and an equal or lower minor
/// version runs; a newer minor version needs a newer KassenSturz, and another
/// major version is refused. A minor step only adds; anything that would
/// break a plugin is a major step (a human gate, AGENTS.md §3.2).
pub const API_VERSION: ApiVersion = ApiVersion { major: 1, minor: 0 };

/// A plugin API version, `MAJOR.MINOR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApiVersion {
    /// Changes when the API breaks plugins written for the one before.
    pub major: u16,
    /// Changes when the API only adds something.
    pub minor: u16,
}

impl ApiVersion {
    /// Reads `MAJOR.MINOR` (digits only, no leading zeros, at most four
    /// digits each). Returns `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        let (major, minor) = text.split_once('.')?;
        Some(Self {
            major: u16::try_from(number(major, 4)?).ok()?,
            minor: u16::try_from(number(minor, 4)?).ok()?,
        })
    }

    /// Whether a plugin targeting this version runs on this KassenSturz:
    /// `Ok` if it does, otherwise why not.
    pub fn check_supported(self) -> Result<(), PluginError> {
        if self.major != API_VERSION.major {
            return Err(PluginError::ApiUnsupported { api: self });
        }
        if self.minor > API_VERSION.minor {
            return Err(PluginError::ApiTooNew { api: self });
        }
        Ok(())
    }
}

impl core::fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// A whole number of at most `max_digits` ASCII digits, without a leading
/// zero (except `0` itself).
pub(crate) fn number(text: &str, max_digits: usize) -> Option<u32> {
    let valid = !text.is_empty()
        && text.len() <= max_digits
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if valid { text.parse().ok() } else { None }
}
