// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a plugin can't be added (D-034). Text from a manifest is only ever a
//! parameter, as data the UI shows as plain text, never part of a message.

use kassensturz_core::{ErrorParam, ErrorReport, Report};

use crate::{ApiVersion, Permission};

/// An error of the plugin API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// The manifest is bigger than KassenSturz reads.
    ManifestTooBig {
        /// The most bytes a manifest may have.
        max_bytes: usize,
    },
    /// The manifest isn't a JSON object.
    ManifestMalformed,
    /// A value of the manifest breaks a rule of D-847.
    ManifestInvalid {
        /// What is wrong, by a fixed word.
        reason: &'static str,
        /// Which key; data, never part of a message.
        key: String,
    },
    /// The manifest is in a format this KassenSturz doesn't know.
    UnsupportedManifestVersion {
        /// The version the manifest says it has.
        version: i64,
    },
    /// The plugin is for another major version of the plugin API.
    ApiUnsupported {
        /// The version it targets.
        api: ApiVersion,
    },
    /// The plugin needs a newer KassenSturz.
    ApiTooNew {
        /// The version it targets.
        api: ApiVersion,
    },
    /// The manifest names an extension point API v1 doesn't have.
    UnknownExtensionPoint {
        /// The word, as the manifest has it.
        name: String,
    },
    /// The manifest asks for a permission API v1 doesn't have.
    UnknownPermission {
        /// The word, as the manifest has it.
        name: String,
    },
    /// The manifest asks for a permission none of its extension points may
    /// have (D-848).
    PermissionNotAllowed {
        /// The permission.
        permission: Permission,
    },
    /// A theme or UI density plugin also names an extension point that runs
    /// code (D-756).
    DataOnlyMixed,
    /// The plugin file is bigger than KassenSturz reads (D-849).
    FileTooBig {
        /// The most bytes a plugin file may have.
        max_bytes: usize,
    },
    /// The file isn't a WebAssembly module, or it is cut off or damaged.
    NotAPlugin,
    /// The module has no manifest section.
    ManifestMissing,
    /// The module has two manifest sections.
    ManifestTwice,
    /// The module has a KassenSturz section this version doesn't know.
    UnknownSection {
        /// The section's name, as the file has it.
        name: String,
    },
    /// A theme or UI density plugin holds code (D-756).
    DataOnlyHasCode,
    /// A theme plugin has no theme section (D-925).
    ThemeMissing,
    /// The module has two theme sections.
    ThemeTwice,
    /// The module has a theme section but isn't a theme plugin.
    ThemeUnexpected,
    /// The theme's data is bigger than KassenSturz reads.
    ThemeTooBig {
        /// The most bytes a theme's data may have.
        max_bytes: usize,
    },
    /// The theme's data isn't a JSON object.
    ThemeMalformed,
    /// The theme is in a format this KassenSturz doesn't know.
    UnsupportedThemeVersion {
        /// The version the theme says it has.
        version: i64,
    },
    /// A value of the theme breaks a rule of D-925.
    ThemeInvalid {
        /// What is wrong, by a fixed word.
        reason: &'static str,
        /// Which key; data, never part of a message.
        key: String,
    },
    /// A colour of the theme doesn't stand out enough from its background.
    ThemeLowContrast {
        /// Which colour, such as `light.text`.
        key: String,
    },
}

impl core::fmt::Display for PluginError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.report().code)
    }
}

impl std::error::Error for PluginError {}

