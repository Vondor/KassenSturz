// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The provider's address, and whether it is this computer (D-175).
//!
//! Only a loopback address is this computer: `localhost`, `127.0.0.0/8`
//! and `::1`. Every other address, a private home network address
//! included, is another computer, so what is sent to it has left the
//! machine and needs the user's consent (D-035). Plain `http://` is
//! accepted only for this computer; any other address needs `https://`
//! (Q-125 A, threat T-30).

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
    pub fn parse(text: &str) -> Result<Self, AiError> {
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
        if !secure && place != Place::ThisComputer {
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

    /// The address of one of the provider's endpoints, e.g.
    /// `chat/completions`.
    pub fn endpoint(&self, path: &str) -> String {
        format!("{}/{path}", self.text)
    }

    /// Whether the address stops at the server, without a path such as
    /// `/v1`: the common slip with providers that serve their interface
    /// under `/v1` (D-231).
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
