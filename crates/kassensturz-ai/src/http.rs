// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The HTTP client of the adapter: `ureq` with `rustls` (Q-122 A, D-183),
//! the only way out of KassenSturz besides the bank backend (threat T-13).
//!
//! Every connection is made with these rules (threats T-30, T-31):
//!
//! - `https://` is checked against the system's certificate store
//!   (`rustls-platform-verifier`); there is no "connect anyway".
//! - No proxy, not even one named in the environment: a proxy would see
//!   everything sent, and it isn't the provider the user agreed to.
//! - No redirect is followed; a redirect answer is `ai.redirect`.
//! - Size and time limits ([`Limits`]); the provider's error text is never
//!   read, only its status.
//!
//! The client is built for each call, after the AI switch was checked
//! (threat T-28).

use std::sync::Arc;
use std::time::Duration;

use kassensturz_platform::Secret;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use zeroize::Zeroizing;

use crate::error::AiError;

/// The size and time limits of one call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// How long to wait for the connection, the TLS handshake included.
    pub connect: Duration,
    /// How long the whole call may take. A local model on a slow computer
    /// can take minutes for one answer.
    pub total: Duration,
    /// The largest answer read.
    pub reply_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(15),
            total: Duration::from_secs(300),
            reply_bytes: 8 * 1024 * 1024,
        }
    }
}

fn agent(limits: &Limits) -> Agent {
    let tls = TlsConfig::builder()
        .provider(TlsProvider::Rustls)
        .root_certs(RootCerts::PlatformVerifier)
        .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .build();
    Agent::config_builder()
        .tls_config(tls)
        .proxy(None)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_connect(Some(limits.connect))
        .timeout_global(Some(limits.total))
        .user_agent("KassenSturz")
        .build()
        .new_agent()
}

/// Sends `body` as JSON to `url` with `key` as the bearer token, and
/// answers the reply's body.
pub(crate) fn post_json(
    url: &str,
    body: &str,
    key: Option<&Secret>,
    limits: &Limits,
) -> Result<Vec<u8>, AiError> {
    let mut request = agent(limits)
        .post(url)
        .header("Content-Type", "application/json");
    let bearer = key.map(bearer);
    if let Some(bearer) = &bearer {
        request = request.header("Authorization", bearer.as_str());
    }
    read(request.send(body.as_bytes()), limits)
}

/// Asks `url` with `key` as the bearer token, and answers the reply's
/// body.
pub(crate) fn get(url: &str, key: Option<&Secret>, limits: &Limits) -> Result<Vec<u8>, AiError> {
    let mut request = agent(limits).get(url);
    let bearer = key.map(bearer);
    if let Some(bearer) = &bearer {
        request = request.header("Authorization", bearer.as_str());
    }
    read(request.call(), limits)
}

/// The header value, wiped when dropped. `ureq` keeps its own copy for the
/// call, which KassenSturz can't wipe (D-184).
fn bearer(key: &Secret) -> Zeroizing<String> {
    Zeroizing::new(format!("Bearer {}", key.expose()))
}

fn read(
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    limits: &Limits,
) -> Result<Vec<u8>, AiError> {
    let mut response = response.map_err(error)?;
    let status = response.status().as_u16();
    match status {
        200..=299 => {}
        300..=399 => return Err(AiError::Redirect),
        401 | 403 => return Err(AiError::KeyRejected),
        404 => return Err(AiError::NotFound),
        429 => return Err(AiError::RateLimited),
        _ => return Err(AiError::ProviderFailed { status }),
    }
    response
        .body_mut()
        .with_config()
        .limit(limits.reply_bytes)
        .read_to_vec()
        .map_err(error)
}

/// `ureq`'s error as a code, without its text, which may name the address.
fn error(error: ureq::Error) -> AiError {
    match error {
        ureq::Error::Timeout(_) => AiError::Timeout,
        ureq::Error::BodyExceedsLimit(_) => AiError::ReplyTooLarge,
        ureq::Error::TooManyRedirects | ureq::Error::RedirectFailed => AiError::Redirect,
        ureq::Error::Tls(_) | ureq::Error::Pem(_) | ureq::Error::Rustls(_) => {
            AiError::UntrustedServer
        }
        ureq::Error::Io(io) if is_tls(&io) => AiError::UntrustedServer,
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => AiError::Timeout,
        ureq::Error::BadUri(_) => AiError::BadAddress,
        _ => AiError::Unreachable,
    }
}

/// Whether an I/O error is a TLS error that `rustls` reported through the
/// stream.
fn is_tls(io: &std::io::Error) -> bool {
    io.get_ref()
        .is_some_and(|inner| inner.downcast_ref::<rustls::Error>().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_keeps_the_rules() {
        let limits = Limits::default();
        let agent = agent(&limits);
        let config = agent.config();
        let tls = config.tls_config();
        assert_eq!(tls.provider(), TlsProvider::Rustls);
        assert!(matches!(tls.root_certs(), RootCerts::PlatformVerifier));
        assert!(!tls.disable_verification(), "no \"connect anyway\"");
        assert!(tls.unversioned_rustls_crypto_provider().is_some());
        assert!(
            config.proxy().is_none(),
            "no proxy, not even from the environment"
        );
        assert_eq!(config.max_redirects(), 0);
        assert!(!config.http_status_as_error());
    }
}
