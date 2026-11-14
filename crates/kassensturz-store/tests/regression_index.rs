// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keeps `docs/REGRESSION_TESTS.md` honest (D-787): every test it names in
//! backticks still exists somewhere in the workspace, so a regression test
//! that is renamed or removed fails here until the page is updated.

use std::path::{Path, PathBuf};

fn rust_files(folder: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).expect("folder read").flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn every_test_the_regression_page_names_exists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let page = std::fs::read_to_string(root.join("docs/REGRESSION_TESTS.md")).expect("page read");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    let code: String = files
        .iter()
        .map(|file| std::fs::read_to_string(file).expect("source read"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut named = 0;
    let mut missing = Vec::new();
    for name in page.split('`').skip(1).step_by(2) {
        // Test names are long snake_case words; paths, commands and file
        // names have other characters.
        let is_name = name.len() >= 12
            && name.contains('_')
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !is_name {
            continue;
        }
        named += 1;
        if !code.contains(&format!("fn {name}(")) {
            missing.push(name);
        }
    }
    assert!(
        named > 40,
        "only {named} names found: the page changed shape"
    );
    assert!(
        missing.is_empty(),
        "named on the page, not in the code: {missing:?}"
    );
}
