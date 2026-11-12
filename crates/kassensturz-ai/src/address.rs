// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The provider's address, and whether it is this computer (D-175).
//!
//! Only a loopback address is this computer: `localhost`, `127.0.0.0/8`
//! and `::1`. Every other address, a private home network address
//! included, is another computer, so what is sent to it has left the
//! machine and needs the user's consent (D-035). Plain `http://` is
//! accepted for this computer; any other address needs `https://`
//! (Q-125 A, threat T-30) unless the user ticked "Allow unencrypted
//! connections" (D-746): not every home server can be given a certificate,
//! so the user may choose it, warned in plain words, more strongly when the
//! address may be on the Internet ([`Network`]).

use std::fmt;
use std::net::IpAddr;

use ureq::http::Uri;

use crate::error::AiError;

/// The longest address KassenSturz accepts.
const MAX_LEN: usize = 2048;

/// Where the provider runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    /// This computer: a loopback address. Nothing sent to it leaves the
    /// machine.
    ThisComputer,
    /// Another computer, at home or in the cloud: what is sent to it leaves
    /// the machine (D-035).
    AnotherComputer,
}

/// Which network the provider is on, as far as its address tells without
/// looking anything up (D-746). It picks how strongly the settings warn
/// about an unencrypted connection; it never decides what is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Network {
    /// This computer: a loopback address.
    ThisComputer,
    /// A home or office network: a private, link-local or unique local IP
    /// address (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`,
    /// `169.254.0.0/16`, `fc00::/7`, `fe80::/10`), or a name that only a
    /// local network gives out (one word without dots, or one ending in
    /// `.local`, `.lan`, `.home.arpa` or `.internal`).
    Home,
    /// Possibly the Internet: a public IP address, or a name that could be
    /// anywhere.
    Internet,
}

/// The base address of an OpenAI-compatible provider, e.g.
/// `http://localhost:11434/v1` or `https://api.example.com/v1`, checked:
/// `http://` or `https://`, a server name, no login, query or `#` part,
/// and plain `http://` only on this computer.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ProviderAddress {
    /// The address without a trailing `/`.
    text: String,
    secure: bool,
    host: String,
    port: u16,
    place: Place,
}

impl ProviderAddress {
    /// Checks `text`, as the user typed it, with spaces around it ignored.
    /// Plain `http://` is accepted only for this computer.
    pub fn parse(text: &str) -> Result<Self, AiError> {
        Self::parse_with(text, false)
    }

    /// Like [`ProviderAddress::parse`], but with `allow_unencrypted` plain
    /// `http://` is accepted for another computer too: the user's choice
    /// "Allow unencrypted connections" (D-746).
    pub fn parse_with(text: &str, allow_unencrypted: bool) -> Result<Self, AiError> {
        let text = text.trim();
        if text.is_empty() || text.len() > MAX_LEN || text.contains(['@', '#', '?', '\\']) {
            return Err(AiError::BadAddress);
        }
        let uri: Uri = text.parse().map_err(|_| AiError::BadAddress)?;
        let secure = match uri.scheme_str() {
            Some(scheme) if scheme.eq_ignore_ascii_case("https") => true,
            Some(scheme) if scheme.eq_ignore_ascii_case("http") => false,
            _ => return Err(AiError::BadAddress),
        };
        let host = uri
            .host()
            .filter(|host| !host.is_empty())
            .ok_or(AiError::BadAddress)?
            .to_ascii_lowercase();
        let port = uri.port_u16().unwrap_or(if secure { 443 } else { 80 });
        let place = place_of(&host);
        if !secure && place != Place::ThisComputer && !allow_unencrypted {
            return Err(AiError::PlainHttp);
        }
        Ok(Self {
            text: text.trim_end_matches('/').to_owned(),
            secure,
            host,
            port,
            place,
        })
    }

    /// The address as checked, without a trailing `/`.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Where the provider runs.
    pub fn place(&self) -> Place {
        self.place
    }

    /// Whether the connection is encrypted (`https://`).
    pub fn is_secure(&self) -> bool {
        self.secure
    }

