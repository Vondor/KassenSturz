// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Loads the real libraries by their ABI names and reads their versions
//! (D-138). Nothing else is called, so no bank data and no secrets are
//! involved (D-055).
//!
//! Where AqBanking isn't installed the test passes after saying so, unless
//! `KASSENSTURZ_REQUIRE_AQBANKING` is set, as it is in the CI jobs whose
//! distribution packages provide the libraries (Kubuntu, Fedora).

use kassensturz_aqbanking_sys::{Libraries, Library, LoadError};

#[test]
fn the_installed_libraries_load_and_meet_the_minimum_versions() {
    let required = std::env::var_os("KASSENSTURZ_REQUIRE_AQBANKING").is_some();
    // SAFETY: loading runs the libraries' initialisers; the value lives to
    // the end of the test and no pointer is taken from it.
    let loaded = unsafe { Libraries::load() };
    match loaded {
        Ok(libraries) => {
            let gwenhywfar = libraries.gwenhywfar_version();
            let aqbanking = libraries.aqbanking_version();
            println!("Gwenhywfar {gwenhywfar}, AqBanking {aqbanking}");
            assert!(gwenhywfar.meets(Library::Gwenhywfar.minimum_version()));
            assert!(aqbanking.meets(Library::AqBanking.minimum_version()));
        }
        Err(error @ LoadError::NotFound { .. }) if !required => {
            println!("AqBanking is not installed here, nothing to load: {error}");
        }
        Err(error) => panic!("the libraries could not be used: {error}"),
    }
}

#[test]
fn a_library_that_does_not_exist_is_reported_as_not_found() {
    // SAFETY: nothing is loaded; the name doesn't exist.
    let error = unsafe {
        Libraries::load_from("libkassensturz-no-such-library.so.0", "libaqbanking.so.44")
    }
    .err()
    .expect("a library that doesn't exist can't load");
    assert!(matches!(
        error,
        LoadError::NotFound {
            library: Library::Gwenhywfar,
            ..
        }
    ));
}
