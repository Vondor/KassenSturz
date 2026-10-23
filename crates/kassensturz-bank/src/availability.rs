// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether bank connections are available on this computer: AqBanking and
//! Gwenhywfar are found at runtime and checked against the versions the
//! bindings were made for (D-054, D-138, threat T-17).

use std::fmt;

use crate::error::BankError;

/// The oldest AqBanking KassenSturz works with (D-138).
pub const AQBANKING_MINIMUM: Version = Version::new(6, 9, 2);
/// The oldest Gwenhywfar KassenSturz works with (D-138).
pub const GWENHYWFAR_MINIMUM: Version = Version::new(5, 14, 1);

/// The file name AqBanking is loaded by: its ABI name, so a library with
/// another ABI is never loaded (D-138).
pub const AQBANKING_LIBRARY: &str = "libaqbanking.so.44";
/// The file name Gwenhywfar is loaded by (D-138).
pub const GWENHYWFAR_LIBRARY: &str = "libgwenhywfar.so.79";
/// The file name of Gwenhywfar's Qt 6 dialogs, loaded through KassenSturz's
/// own C++ library (Q-035, D-138).
pub const GWENGUI_QT6_LIBRARY: &str = "libgwengui-qt6.so.79";

/// KassenSturz's own FinTS product registration number (D-058, Q-087).
///
/// `None` until the registration office has issued it: live bank access is
/// refused with `bank.not-registered` in the meantime. A fork under another
/// name registers its own number; never use another program's.
pub const FINTS_REGISTRATION_KEY: Option<&str> = None;

/// A library's version, as `major.minor.patch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// The first number.
    pub major: u32,
    /// The second number.
    pub minor: u32,
    /// The third number.
    pub patch: u32,
}

impl Version {
    /// The version `major.minor.patch`.
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Reads `6.9.2` or `6.9.2beta`: the numbers up to the first character
    /// that isn't a digit or a dot. A missing number counts as 0. `None` if
    /// there is no number at the start.
    pub fn parse(text: &str) -> Option<Self> {
        let numeric: String = text
            .trim()
            .chars()
            .take_while(|character| character.is_ascii_digit() || *character == '.')
            .collect();
        let mut parts = numeric.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().map_or(Some(0), |part| part.parse().ok())?;
        let patch = parts.next().map_or(Some(0), |part| part.parse().ok())?;
        Some(Self::new(major, minor, patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// One of the libraries bank connections need.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Library {
    /// AqBanking, which speaks FinTS.
    AqBanking,
    /// Gwenhywfar, AqBanking's base library, with the dialogs.
    Gwenhywfar,
}

impl Library {
    /// The keyword for it in `docs/ERROR_CODES.md`.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::AqBanking => "aqbanking",
            Self::Gwenhywfar => "gwenhywfar",
        }
    }

    /// The oldest version KassenSturz works with (D-138).
    pub const fn minimum(self) -> Version {
        match self {
            Self::AqBanking => AQBANKING_MINIMUM,
            Self::Gwenhywfar => GWENHYWFAR_MINIMUM,
        }
    }
}

/// Whether bank connections work on this computer. The app works fully
/// without them, just without bank connections, and says what to install
/// (D-054).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// Both libraries are there and new enough.
    Available {
        /// The AqBanking found.
        aqbanking: Version,
        /// The Gwenhywfar found.
        gwenhywfar: Version,
    },
    /// At least one library is missing, or has another ABI than the one
    /// KassenSturz loads (D-138).
    NotInstalled,
    /// A library is there but older than KassenSturz needs.
    TooOld {
        /// Which one.
        library: Library,
        /// The version found.
        found: Version,
        /// The oldest version KassenSturz works with.
        minimum: Version,
    },
}

