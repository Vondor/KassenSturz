// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why a plugin couldn't be loaded or called (D-034). Names from a plugin
//! file are only ever parameters, as data the UI shows as plain text.

use kassensturz_core::{ErrorParam, ErrorReport, Report};
use kassensturz_plugin_api::{Permission, PluginError};

/// An error of the plugin host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// The plugin file or its manifest was refused.
    Package(PluginError),
    /// The plugin belongs to the AI features, which are off (D-013).
    AiOff,
    /// The module isn't valid WebAssembly, or uses a feature the host
    /// doesn't run.
    ModuleInvalid,
    /// The module imports something the host doesn't offer.
    ImportNotAllowed {
        /// The import's module name, as the file has it.
        module: String,
        /// The import's name, as the file has it.
        name: String,
    },
    /// The module's start function failed, or it asked for more memory or
    /// table than the limits allow.
    StartFailed,
    /// The module doesn't export what the host needs.
    ExportMissing {
        /// The export's name.
        name: String,
    },
    /// An export has the wrong kind or type.
    ExportWrongType {
        /// The export's name.
        name: String,
    },
    /// A data-only plugin was called; it has no code.
    NoCode,
    /// The input is bigger than the host passes in.
    InputTooBig {
        /// The most bytes.
        max_bytes: usize,
    },
    /// The plugin gave the host no room for the input.
    InputNotAccepted,
    /// The plugin's output is bigger than the host takes back.
    OutputTooBig {
        /// The most bytes.
        max_bytes: usize,
    },
    /// The plugin's output lies outside its memory, or isn't what the call
    /// should answer.
    OutputInvalid,
    /// The plugin didn't ask for a permission this extension point needs
    /// (D-926), so KassenSturz doesn't call it.
    PermissionMissing {
        /// The permission.
        permission: Permission,
    },
    /// The plugin ran out of fuel and was stopped.
    RanTooLong,
    /// The plugin's code failed (a trap) and was stopped.
    Trapped,
    /// The plugin was stopped earlier and takes no more calls until it is
    /// loaded again.
    Stopped,
}

impl From<PluginError> for HostError {
    fn from(error: PluginError) -> Self {
        Self::Package(error)
    }
}

impl core::fmt::Display for HostError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.report().code)
    }
}

impl std::error::Error for HostError {}

impl Report for HostError {
    fn report(&self) -> ErrorReport {
        let text = |name: &String| ErrorParam::Text(name.clone());
        match self {
            Self::Package(error) => error.report(),
            Self::AiOff => ErrorReport::new("plugin-host.ai-off"),
            Self::ModuleInvalid => ErrorReport::new("plugin-host.module-invalid"),
            Self::ImportNotAllowed { module, name } => {
                ErrorReport::new("plugin-host.import-not-allowed")
                    .with("module", text(module))
                    .with("name", text(name))
            }
            Self::StartFailed => ErrorReport::new("plugin-host.start-failed"),
            Self::ExportMissing { name } => {
                ErrorReport::new("plugin-host.export-missing").with("name", text(name))
            }
            Self::ExportWrongType { name } => {
                ErrorReport::new("plugin-host.export-wrong-type").with("name", text(name))
            }
            Self::NoCode => ErrorReport::new("plugin-host.no-code"),
            Self::InputTooBig { max_bytes } => ErrorReport::new("plugin-host.input-too-big")
                .with("max_bytes", ErrorParam::count(*max_bytes)),
            Self::InputNotAccepted => ErrorReport::new("plugin-host.input-not-accepted"),
            Self::OutputTooBig { max_bytes } => ErrorReport::new("plugin-host.output-too-big")
                .with("max_bytes", ErrorParam::count(*max_bytes)),
            Self::OutputInvalid => ErrorReport::new("plugin-host.output-invalid"),
            Self::PermissionMissing { permission } => {
                ErrorReport::new("plugin-host.permission-missing")
                    .with("permission", ErrorParam::Keyword(permission.key()))
            }
            Self::RanTooLong => ErrorReport::new("plugin-host.ran-too-long"),
            Self::Trapped => ErrorReport::new("plugin-host.trapped"),
            Self::Stopped => ErrorReport::new("plugin-host.stopped"),
        }
    }
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
        let name = || "x".to_owned();
        for error in [
            HostError::Package(PluginError::NotAPlugin),
            HostError::AiOff,
            HostError::ModuleInvalid,
            HostError::ImportNotAllowed {
                module: name(),
                name: name(),
            },
            HostError::StartFailed,
            HostError::ExportMissing { name: name() },
            HostError::ExportWrongType { name: name() },
            HostError::NoCode,
            HostError::InputTooBig { max_bytes: 1 },
            HostError::InputNotAccepted,
            HostError::OutputTooBig { max_bytes: 1 },
            HostError::OutputInvalid,
            HostError::PermissionMissing {
                permission: Permission::ReadBook,
            },
            HostError::PermissionMissing {
                permission: Permission::ProposeChanges,
            },
            HostError::RanTooLong,
            HostError::Trapped,
            HostError::Stopped,
        ] {
            check(&error.report(), &rows, &mut seen);
        }
        let unused = unused(&rows, &seen, &["plugin-host"]);
        assert!(
            unused.is_empty(),
            "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
        );
    }
}
