// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A server whose certificate the system doesn't trust gets nothing
//! (threat T-30, D-183): the adapter checks `https://` against the
//! system's certificate store and has no "connect anyway".
//!
//! The fake TLS server here presents a self-signed certificate made for
//! these tests. It holds no private key and only pretends to sign, which
//! is enough: the handshake has to fail either way. The test expects
//! `ai.untrusted-server` and that no request reached the server; that the
//! client checks against the system's store is the unit test in
//! `http.rs`. A unit test, so it uses the crate's own `rustls`, which a
//! build without AI leaves out (D-176).

use std::io::Read;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::{AiClient, AiError, ConfigFile, KeyUse, Message, ProviderSettings};
use kassensturz_platform::Secret;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use rustls::server::{ClientHello, ResolvesServerCert, ServerConfig, ServerConnection};
use rustls::sign::{CertifiedKey, Signer, SigningKey};
use rustls::{SignatureAlgorithm, SignatureScheme};

/// A self-signed certificate for `127.0.0.1`, made with
/// `openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256
/// -subj "/CN=KassenSturz test server" -addext
/// "subjectAltName=IP:127.0.0.1" -days 36500`; the key was thrown away.
const CERTIFICATE: &str = "-----BEGIN CERTIFICATE-----
MIIBrDCCAVKgAwIBAgIUCQ6KK3Q+nfSHTs4B5o3nDjtYd+swCgYIKoZIzj0EAwIw
IjEgMB4GA1UEAwwXS2Fzc2VuU3R1cnogdGVzdCBzZXJ2ZXIwIBcNMjYxMDI1MTM1
OTM4WhgPMjEyNjEwMDExMzU5MzhaMCIxIDAeBgNVBAMMF0thc3NlblN0dXJ6IHRl
c3Qgc2VydmVyMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEuNPJD3fSpsIn4jYS
duJcJh7SEExotvbGtowEkZVcLvxp8UfdkDtPHz1sDY0MfW9auCgu5igGQwYGVFeB
61jGxKNkMGIwHQYDVR0OBBYEFEprRkAJyFikj4X2jbKDJD2euYirMB8GA1UdIwQY
MBaAFEprRkAJyFikj4X2jbKDJD2euYirMA8GA1UdEwEB/wQFMAMBAf8wDwYDVR0R
BAgwBocEfwAAATAKBggqhkjOPQQDAgNIADBFAiAMe/Q0Okt4znHvzu/sm/Y2qT+i
VoTmZbXiz6iEeoNbXwIhAO0YiZmPKo6TTpyDu6EbAbg3xqKFD/bxMnDN+OHIx5aQ
-----END CERTIFICATE-----
";

/// Pretends to sign with the certificate's key.
#[derive(Debug)]
struct Pretend;

impl SigningKey for Pretend {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        offered
            .contains(&SignatureScheme::ECDSA_NISTP256_SHA256)
            .then(|| Box::new(Pretend) as Box<dyn Signer>)
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ECDSA
    }
}

impl Signer for Pretend {
    fn sign(&self, _message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        Ok(vec![0; 70])
    }

    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ECDSA_NISTP256_SHA256
    }
}

#[derive(Debug)]
struct Resolver(Arc<CertifiedKey>);

impl ResolvesServerCert for Resolver {
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

#[test]
fn an_untrusted_certificate_gets_nothing() {
    let certificate = CertificateDer::from_pem_slice(CERTIFICATE.as_bytes()).unwrap();
    let key = Arc::new(CertifiedKey::new(vec![certificate], Arc::new(Pretend)));
    let config = Arc::new(
        ServerConfig::builder_with_provider(
            Arc::new(rustls::crypto::aws_lc_rs::default_provider()),
        )
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(Resolver(key))),
    );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let plain = Arc::new(Mutex::new(None));
    let seen = plain.clone();
    thread::spawn(move || {
        let (mut tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut connection = ServerConnection::new(config).unwrap();
        let mut stream = rustls::Stream::new(&mut connection, &mut tcp);
        let mut buffer = Vec::new();
        let result = stream.read_to_end(&mut buffer);
        *seen.lock().unwrap() = Some((result.is_ok(), buffer));
    });

    let folder = std::env::temp_dir().join(format!("kassensturz-ai-tls-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("kassensturzrc");
    let settings = ProviderSettings::new(
        &format!("https://127.0.0.1:{port}/v1"),
        "test-model",
        KeyUse::AskEachTime,
    )
    .unwrap();
    let mut text = String::from("[AI]\nEnabled=true\n");
    for (key, value) in settings.entries() {
        text.push_str(&format!("{key}={value}\n"));
    }
    std::fs::write(&path, text).unwrap();

    let client = AiClient::new(ConfigFile::at(&path));
    let request = client
        .prepare(vec![Message::user("Space Colony question")], Vec::new())
        .unwrap();
    let key = Secret::new("sk-test-0000-made-up-key".to_owned());
    assert_eq!(
        client.send(&request, Some(&key)),
        Err(AiError::UntrustedServer)
    );

    // The server's side ends when the client gives up.
    for _ in 0..100 {
        if plain.lock().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let (finished, received) = plain.lock().unwrap().take().expect("the server finished");
    assert!(!finished, "the handshake must fail on the server too");
    assert!(received.is_empty(), "the server received a request");
    let _ = std::fs::remove_dir_all(folder);
}
