// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Loading the libraries by their ABI names and checking their versions
//! (D-054, D-138).

use std::ffi::OsStr;
use std::fmt;

use crate::aqbanking::AqBanking;
use crate::gwenhywfar::Gwenhywfar;

/// The file name Gwenhywfar is loaded by: its ABI name, so that only a
/// build the bindings were made for is picked up (D-138). Windows and macOS
/// follow libtool's naming for the same ABI number; those names are checked
/// when their builds get AqBanking (M7).
#[cfg(all(unix, not(target_os = "macos")))]
pub const GWENHYWFAR_LIBRARY: &str = "libgwenhywfar.so.79";
/// See the Linux value.
#[cfg(target_os = "macos")]
pub const GWENHYWFAR_LIBRARY: &str = "libgwenhywfar.79.dylib";
/// See the Linux value.
#[cfg(windows)]
pub const GWENHYWFAR_LIBRARY: &str = "libgwenhywfar-79.dll";

/// The file name AqBanking is loaded by (D-138); see [`GWENHYWFAR_LIBRARY`].
#[cfg(all(unix, not(target_os = "macos")))]
pub const AQBANKING_LIBRARY: &str = "libaqbanking.so.44";
/// See the Linux value.
#[cfg(target_os = "macos")]
pub const AQBANKING_LIBRARY: &str = "libaqbanking.44.dylib";
/// See the Linux value.
#[cfg(windows)]
pub const AQBANKING_LIBRARY: &str = "libaqbanking-44.dll";

/// One of the two libraries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Library {
    /// Gwenhywfar, the support library AqBanking needs (GUI callbacks, dates,
    /// TLS).
    Gwenhywfar,
    /// AqBanking itself.
    AqBanking,
}

impl Library {
    /// The library's name as it is written.
    pub const fn name(self) -> &'static str {
        match self {
            Library::Gwenhywfar => "Gwenhywfar",
            Library::AqBanking => "AqBanking",
        }
    }

    /// The file name the library is loaded by.
    pub const fn file_name(self) -> &'static str {
        match self {
            Library::Gwenhywfar => GWENHYWFAR_LIBRARY,
            Library::AqBanking => AQBANKING_LIBRARY,
        }
    }

    /// The oldest version that is used (D-138): the one the bindings were
    /// generated from.
    pub const fn minimum_version(self) -> Version {
        match self {
            Library::Gwenhywfar => Version::new(5, 14, 1),
            Library::AqBanking => Version::new(6, 9, 2),
        }
    }
}

/// A version as the libraries report it: `GWEN_Version` and
/// `AB_Banking_GetVersion` fill four integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    /// Major version, the first number.
    pub major: i32,
    /// Minor version, the second number.
    pub minor: i32,
    /// Patch level, the third number.
    pub patch: i32,
    /// Build number, the fourth; 0 for releases.
    pub build: i32,
}

impl Version {
    /// A release version with build number 0.
    pub const fn new(major: i32, minor: i32, patch: i32) -> Self {
        Version {
            major,
            minor,
            patch,
            build: 0,
        }
    }

    /// Whether this version is at least `minimum` (ordered by major, minor,
    /// patch, then build).
    pub fn meets(self, minimum: Version) -> bool {
        self >= minimum
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if self.build != 0 {
            write!(f, ".{}", self.build)?;
        }
        Ok(())
    }
}

/// Why the libraries couldn't be used. All three mean "not installed" to
/// the user (D-054); the difference is for the log.
#[derive(Debug)]
pub enum LoadError {
    /// The library file wasn't found or couldn't be opened.
    NotFound {
        /// Which library.
        library: Library,
        /// The loader's own message, for the log.
        reason: String,
    },
    /// The library was found, but lacks a function the bindings declare: a
    /// different build than the one the bindings were made for (D-138).
    MissingFunction {
        /// Which library.
        library: Library,
        /// The loader's own message, for the log.
        reason: String,
    },
    /// The library was found but is older than the minimum (D-138).
    TooOld {
        /// Which library.
        library: Library,
        /// The version it reports.
        found: Version,
        /// The oldest version that is used.
        minimum: Version,
    },
}

impl LoadError {
    /// Which library the error is about.
    pub fn library(&self) -> Library {
        match self {
            LoadError::NotFound { library, .. }
            | LoadError::MissingFunction { library, .. }
            | LoadError::TooOld { library, .. } => *library,
        }
    }