impl Availability {
    /// Judges the versions found: `None` for a library that couldn't be
    /// loaded. AqBanking is judged first, since it is the one the user
    /// knows by name.
    pub fn check(aqbanking: Option<Version>, gwenhywfar: Option<Version>) -> Self {
        let (Some(aqbanking), Some(gwenhywfar)) = (aqbanking, gwenhywfar) else {
            return Self::NotInstalled;
        };
        for (library, found) in [
            (Library::AqBanking, aqbanking),
            (Library::Gwenhywfar, gwenhywfar),
        ] {
            if found < library.minimum() {
                return Self::TooOld {
                    library,
                    found,
                    minimum: library.minimum(),
                };
            }
        }
        Self::Available {
            aqbanking,
            gwenhywfar,
        }
    }

    /// Whether bank connections can be used.
    pub const fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }

    /// Fails unless bank connections can be used, with the error that says
    /// why not.
    pub fn require(&self) -> Result<(), BankError> {
        match *self {
            Self::Available { .. } => Ok(()),
            Self::NotInstalled => Err(BankError::NotInstalled),
            Self::TooOld {
                library,
                found,
                minimum,
            } => Err(BankError::TooOld {
                library,
                found,
                minimum,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_read_from_the_libraries_text() {
        assert_eq!(Version::parse("6.9.2"), Some(Version::new(6, 9, 2)));
        assert_eq!(Version::parse("5.14.1beta"), Some(Version::new(5, 14, 1)));
        assert_eq!(Version::parse(" 7 "), Some(Version::new(7, 0, 0)));
        assert_eq!(Version::parse("6.9"), Some(Version::new(6, 9, 0)));
        assert_eq!(Version::parse("beta"), None);
        assert_eq!(Version::parse(""), None);
        assert_eq!(Version::new(6, 9, 2).to_string(), "6.9.2");
    }

    #[test]
    fn versions_compare_number_by_number() {
        assert!(Version::new(6, 10, 0) > Version::new(6, 9, 2));
        assert!(Version::new(7, 0, 0) > Version::new(6, 99, 99));
        assert!(Version::new(6, 9, 2) >= AQBANKING_MINIMUM);
    }

    #[test]
    fn the_minimums_are_the_versions_the_bindings_were_made_for() {
        assert_eq!(AQBANKING_MINIMUM.to_string(), "6.9.2");
        assert_eq!(GWENHYWFAR_MINIMUM.to_string(), "5.14.1");
        assert_eq!(AQBANKING_LIBRARY, "libaqbanking.so.44");
        assert_eq!(GWENHYWFAR_LIBRARY, "libgwenhywfar.so.79");
        assert_eq!(GWENGUI_QT6_LIBRARY, "libgwengui-qt6.so.79");
    }

    #[test]
    fn a_missing_library_counts_as_not_installed() {
        assert_eq!(
            Availability::check(None, Some(GWENHYWFAR_MINIMUM)),
            Availability::NotInstalled
        );
        assert_eq!(
            Availability::check(Some(AQBANKING_MINIMUM), None),
            Availability::NotInstalled
        );
        assert_eq!(
            Availability::NotInstalled.require(),
            Err(BankError::NotInstalled)
        );
    }

    #[test]
    fn an_old_library_is_named_with_the_version_needed() {
        let old = Version::new(6, 5, 0);
        let availability = Availability::check(Some(old), Some(GWENHYWFAR_MINIMUM));
        assert_eq!(
            availability,
            Availability::TooOld {
                library: Library::AqBanking,
                found: old,
                minimum: AQBANKING_MINIMUM,
            }
        );
        assert!(!availability.is_available());
        let old = Version::new(5, 10, 2);
        assert_eq!(
            Availability::check(Some(AQBANKING_MINIMUM), Some(old)),
            Availability::TooOld {
                library: Library::Gwenhywfar,
                found: old,
                minimum: GWENHYWFAR_MINIMUM,
            }
        );
    }

    #[test]
    fn the_minimums_and_newer_are_available() {
        let newer = Version::new(6, 10, 0);
        let availability = Availability::check(Some(newer), Some(GWENHYWFAR_MINIMUM));
        assert!(availability.is_available());
        assert_eq!(availability.require(), Ok(()));
    }

    #[test]
    fn there_is_no_registration_number_yet() {
        // Q-087: live FinTS isn't tried until the maintainer's number is in.
        assert_eq!(FINTS_REGISTRATION_KEY, None);
    }
}
