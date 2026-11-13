// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Decides whether this build carries a FinTS product registration number
//! (D-745).
//!
//! The registration office allows the number inside a finished program
//! that is shipped unchanged as the registered product, and not in public
//! source. So the source holds none. A build gets one only from outside
//! the repository:
//!
//! - the variable `KASSENSTURZ_FINTS_REGISTRATION`, or
//! - the file `fints-registration` in the repository's root, which git
//!   ignores (the Flatpak and Windows builds copy the checkout, as they do
//!   for `build-version`).
//!
//! The release workflow writes that file from a protected secret, for a
//! release tag only. Whoever builds KassenSturz themselves and has their
//! own registration puts their own number there. Without either, the build
//! has none and says `bank.not-registered` (or asks at run time, D-260).
//!
//! No source file ever holds the number, not even a generated one, so a
//! package of debug sources can't pick it up: it reaches the compiler as
//! the variable `KASSENSTURZ_FINTS_REGISTRATION_BUILT_IN`, which
//! `availability.rs` reads with `option_env!`. Cargo shows that line only
//! in a very verbose build (`-vv`), which the release workflow never
//! uses. A value of the wrong shape stops the build without showing it.

use std::path::Path;

include!("src/shape.rs");

/// The variable a build may carry the number in.
const VARIABLE: &str = "KASSENSTURZ_FINTS_REGISTRATION";
/// The git-ignored file a build may carry the number in.
const FILE: &str = "../../fints-registration";
/// The variable the compiler gets the checked number in.
const BUILT_IN: &str = "KASSENSTURZ_FINTS_REGISTRATION_BUILT_IN";

fn main() {
    println!("cargo:rerun-if-env-changed={VARIABLE}");
    // A file that isn't there would make cargo run this script, and build
    // the crate, on every build. So the file is watched only once it
    // exists; after adding it, `cargo clean -p kassensturz-bank` makes the
    // next build read it (README, "Bank access in your own build").
    if Path::new(FILE).exists() {
        println!("cargo:rerun-if-changed={FILE}");
    }
    println!("cargo:rerun-if-changed=src/shape.rs");

    let given = std::env::var(VARIABLE)
        .ok()
        .or_else(|| std::fs::read_to_string(FILE).ok())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());

    match given {
        None => {}
        Some(key) if is_registration_shape(&key) => {
            println!("cargo:rustc-env={BUILT_IN}={key}");
        }
        Some(_) => {
            // The value itself stays out of the message.
            eprintln!(
                "error: the FinTS registration number given to this build ({VARIABLE} or the \
                 file fints-registration) is not exactly {REGISTRATION_KEY_LENGTH} letters and \
                 digits."
            );
            std::process::exit(1);
        }
    }
}
