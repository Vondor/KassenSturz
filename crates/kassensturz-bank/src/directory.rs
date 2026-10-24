// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which bank an IBAN belongs to and what setting up a login there needs,
//! so the app asks for the IBAN first and then only for what that bank needs
//! (the maintainer's answer to Q-035, D-145).
//!
//! Banks come from two sources, layered: the hand-checked list in this
//! repository ([`crate::banks`]) first, for corrections, then an external
//! source, AqBanking's bank information (M4.5). What neither knows is an
//! explicit state the screens show in plain language: unknown bank, no
//! server, unknown login type (`docs/BANK_LIST.md`). The one guess there is
//! says so: a cooperative bank without a server is offered the Atruvia
//! server its neighbours use, marked as a guess (D-156, [`guess_server`]).

use crate::banks::{self, BankEntry};
use crate::iban::{BankCode, Iban};

/// Where banks are looked up by their code: AqBanking's bank information
/// (M4.5), the hand-checked list, or the fake bank's own in tests.
pub trait BankDirectory {
    /// What is known about the bank with `code`; `None` for a code the
    /// source has never heard of.
    fn lookup(&self, code: &BankCode) -> Option<BankInfo>;

    /// The banks of `country` whose code starts with `prefix`, one entry
    /// per code, as [`BankDirectory::lookup`] would give each. A source
    /// that can't list banks by their code answers with none, which only
    /// takes away the guess of a server ([`guess_server`]).
    fn banks_starting_with(&self, _country: &str, _prefix: &str) -> Vec<BankInfo> {
        Vec::new()
    }
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
    /// The bank is known and the sources have no FinTS server address for
    /// it, but the banks next to it in the bank code list all use one
    /// Atruvia server: the app offers that address, marked as a guess the
    /// user can change (D-156). The customer ID is asked as optional, as
    /// the sources don't say whether the bank has one.
    GuessedServer {
        /// The bank.
        bank: BankInfo,
        /// The guessed server's address.
        url: String,
        /// The account number within the bank, read from the IBAN where the
        /// layout is known.
        account_number: Option<String>,
    },
    /// The bank is known but no FinTS server address is, and none can be
    /// guessed: the bank may not offer FinTS, or the sources lack it. The
    /// user can give the address by hand.
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
        return match guess_server(&bank, directory) {
            Some(url) => Setup::GuessedServer {
                bank,
                url,
                account_number: iban.account_number().map(str::to_owned),
            },
            None => Setup::NoServer { bank },
        };
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

/// The longest bank code prefixes [`guess_server`] looks at, longest first.
/// The first three digits of a German bank code name the region the bank
/// was founded in; with fewer the neighbourhood is too wide to say which of
/// Atruvia's two servers a bank is on.
const GUESS_PREFIXES: [usize; 3] = [5, 4, 3];

/// The Atruvia server a cooperative bank without a server of its own most
/// likely uses (D-156), or `None` when nothing safe can be said.
///
/// Atruvia, the computing centre of the Volksbanken and Raiffeisenbanken,
/// has two FinTS servers, one for the banks that came from each of its
/// former centres, and which one a bank is on follows its region: the
/// neighbours in the bank code list are on the same one (95% of the 434
/// banks with a known Atruvia server, checked against AqBanking's data).
/// So the guess is the server most of the bank's neighbours use, looking at
/// the nearest codes first. It is made only when
///
/// - the bank's BIC says it is a cooperative bank (`GENODE…`) in Germany,
///   and its name doesn't say Sparda: Sparda banks are not all on Atruvia,
/// - at least one neighbour is on an Atruvia server (an address on
///   `atruvia.de`), and no other server is ever guessed, and
/// - the nearest neighbourhood that has one is not split evenly.
///
/// The guess is never stored, and the app shows it as a guess.
pub fn guess_server(bank: &BankInfo, directory: &dyn BankDirectory) -> Option<String> {
    if bank.server.is_some() || bank.code.country() != "DE" {
        return None;
    }
    let cooperative = bank
        .bic
        .as_deref()
        .and_then(|bic| bic.get(..6))
        .is_some_and(|start| start.eq_ignore_ascii_case("GENODE"));
    if !cooperative || bank.name.to_lowercase().contains("sparda") {
        return None;
    }
    for length in GUESS_PREFIXES {
        let prefix = bank.code.code().get(..length)?;
        let mut counts: Vec<(String, usize)> = Vec::new();
        for neighbour in directory.banks_starting_with(bank.code.country(), prefix) {
            if neighbour.code == bank.code {
                continue;
            }
            let Some(server) = neighbour.server else {
                continue;
            };
            let url = server.url.trim();
            if !is_atruvia(url) {
                continue;
            }
            match counts.iter_mut().find(|(known, _)| known == url) {
                Some((_, count)) => *count += 1,
                None => counts.push((url.to_owned(), 1)),
            }
        }
        counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        match counts.as_slice() {
            [] => {}
            [(url, _)] => return Some(url.clone()),
            [(url, most), (_, next), ..] if most > next => return Some(url.clone()),
            // Split evenly: a wider neighbourhood is no better.
            _ => return None,
        }
    }
    None
}

/// Whether `url` is an `https://` address on Atruvia's domain.
fn is_atruvia(url: &str) -> bool {
    let Some(rest) = url
        .get(..8)
        .filter(|scheme| scheme.eq_ignore_ascii_case("https://"))
        .map(|_| &url[8..])
    else {
        return false;
    };
    let host = rest
        .split(['/', ':', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    host == "atruvia.de" || host.ends_with(".atruvia.de")
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

    fn banks_starting_with(&self, country: &str, prefix: &str) -> Vec<BankInfo> {
        banks::HAND_CHECKED
            .iter()
            .filter(|entry| entry.country == country && entry.code.starts_with(prefix))
            .map(BankEntry::info)
            .collect()
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

    /// The external source's banks, each replaced by the hand-checked
    /// entry of its code if there is one, and the entries it doesn't have.
    fn banks_starting_with(&self, country: &str, prefix: &str) -> Vec<BankInfo> {
        let checked = HandCheckedList.banks_starting_with(country, prefix);
        let mut banks: Vec<BankInfo> = self
            .external
            .banks_starting_with(country, prefix)
            .into_iter()
            .filter(|bank| !checked.iter().any(|entry| entry.code == bank.code))
            .collect();
        banks.extend(checked);
        banks
    }
}

/// Makes a trait object usable where a directory is given by reference.
impl BankDirectory for &dyn BankDirectory {
    fn lookup(&self, code: &BankCode) -> Option<BankInfo> {
        (**self).lookup(code)
    }

    fn banks_starting_with(&self, country: &str, prefix: &str) -> Vec<BankInfo> {
        (**self).banks_starting_with(country, prefix)
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

        fn banks_starting_with(&self, country: &str, prefix: &str) -> Vec<BankInfo> {
            self.0
                .iter()
                .filter(|bank| {
                    bank.code.country() == country && bank.code.code().starts_with(prefix)
                })
                .cloned()
                .collect()
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

    const NORTH: &str = "https://fints1.atruvia.de/cgi-bin/hbciservlet";
    const SOUTH: &str = "https://fints2.atruvia.de/cgi-bin/hbciservlet";

    fn listed(code: &str, name: &str, bic: Option<&str>, url: Option<&str>) -> BankInfo {
        BankInfo {
            code: BankCode::new("DE", code),
            name: name.to_owned(),
            bic: bic.map(str::to_owned),
            server: url.map(|url| FintsServer {
                url: url.to_owned(),
                login_kinds: None,
            }),
        }
    }

    fn cooperative(code: &str, url: Option<&str>) -> BankInfo {
        listed(code, "Genossenbank", Some("GENODEF1XXX"), url)
    }

    #[test]
    fn a_cooperative_bank_without_a_server_gets_the_server_of_its_neighbours() {
        let target = cooperative("55500003", None);
        let directory = Listed(vec![
            cooperative("55500001", Some(SOUTH)),
            cooperative("55500002", Some(SOUTH)),
            target.clone(),
            // Farther away and of the other server: the nearest wins.
            cooperative("55400001", Some(NORTH)),
            cooperative("55400002", Some(NORTH)),
            cooperative("55400003", Some(NORTH)),
        ]);
        assert_eq!(guess_server(&target, &directory), Some(SOUTH.to_owned()));
        // The same through the setup, as a guess and not a known server.
        let iban = Iban::parse("DE36555000030000000001").expect("an IBAN");
        let setup = setup_for(&iban, &directory);
        assert!(
            matches!(&setup, Setup::GuessedServer { url, .. } if url == SOUTH),
            "{setup:?}"
        );
    }

    #[test]
    fn the_nearest_neighbourhood_with_a_server_decides() {
        // Nobody within the first five digits; four digits find a majority.
        let target = cooperative("55500003", None);
        let directory = Listed(vec![
            cooperative("55510001", Some(NORTH)),
            cooperative("55520001", Some(NORTH)),
            cooperative("55530001", Some(SOUTH)),
        ]);
        assert_eq!(guess_server(&target, &directory), Some(NORTH.to_owned()));
        // Three digits are the widest look: nothing within them, no guess.
        let far = Listed(vec![cooperative("55610001", Some(SOUTH))]);
        assert_eq!(guess_server(&target, &far), None);
    }

    #[test]
    fn nothing_is_guessed_where_it_would_be_a_coin_toss_or_not_atruvia() {
        let target = cooperative("55500003", None);
        // Split evenly, and a wider look is no better.
        let split = Listed(vec![
            cooperative("55500001", Some(NORTH)),
            cooperative("55500002", Some(SOUTH)),
            cooperative("55400001", Some(SOUTH)),
            cooperative("55400002", Some(SOUTH)),
        ]);
        assert_eq!(guess_server(&target, &split), None);
        // Neighbours on another computing centre, or not on https.
        let other = Listed(vec![
            cooperative("55500001", Some("https://fints.other.example/")),
            cooperative("55500002", Some("https://atruvia.de.example/fints")),
            cooperative(
                "55500004",
                Some("http://fints2.atruvia.de/cgi-bin/hbciservlet"),
            ),
        ]);
        assert_eq!(guess_server(&target, &other), None);
        // No neighbour at all.
        assert_eq!(guess_server(&target, &Listed(vec![target.clone()])), None);
    }

    #[test]
    fn only_cooperative_banks_in_germany_without_a_server_are_guessed_for() {
        let neighbours = |target: BankInfo| {
            Listed(vec![
                cooperative("55500001", Some(SOUTH)),
                cooperative("55500002", Some(SOUTH)),
                target,
            ])
        };
        for (target, why) in [
            (
                listed("55500003", "Sparda-Bank Test", Some("GENODEF1S99"), None),
                "a Sparda bank",
            ),
            (
                listed("55500003", "Bank", Some("TESTDEFFXXX"), None),
                "another kind of bank",
            ),
            (listed("55500003", "Bank", None, None), "no BIC"),
            (
                listed("55500003", "Bank", Some("GEN"), None),
                "a BIC too short to say",
            ),
            (
                cooperative("55500003", Some("https://fints.own.example/")),
                "a server of its own",
            ),
        ] {
            assert_eq!(
                guess_server(&target, &neighbours(target.clone())),
                None,
                "{why}"
            );
        }
        let mut austrian = cooperative("55500003", None);
        austrian.code = BankCode::new("AT", "55500003");
        assert_eq!(guess_server(&austrian, &neighbours(austrian.clone())), None);
        // A trailing space in the data doesn't make a second server.
        let target = cooperative("55500003", None);
        let spaced = Listed(vec![
            cooperative(
                "55500001",
                Some("https://fints2.atruvia.de/cgi-bin/hbciservlet "),
            ),
            cooperative("55500002", Some(SOUTH)),
        ]);
        assert_eq!(guess_server(&target, &spaced), Some(SOUTH.to_owned()));
    }

    #[test]
    fn a_hand_checked_entry_stands_in_the_neighbours_too() {
        // The hand-checked list's banks replace the external source's for
        // the same code and are listed when the source lacks them.
        let external = Listed(vec![cooperative("55190000", None)]);
        let corrected = Corrected::new(&external as &dyn BankDirectory);
        let banks = corrected.banks_starting_with("DE", "5519");
        let entry = banks::HAND_CHECKED
            .iter()
            .find(|entry| entry.code == "55190000")
            .expect("an entry");
        assert_eq!(banks, vec![entry.info()]);
        assert!(corrected.banks_starting_with("DE", "9999").is_empty());
    }
}