    /// Which network the provider is on, as far as the address tells.
    pub fn network(&self) -> Network {
        if self.place == Place::ThisComputer {
            Network::ThisComputer
        } else if is_home_host(&self.host) {
            Network::Home
        } else {
            Network::Internet
        }
    }

    /// Whether what is sent travels unencrypted to another computer: only
    /// possible when the user allowed it (D-746).
    pub fn is_unencrypted_to_another_computer(&self) -> bool {
        !self.secure && self.place == Place::AnotherComputer
    }

    /// The address of one of the provider's endpoints, e.g.
    /// `chat/completions`.
    pub fn endpoint(&self, path: &str) -> String {
        format!("{}/{path}", self.text)
    }

    /// Whether the address stops at the server, without a path such as
    /// `/v1`: the common slip with providers that serve their interface
    /// under `/v1` (D-233).
    pub fn has_no_path(&self) -> bool {
        self.text
            .parse::<Uri>()
            .is_ok_and(|uri| matches!(uri.path(), "" | "/"))
    }

    /// The same address with `/v1` added, if it has no path.
    pub fn with_v1(&self) -> Option<Self> {
        if self.has_no_path() {
            Self::parse(&format!("{}/v1", self.text)).ok()
        } else {
            None
        }
    }

    /// The server this address reaches, as one ASCII word: scheme, host and
    /// port, e.g. `https:api.example.com:443`. A saved key is filed under
    /// it, so a key goes only to the server it was saved for (D-184).
    pub fn origin(&self) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        let host = self.host.trim_start_matches('[').trim_end_matches(']');
        format!("{scheme}:{host}:{}", self.port)
    }
}

impl fmt::Debug for ProviderAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ProviderAddress").field(&self.text).finish()
    }
}

/// This computer only for a loopback name or address. A name is never
/// looked up: what a name resolves to could change after the check.
fn place_of(host: &str) -> Place {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let loopback = match bare.parse::<IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => bare == "localhost",
    };
    if loopback {
        Place::ThisComputer
    } else {
        Place::AnotherComputer
    }
}

