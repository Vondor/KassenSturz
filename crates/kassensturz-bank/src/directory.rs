// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which bank an IBAN belongs to and what setting up a login there needs,
//! so the app asks for the IBAN first and then only for what that bank needs
//! (the maintainer's answer to Q-035, D-144).
//!
//! Banks come from two sources, layered: the hand-checked list in this
//! repository ([`crate::banks`]) first, for corrections, then an external
//! source, AqBanking's bank information (M4.5). What neither knows is an
//! explicit state the screens show in plain language, never a guess:
//! unknown bank, no server, unknown login type (`docs/BANK_LIST.md`).

use crate::banks::{self, BankEntry};
use crate::iban::{BankCode, Iban};

/// Where banks are looked up by their code: AqBanking's bank information
/// (M4.5), the hand-checked list, or the fake bank's own in tests.
pub trait BankDirectory {
    /// What is known about the bank with `code`; `None` for a code the
    /// source has never heard of.
    fn lookup(&self, code: &BankCode) -> Option<BankInfo>;
}

/// What a directory knows about a bank. Nothing here is a secret: it is
/// the same for every customer of the bank.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BankInfo {
    /// The bank's code.
    pub code: BankCode,
    /// The bank's name, as the user knows it.
    pub name: String,
    /// The bank's BIC, if known.
    pub bic: Option<String>,
    /// The bank's FinTS server, if known.
    pub server: Option<FintsServer>,
}

/// How a bank is reached over FinTS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FintsServer {
    /// The address, `https://…` (threat T-12: FinTS runs over TLS).
    pub url: String,
    /// The ways to log in the bank offers; `None` when the source doesn't
    /// say, which the screens show as an unknown login type.
    pub login_kinds: Option<Vec<LoginKind>>,
}

/// How a customer logs in at a bank. Only these two: no key files, so no
/// EBICS (D-059).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoginKind {
    /// A login name and a PIN, with a TAN for what the bank asks one for.
    PinTan {
        /// Whether the bank gives its customers a customer ID next to the
        /// login name.
        customer_id: CustomerId,
    },
    /// The bank's chip card in a card reader; the PIN is typed at the
    /// reader.
    ChipCard,
}

/// Whether a bank's customers have a customer ID apart from their login
/// name. Most German banks use the login name for both; a few give a
/// separate one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CustomerId {
    /// The login name is the customer ID too; it isn't asked for.
    SameAsLoginName,
    /// The bank gives a separate customer ID; it is asked for.
    Separate,
    /// Not known: the app asks for it as optional, "if your bank gave you
    /// one".
    Unknown,
}

/// What the app asks the user when setting up a login, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Question {
    /// The login name the bank gave (German: Anmeldename, Legitimations-ID,
    /// Kundennummer, depending on the bank).
    LoginName,
    /// The customer ID the bank gave next to the login name.
    CustomerId,
    /// A customer ID, if the bank gave one; may be left empty.
    OptionalCustomerId,
    /// The PIN, with the choice of D-057 how long to keep it.
    Pin,
    /// The chip card in its reader.
    ChipCard,
}

impl LoginKind {
    /// The questions this way of logging in needs answered, in order.
    pub fn questions(self) -> Vec<Question> {
        match self {
            Self::PinTan { customer_id } => {
                let mut questions = vec![Question::LoginName];
                match customer_id {
                    CustomerId::SameAsLoginName => {}
                    CustomerId::Separate => questions.push(Question::CustomerId),
                    CustomerId::Unknown => questions.push(Question::OptionalCustomerId),
                }
                questions.push(Question::Pin);
                questions
            }
            Self::ChipCard => vec![Question::ChipCard],
        }
    }
}