    fn from_loader(library: Library, error: libloading::Error) -> Self {
        let reason = error.to_string();
        match error {
            libloading::Error::DlSym { .. }
            | libloading::Error::DlSymUnknown
            | libloading::Error::GetProcAddress { .. }
            | libloading::Error::GetProcAddressUnknown => {
                LoadError::MissingFunction { library, reason }
            }
            _ => LoadError::NotFound { library, reason },
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::NotFound { library, reason } => {
                write!(
                    f,
                    "{} ({}) not found: {reason}",
                    library.name(),
                    library.file_name()
                )
            }
            LoadError::MissingFunction { library, reason } => write!(
                f,
                "{} ({}) is a different build than the bindings were made for: {reason}",
                library.name(),
                library.file_name()
            ),
            LoadError::TooOld {
                library,
                found,
                minimum,
            } => write!(
                f,
                "{} {found} is too old, {minimum} or newer is needed",
                library.name()
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// Both libraries, loaded and checked.
///
/// Dropping it unloads them, so it must outlive every pointer obtained from
/// them; a program keeps one for as long as it uses AqBanking.
pub struct Libraries {
    gwenhywfar: Gwenhywfar,
    aqbanking: AqBanking,
    gwenhywfar_version: Version,
    aqbanking_version: Version,
}

impl Libraries {
    /// Loads Gwenhywfar and AqBanking by their ABI names and checks their
    /// versions (D-138). Nothing else is called: no `GWEN_Init`, no
    /// `AB_Banking_new`.
    ///
    /// # Safety
    ///
    /// Loading a shared library runs its initialisation code, which Rust
    /// can't check; see `libloading::Library::new`. The caller must keep the
    /// returned value alive while any pointer from the libraries is in use.
    pub unsafe fn load() -> Result<Self, LoadError> {
        unsafe { Self::load_from(GWENHYWFAR_LIBRARY, AQBANKING_LIBRARY) }
    }

    /// Like [`Libraries::load`], but from the given file names or paths:
    /// for tests with a stub library (M4.5), never for a real bank.
    ///
    /// # Safety
    ///
    /// As for [`Libraries::load`].
    pub unsafe fn load_from<P: AsRef<OsStr>>(
        gwenhywfar: P,
        aqbanking: P,
    ) -> Result<Self, LoadError> {
        // Gwenhywfar first: AqBanking links it, so the loader would pull it in
        // anyway, but this way its version is checked before anything else.
        let gwenhywfar = unsafe { Gwenhywfar::new(gwenhywfar) }
            .map_err(|e| LoadError::from_loader(Library::Gwenhywfar, e))?;
        let gwenhywfar_version = read_version(|major, minor, patch, build| unsafe {
            gwenhywfar.GWEN_Version(major, minor, patch, build)
        });
        check_minimum(Library::Gwenhywfar, gwenhywfar_version)?;

        let aqbanking = unsafe { AqBanking::new(aqbanking) }
            .map_err(|e| LoadError::from_loader(Library::AqBanking, e))?;
        let aqbanking_version = read_version(|major, minor, patch, build| unsafe {
            aqbanking.AB_Banking_GetVersion(major, minor, patch, build)
        });
        check_minimum(Library::AqBanking, aqbanking_version)?;

        Ok(Libraries {
            gwenhywfar,
            aqbanking,
            gwenhywfar_version,
            aqbanking_version,
        })
    }

    /// Gwenhywfar's functions.
    pub fn gwenhywfar(&self) -> &Gwenhywfar {
        &self.gwenhywfar
    }

    /// AqBanking's functions.
    pub fn aqbanking(&self) -> &AqBanking {
        &self.aqbanking
    }

    /// The version Gwenhywfar reports.
    pub fn gwenhywfar_version(&self) -> Version {
        self.gwenhywfar_version
    }

    /// The version AqBanking reports.
    pub fn aqbanking_version(&self) -> Version {
        self.aqbanking_version
    }
}

/// Calls a version function that fills four integers.
fn read_version(get: impl FnOnce(*mut i32, *mut i32, *mut i32, *mut i32)) -> Version {
    let (mut major, mut minor, mut patch, mut build) = (0i32, 0i32, 0i32, 0i32);
    get(&mut major, &mut minor, &mut patch, &mut build);
    Version {
        major,
        minor,
        patch,
        build,
    }
}

fn check_minimum(library: Library, found: Version) -> Result<(), LoadError> {
    let minimum = library.minimum_version();
    if found.meets(minimum) {
        Ok(())
    } else {
        Err(LoadError::TooOld {
            library,
            found,
            minimum,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_versions_meet_the_minimum() {
        assert!(Version::new(5, 14, 1).meets(Library::Gwenhywfar.minimum_version()));
        assert!(Version::new(6, 9, 2).meets(Library::AqBanking.minimum_version()));
    }

    #[test]
    fn newer_versions_meet_the_minimum_and_older_ones_do_not() {
        let minimum = Library::Gwenhywfar.minimum_version();
        assert!(Version::new(5, 14, 2).meets(minimum));
        assert!(Version::new(5, 15, 0).meets(minimum));
        assert!(Version::new(6, 0, 0).meets(minimum));
        assert!(
            Version {
                build: 1,
                ..Version::new(5, 14, 1)
            }
            .meets(minimum)
        );
        assert!(!Version::new(5, 14, 0).meets(minimum));
        assert!(!Version::new(5, 13, 9).meets(minimum));
        assert!(!Version::new(4, 20, 0).meets(minimum));
    }

    #[test]
    fn versions_are_written_like_the_releases_name_them() {
        assert_eq!(Version::new(6, 9, 2).to_string(), "6.9.2");
        assert_eq!(
            Version {
                build: 3,
                ..Version::new(5, 14, 1)
            }
            .to_string(),
            "5.14.1.3"
        );
    }

    #[test]
    fn too_old_is_reported_with_both_versions() {
        let error = check_minimum(Library::AqBanking, Version::new(6, 5, 4)).unwrap_err();
        assert_eq!(error.library(), Library::AqBanking);
        assert_eq!(
            error.to_string(),
            "AqBanking 6.5.4 is too old, 6.9.2 or newer is needed"
        );
    }

    #[test]
    fn a_missing_function_is_told_apart_from_a_missing_library() {
        let missing = LoadError::from_loader(Library::AqBanking, libloading::Error::DlSymUnknown);
        assert!(matches!(missing, LoadError::MissingFunction { .. }));
        assert!(missing.to_string().contains("different build"));
        let not_found =
            LoadError::from_loader(Library::Gwenhywfar, libloading::Error::DlOpenUnknown);
        assert!(matches!(not_found, LoadError::NotFound { .. }));
        assert!(
            not_found
                .to_string()
                .starts_with("Gwenhywfar (libgwenhywfar")
        );
    }

    #[test]
    fn library_names_are_abi_names() {
        // The ABI numbers come from both libraries' configure.ac (D-138).
        assert!(Library::Gwenhywfar.file_name().contains("79"));
        assert!(Library::AqBanking.file_name().contains("44"));
    }
}