/// Whether `host` is on a home or office network ([`Network::Home`]). A
/// name is never looked up, as in [`place_of`].
fn is_home_host(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => {
            let first = ip.segments()[0];
            // Unique local fc00::/7 and link-local fe80::/10.
            (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
        Err(_) => {
            let name = bare.trim_end_matches('.');
            !name.contains('.')
                || [".local", ".lan", ".home.arpa", ".internal"]
                    .iter()
                    .any(|end| name.ends_with(end))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(text: &str) -> Result<Place, AiError> {
        ProviderAddress::parse(text).map(|address| address.place())
    }

    #[test]
    fn only_loopback_is_this_computer() {
        for text in [
            "http://localhost:11434/v1",
            "http://LOCALHOST:1234/v1",
            "http://127.0.0.1:8080",
            "http://127.1.2.3/v1",
            "http://[::1]:8080/v1",
            "https://localhost/v1",
        ] {
            assert_eq!(place(text), Ok(Place::ThisComputer), "{text}");
        }
        for text in [
            "https://api.example.com/v1",
            "https://192.168.1.20:11434/v1",
            "https://10.0.0.5/v1",
            "https://[fe80::1]/v1",
            "https://[::ffff:127.0.0.1]/v1",
            "https://localhost.example.com/v1",
            "https://127.1/v1",
        ] {
            assert_eq!(place(text), Ok(Place::AnotherComputer), "{text}");
        }
    }

    #[test]
    fn plain_http_only_on_this_computer() {
        for text in [
            "http://192.168.1.20:11434/v1",
            "http://homeserver.local:11434/v1",
            "http://api.example.com/v1",
            "http://[::ffff:127.0.0.1]/v1",
            "http://0.0.0.0:11434/v1",
        ] {
            assert_eq!(place(text), Err(AiError::PlainHttp), "{text}");
        }
    }

    #[test]
    fn plain_http_to_another_computer_when_allowed() {
        for (text, network) in [
            ("http://192.168.1.20:1234/v1", Network::Home),
            ("http://10.0.0.5/v1", Network::Home),
            ("http://172.16.4.2/v1", Network::Home),
            ("http://169.254.3.4/v1", Network::Home),
            ("http://[fd12:3456::1]/v1", Network::Home),
            ("http://[fe80::1]/v1", Network::Home),
            ("http://homeserver:1234/v1", Network::Home),
            ("http://homeserver.local:1234/v1", Network::Home),
            ("http://nas.lan/v1", Network::Home),
            ("http://llm.home.arpa/v1", Network::Home),
            ("http://203.0.113.7:1234/v1", Network::Internet),
            ("http://172.32.0.1/v1", Network::Internet),
            ("http://[2001:db8::7]/v1", Network::Internet),
            ("http://api.example.com/v1", Network::Internet),
            ("http://0.0.0.0:11434/v1", Network::Internet),
        ] {
            let address = ProviderAddress::parse_with(text, true).unwrap();
            assert_eq!(address.place(), Place::AnotherComputer, "{text}");
            assert_eq!(address.network(), network, "{text}");
            assert!(!address.is_secure(), "{text}");
            assert!(address.is_unencrypted_to_another_computer(), "{text}");
            assert_eq!(
                ProviderAddress::parse_with(text, false),
                Err(AiError::PlainHttp),
                "{text}"
            );
        }
        let local = ProviderAddress::parse_with("http://localhost:1234/v1", true).unwrap();
        assert_eq!(local.network(), Network::ThisComputer);
        assert!(!local.is_unencrypted_to_another_computer());
        let secure = ProviderAddress::parse_with("https://203.0.113.7/v1", true).unwrap();
        assert_eq!(secure.network(), Network::Internet);
        assert!(!secure.is_unencrypted_to_another_computer());
        // Allowing plain http:// never lets an odd address through.
        assert_eq!(
            ProviderAddress::parse_with("ftp://192.168.1.20/v1", true),
            Err(AiError::BadAddress)
        );
    }

    #[test]
    fn odd_addresses_are_refused() {
        for text in [
            "",
            "   ",
            "localhost:11434",
            "ftp://example.com",
            "file:///etc/passwd",
            "https://user:secret@api.example.com/v1",
            "https://api.example.com/v1?key=1",
            "https://api.example.com/v1#x",
            "https:///v1",
            "https://api.example.com\\v1",
        ] {
            assert_eq!(place(text), Err(AiError::BadAddress), "{text:?}");
        }
        let long = format!("https://example.com/{}", "a".repeat(MAX_LEN));
        assert_eq!(place(&long), Err(AiError::BadAddress));
    }

    #[test]
    fn an_address_without_a_path_can_get_v1() {
        for text in [
            "http://localhost:1234",
            "http://localhost:1234/",
            "https://api.example.com",
        ] {
            let address = ProviderAddress::parse(text).unwrap();
            assert!(address.has_no_path(), "{text}");
            let with = address.with_v1().unwrap();
            assert_eq!(with.as_str(), format!("{}/v1", text.trim_end_matches('/')));
            assert!(!with.has_no_path());
            assert_eq!(with.origin(), address.origin());
        }
        for text in [
            "http://localhost:1234/v1",
            "https://example.com/v1beta/openai/",
        ] {
            let address = ProviderAddress::parse(text).unwrap();
            assert!(!address.has_no_path(), "{text}");
            assert!(address.with_v1().is_none(), "{text}");
        }
    }

    #[test]
    fn endpoints_and_origin() {
        let address = ProviderAddress::parse("  https://API.example.com/v1/ ").unwrap();
        assert_eq!(address.as_str(), "https://API.example.com/v1");
        assert_eq!(
            address.endpoint("chat/completions"),
            "https://API.example.com/v1/chat/completions"
        );
        assert_eq!(address.origin(), "https:api.example.com:443");
        assert!(address.is_secure());

        let local = ProviderAddress::parse("http://[::1]:1234").unwrap();
        assert_eq!(local.origin(), "http:::1:1234");
        assert_eq!(
            ProviderAddress::parse("http://localhost/v1")
                .unwrap()
                .origin(),
            "http:localhost:80"
        );
    }
}