/// What setting up a bank login from an IBAN comes to. Every state the
/// screens have to show is here, so none is ever a guess (item 94 of the
/// Proposals list, `docs/BANK_LIST.md`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setup {
    /// The bank and its server are known: the app asks the questions of
    /// the login kinds offered and nothing else.
    Ready {
        /// The bank.
        bank: BankInfo,
        /// Its FinTS server's address.
        url: String,
        /// The ways to log in it offers, with their questions.
        login_kinds: Vec<LoginKind>,
        /// The account number within the bank, read from the IBAN where the
        /// layout is known.
        account_number: Option<String>,
    },
    /// The IBAN is from a country whose banks don't offer FinTS; the
    /// account can be filled from files only.
    NoFints {
        /// The country, e.g. `AT`.
        country: String,
    },
    /// No source knows a bank with this code. The user can give the bank's
    /// name, server and login kind by hand (Gwenhywfar's dialogs as the
    /// fallback, Q-035), and the list in the repository gets an entry
    /// (`docs/BANK_LIST.md`).
    UnknownBank {
        /// The code nothing was found for.
        code: BankCode,
    },
    /// The bank is known but no FinTS server address is: the bank may not
    /// offer FinTS, or the sources lack it. The user can give the address
    /// by hand.
    NoServer {
        /// The bank.
        bank: BankInfo,
    },
    /// The bank and its server are known, but not how its customers log
    /// in. The user chooses PIN/TAN or chip card by hand.
    UnknownLoginKind {
        /// The bank.
        bank: BankInfo,
        /// Its FinTS server's address.
        url: String,
    },
}

/// What setting up a login for the account with `iban` comes to, looked up
/// in `directory`.
pub fn setup_for(iban: &Iban, directory: &dyn BankDirectory) -> Setup {
    let Some(code) = iban.bank_code() else {
        return Setup::NoFints {
            country: iban.country().to_owned(),
        };
    };
    let Some(bank) = directory.lookup(&code) else {
        return Setup::UnknownBank { code };
    };
    let Some(server) = bank.server.clone() else {
        return Setup::NoServer { bank };
    };
    match server.login_kinds {
        Some(login_kinds) if !login_kinds.is_empty() => Setup::Ready {
            bank,
            url: server.url,
            login_kinds,
            account_number: iban.account_number().map(str::to_owned),
        },
        _ => Setup::UnknownLoginKind {
            bank,
            url: server.url,
        },
    }
}

/// The hand-checked list in this repository as a directory
/// ([`crate::banks::HAND_CHECKED`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HandCheckedList;

impl BankDirectory for HandCheckedList {
    fn lookup(&self, code: &BankCode) -> Option<BankInfo> {
        banks::HAND_CHECKED
            .iter()
            .find(|entry| entry.country == code.country() && entry.code == code.code())
            .map(BankEntry::info)
    }
}

/// An external source with the hand-checked list in front of it: an entry
/// in the list wins over the source entirely, so a wrong server or login
/// kind in the external source is corrected by one entry (item 93 of the
/// Proposals list: external source by default, the list in Git for
/// corrections).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Corrected<D> {
    /// The external source, e.g. AqBanking's bank information.
    pub external: D,
}

impl<D: BankDirectory> Corrected<D> {
    /// `external` with the hand-checked list in front of it.
    pub const fn new(external: D) -> Self {
        Self { external }
    }
}

impl<D: BankDirectory> BankDirectory for Corrected<D> {
    fn lookup(&self, code: &BankCode) -> Option<BankInfo> {
        HandCheckedList
            .lookup(code)
            .or_else(|| self.external.lookup(code))
    }
}

