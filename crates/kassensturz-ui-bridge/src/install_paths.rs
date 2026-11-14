// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keeps the libraries of the Windows package from looking for files in
//! folders a user can create (B-293).
//!
//! The Qt and KDE libraries come from KDE Craft's build, and some of them
//! have the folder of that build compiled in: OpenSSL's configuration file,
//! modules and engines under `C:\Craft...`, libxml2's catalog. On a computer
//! that has no Craft, any user can create such a folder, and a library that
//! trusts it could be made to load a file planted there (CWE-427). The
//! libraries read an environment variable first when it is set, so the app
//! points each one at the install folder before any of them starts. A
//! variable the user already set stays as it is.
//!
//! Nothing changes on other systems, where the paths are the system's own.

use std::path::{Path, PathBuf};

/// The variables to set, each with the file or folder in the install folder
/// (the one that holds `bin`, where the program is). The folders need not
/// exist: a library then finds nothing, and finding nothing is the point.
fn variables(program: &Path) -> Vec<(&'static str, PathBuf)> {
    // `<install>\bin\kassensturz.exe`: two steps up.
    let Some(install) = program.parent().and_then(Path::parent) else {
        return Vec::new();
    };
    vec![
        ("OPENSSL_CONF", install.join("ssl").join("openssl.cnf")),
        ("OPENSSL_MODULES", install.join("lib").join("ossl-modules")),
        ("OPENSSL_ENGINES", install.join("lib").join("engines-3")),
        (
            "XML_CATALOG_FILES",
            install.join("etc").join("xml").join("catalog"),
        ),
    ]
}

/// Points the libraries at the install folder. Call it first in `main`,
/// before Qt or anything else starts, on Windows; elsewhere it does nothing.
pub fn apply_at_start() {
    if !cfg!(windows) {
        return;
    }
    let Ok(program) = std::env::current_exe() else {
        return;
    };
    for (name, path) in variables(&program) {
        if std::env::var_os(name).is_some() {
            continue;
        }
        if let Some(path) = path.to_str() {
            // SAFETY: nothing but `main`'s first statements runs yet, so no
            // other thread reads or writes the environment.
            unsafe { crate::language::set_for_qt(name, path) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_path_is_in_the_install_folder() {
        let program = Path::new("install").join("bin").join("kassensturz.exe");
        let found = variables(&program);
        let names: Vec<&str> = found.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "OPENSSL_CONF",
                "OPENSSL_MODULES",
                "OPENSSL_ENGINES",
                "XML_CATALOG_FILES"
            ]
        );
        for (name, path) in found {
            assert!(path.starts_with("install"), "{name} is {path:?}");
            assert!(
                !path.starts_with(Path::new("install").join("bin")),
                "{name}"
            );
            let text = path.to_string_lossy().to_lowercase();
            assert!(!text.contains("craft"), "{name} is {text}");
        }
    }

    #[test]
    fn a_program_without_an_install_folder_sets_nothing() {
        assert!(variables(Path::new("kassensturz.exe")).is_empty());
    }
}