impl Report for PluginError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::ManifestTooBig { max_bytes } => ErrorReport::new("plugin.manifest-too-big")
                .with("max_bytes", ErrorParam::count(*max_bytes)),
            Self::ManifestMalformed => ErrorReport::new("plugin.manifest-malformed"),
            Self::ManifestInvalid { reason, key } => ErrorReport::new("plugin.manifest-invalid")
                .with("reason", ErrorParam::Keyword(reason))
                .with("key", ErrorParam::Text(key.clone())),
            Self::UnsupportedManifestVersion { version } => {
                ErrorReport::new("plugin.unsupported-manifest-version")
                    .with("version", ErrorParam::Number(*version))
            }
            Self::ApiUnsupported { api } => api_report("plugin.api-unsupported", *api),
            Self::ApiTooNew { api } => api_report("plugin.api-too-new", *api),
            Self::UnknownExtensionPoint { name } => {
                ErrorReport::new("plugin.unknown-extension-point")
                    .with("name", ErrorParam::Text(name.clone()))
            }
            Self::UnknownPermission { name } => ErrorReport::new("plugin.unknown-permission")
                .with("name", ErrorParam::Text(name.clone())),
            Self::PermissionNotAllowed { permission } => {
                ErrorReport::new("plugin.permission-not-allowed")
                    .with("permission", ErrorParam::Keyword(permission.key()))
            }
            Self::DataOnlyMixed => ErrorReport::new("plugin.data-only-mixed"),
            Self::FileTooBig { max_bytes } => ErrorReport::new("plugin.file-too-big")
                .with("max_bytes", ErrorParam::count(*max_bytes)),
            Self::NotAPlugin => ErrorReport::new("plugin.not-a-plugin"),
            Self::ManifestMissing => ErrorReport::new("plugin.manifest-missing"),
            Self::ManifestTwice => ErrorReport::new("plugin.manifest-twice"),
            Self::UnknownSection { name } => ErrorReport::new("plugin.unknown-section")
                .with("name", ErrorParam::Text(name.clone())),
            Self::DataOnlyHasCode => ErrorReport::new("plugin.data-only-has-code"),
            Self::ThemeMissing => ErrorReport::new("plugin.theme-missing"),
            Self::ThemeTwice => ErrorReport::new("plugin.theme-twice"),
            Self::ThemeUnexpected => ErrorReport::new("plugin.theme-unexpected"),
            Self::ThemeTooBig { max_bytes } => ErrorReport::new("plugin.theme-too-big")
                .with("max_bytes", ErrorParam::count(*max_bytes)),
            Self::ThemeMalformed => ErrorReport::new("plugin.theme-malformed"),
            Self::UnsupportedThemeVersion { version } => {
                ErrorReport::new("plugin.unsupported-theme-version")
                    .with("version", ErrorParam::Number(*version))
            }
            Self::ThemeInvalid { reason, key } => ErrorReport::new("plugin.theme-invalid")
                .with("reason", ErrorParam::Keyword(reason))
                .with("key", ErrorParam::Text(key.clone())),
            Self::ThemeLowContrast { key } => ErrorReport::new("plugin.theme-low-contrast")
                .with("key", ErrorParam::Text(key.clone())),
        }
    }
}

fn api_report(code: &'static str, api: ApiVersion) -> ErrorReport {
    ErrorReport::new(code)
        .with("major", ErrorParam::Number(api.major.into()))
        .with("minor", ErrorParam::Number(api.minor.into()))
}

// The list of error codes lives in `kassensturz-store`'s source, so that the
// crates that must not depend on each other all check their codes the same
// way.
#[cfg(test)]
#[path = "../../kassensturz-store/src/error/list.rs"]
mod list;
#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::list::{check, documented, unused};
    use super::*;

    #[test]
    fn every_code_is_listed_with_its_parameters() {
        let rows = documented();
        let mut seen = HashSet::new();
        let api = ApiVersion { major: 2, minor: 0 };
        let mut errors = vec![
            PluginError::ManifestTooBig { max_bytes: 1 },
            PluginError::ManifestMalformed,
            PluginError::UnsupportedManifestVersion { version: 2 },
            PluginError::ApiUnsupported { api },
            PluginError::ApiTooNew { api },
            PluginError::UnknownExtensionPoint { name: "x".into() },
            PluginError::UnknownPermission { name: "x".into() },
            PluginError::DataOnlyMixed,
            PluginError::FileTooBig { max_bytes: 1 },
            PluginError::NotAPlugin,
            PluginError::ManifestMissing,
            PluginError::ManifestTwice,
            PluginError::UnknownSection { name: "x".into() },
            PluginError::DataOnlyHasCode,
            PluginError::ThemeMissing,
            PluginError::ThemeTwice,
            PluginError::ThemeUnexpected,
            PluginError::ThemeTooBig { max_bytes: 1 },
            PluginError::ThemeMalformed,
            PluginError::UnsupportedThemeVersion { version: 2 },
            PluginError::ThemeLowContrast { key: "x".into() },
        ];
        for reason in [
            "missing",
            "unknown-key",
            "wrong-type",
            "empty",
            "too-long",
            "too-many",
            "bad-characters",
            "malformed",
            "duplicate",
        ] {
            errors.push(PluginError::ManifestInvalid {
                reason,
                key: "name".into(),
            });
        }
        for reason in [
            "missing",
            "unknown-key",
            "wrong-type",
            "empty",
            "too-long",
            "bad-characters",
            "malformed",
            "wrong-way",
            "out-of-range",
        ] {
            errors.push(PluginError::ThemeInvalid {
                reason,
                key: "font".into(),
            });
        }
        for permission in Permission::ALL {
            errors.push(PluginError::PermissionNotAllowed { permission });
        }
        for error in errors {
            let report = error.report();
            assert!(report.code.starts_with("plugin."), "{}", report.code);
            check(&report, &rows, &mut seen);
        }
        let unused = unused(&rows, &seen, &["plugin"]);
        assert!(
            unused.is_empty(),
            "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
        );
    }
}