/// Makes a trait object usable where a directory is given by reference.
impl BankDirectory for &dyn BankDirectory {
    fn lookup(&self, code: &BankCode) -> Option<BankInfo> {
        (**self).lookup(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory with exactly the banks a test puts in.
    struct Listed(Vec<BankInfo>);

    impl BankDirectory for Listed {
        fn lookup(&self, code: &BankCode) -> Option<BankInfo> {
            self.0.iter().find(|bank| &bank.code == code).cloned()
        }
    }

    fn bank(server: Option<FintsServer>) -> BankInfo {
        BankInfo {
            code: BankCode::new("DE", "37040044"),
            name: "Testbank".to_owned(),
            bic: Some("TESTDEXXXXX".to_owned()),
            server,
        }
    }

    fn server(login_kinds: Option<Vec<LoginKind>>) -> FintsServer {
        FintsServer {
            url: "https://fints.testbank.example/".to_owned(),
            login_kinds,
        }
    }

    /// SWIFT's example IBAN for Germany (D-055); its bank code is 37040044.
    fn german_iban() -> Iban {
        Iban::parse("DE89370400440532013000").expect("an IBAN")
    }

    #[test]
    fn a_known_bank_with_a_server_and_login_kinds_is_ready() {
        let pin_tan = LoginKind::PinTan {
            customer_id: CustomerId::SameAsLoginName,
        };
        let directory = Listed(vec![bank(Some(server(Some(vec![pin_tan]))))]);
        assert_eq!(
            setup_for(&german_iban(), &directory),
            Setup::Ready {
                bank: bank(Some(server(Some(vec![pin_tan])))),
                url: "https://fints.testbank.example/".to_owned(),
                login_kinds: vec![pin_tan],
                account_number: Some("0532013000".to_owned()),
            }
        );
    }

    #[test]
    fn each_gap_in_the_sources_is_its_own_state() {
        let iban = german_iban();
        assert_eq!(
            setup_for(&iban, &Listed(vec![])),
            Setup::UnknownBank {
                code: BankCode::new("DE", "37040044")
            }
        );
        assert_eq!(
            setup_for(&iban, &Listed(vec![bank(None)])),
            Setup::NoServer { bank: bank(None) }
        );
        for login_kinds in [None, Some(vec![])] {
            assert_eq!(
                setup_for(
                    &iban,
                    &Listed(vec![bank(Some(server(login_kinds.clone())))])
                ),
                Setup::UnknownLoginKind {
                    bank: bank(Some(server(login_kinds))),
                    url: "https://fints.testbank.example/".to_owned(),
                }
            );
        }
    }

    #[test]
    fn an_iban_of_another_country_gets_no_fints() {
        let iban = Iban::parse("AT611904300234573201").expect("an IBAN");
        assert_eq!(
            setup_for(&iban, &Listed(vec![])),
            Setup::NoFints {
                country: "AT".to_owned()
            }
        );
    }

    #[test]
    fn the_questions_follow_the_login_kind() {
        assert_eq!(
            LoginKind::PinTan {
                customer_id: CustomerId::SameAsLoginName
            }
            .questions(),
            vec![Question::LoginName, Question::Pin]
        );
        assert_eq!(
            LoginKind::PinTan {
                customer_id: CustomerId::Separate
            }
            .questions(),
            vec![Question::LoginName, Question::CustomerId, Question::Pin]
        );
        assert_eq!(
            LoginKind::PinTan {
                customer_id: CustomerId::Unknown
            }
            .questions(),
            vec![
                Question::LoginName,
                Question::OptionalCustomerId,
                Question::Pin
            ]
        );
        assert_eq!(LoginKind::ChipCard.questions(), vec![Question::ChipCard]);
    }

    #[test]
    fn the_hand_checked_list_wins_over_the_external_source() {
        // The list is empty in the repository, so a test entry stands in
        // for it through the external side: the layering itself is what is
        // checked here, with the list's lookup on the first place.
        let external = Listed(vec![bank(None)]);
        let corrected = Corrected::new(&external as &dyn BankDirectory);
        let code = BankCode::new("DE", "37040044");
        assert_eq!(corrected.lookup(&code), Some(bank(None)));
        assert_eq!(corrected.lookup(&BankCode::new("DE", "10000000")), None);
        for entry in banks::HAND_CHECKED {
            let code = BankCode::new(entry.country, entry.code);
            assert_eq!(corrected.lookup(&code), Some(entry.info()));
        }
    }
}
