// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AI settings of this computer, read from the app's settings file
//! (D-168, threat T-24).
//!
//! Whether AI is on, the provider (address, model, how its key is kept)
//! and the consent record live in the `[AI]` group of `kassensturzrc`, the
//! file KConfig keeps the settings that belong to this computer in (D-112).
//! They are never in a book, so a copied, synced or imported book can
//! neither turn AI on nor carry a consent. The app writes them with KConfig
//! under the key names below; this module only reads them, at every entry
//! point of [`crate::AiClient`], so a change takes effect at the next call.
//! The provider's key is never in this file (D-169).

use std::path::PathBuf;

use jiff::civil::Date;
use kassensturz_core::ai_switch;

use crate::address::ProviderAddress;
use crate::consent::ConsentRecord;
use crate::error::AiError;

/// The key of the provider's address, e.g. `http://localhost:11434/v1`.
pub const ADDRESS_KEY: &str = "ProviderAddress";
/// The key of the model's name, as the provider calls it.
pub const MODEL_KEY: &str = "ProviderModel";
/// The key of how the provider's key is kept ([`KeyUse::as_word`]).
pub const KEY_USE_KEY: &str = "ProviderKey";
/// The key of the address the user agreed to send to.
pub const CONSENT_ADDRESS_KEY: &str = "ConsentAddress";
/// The key of the version of the statement of what is sent that the user
/// agreed to ([`crate::STATEMENT_VERSION`]).
pub const CONSENT_STATEMENT_KEY: &str = "ConsentStatement";
/// The key of the day the user agreed, as `YYYY-MM-DD`.
pub const CONSENT_DAY_KEY: &str = "ConsentDay";

/// The longest model name KassenSturz accepts.
const MAX_MODEL_LEN: usize = 256;

/// How the provider's key is kept (D-169), as for a banking PIN (D-057).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum KeyUse {
    /// The provider needs no key, as a local server usually doesn't
    /// (Ollama, LM Studio, llama.cpp). The default.
    #[default]
    None,
    /// The user types the key for each request; it is never kept.
    AskEachTime,
    /// Kept in memory until KassenSturz closes, never written anywhere.
    UntilClose,
    /// Saved where the Password storage setting says (D-163), filed under
    /// the provider's server (D-184).
    Saved,
}

impl KeyUse {
    /// The word it is kept under in the settings file.
    pub fn as_word(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::AskEachTime => "ask",
            Self::UntilClose => "remember",
            Self::Saved => "save",
        }
    }

    /// The choice kept under `word`; a word this build doesn't know is
    /// [`KeyUse::AskEachTime`], which never keeps a key and never sends
    /// one without the user typing it.
    pub fn from_word(word: &str) -> Self {
        match word {
            "" | "none" => Self::None,
            "remember" => Self::UntilClose,
            "save" => Self::Saved,
            _ => Self::AskEachTime,
        }
    }
}

/// The provider the user set up: where, which model, and how its key is
/// kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderSettings {
    /// The provider's checked address.
    pub address: ProviderAddress,
    /// The model's name, as the provider calls it.
    pub model: String,
    /// How the provider's key is kept.
    pub key_use: KeyUse,
}

impl ProviderSettings {
    /// Checks the settings as the user typed them: a usable address and a
    /// model name of printable characters.
    pub fn new(address: &str, model: &str, key_use: KeyUse) -> Result<Self, AiError> {
        if model.trim().is_empty() {
            return Err(AiError::NoProvider);
        }
        Self::with_optional_model(address, model, key_use)
    }

    /// Like [`ProviderSettings::new`], but the model may still be empty:
    /// a user who doesn't know the model's name saves the address first and
    /// picks the model from what the provider offers (D-234). Such settings
    /// can list models but are not yet a provider to ask.
    pub fn with_optional_model(
        address: &str,
        model: &str,
        key_use: KeyUse,
    ) -> Result<Self, AiError> {
        let model = model.trim();
        if address.trim().is_empty() {
            return Err(AiError::NoProvider);
        }
        if model.len() > MAX_MODEL_LEN || model.chars().any(char::is_control) {
            return Err(AiError::NoProvider);
        }
        Ok(Self {
            address: ProviderAddress::parse(address)?,
            model: model.to_owned(),
            key_use,
        })
    }

    /// The entries the app writes to the `[AI]` group for these settings.
    pub fn entries(&self) -> [(&'static str, String); 3] {
        [
            (ADDRESS_KEY, self.address.as_str().to_owned()),
            (MODEL_KEY, self.model.clone()),
            (KEY_USE_KEY, self.key_use.as_word().to_owned()),
        ]
    }
}

/// The AI settings as one reading of the settings file found them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiConfig {
    enabled: bool,
    /// The provider as saved, the model possibly still empty.
    saved: Result<ProviderSettings, AiError>,
    consent: Option<ConsentRecord>,
}

impl AiConfig {
    /// The settings in the text of a settings file. Anything missing or
    /// odd reads as off, no provider or no consent.
    pub fn from_text(text: &str) -> Self {
        let value = |key| {
            ai_switch::value_in(text, ai_switch::GROUP, key)
                .and_then(unescape)
                .unwrap_or_default()
        };
        let saved = ProviderSettings::with_optional_model(
            &value(ADDRESS_KEY),
            &value(MODEL_KEY),
            KeyUse::from_word(&value(KEY_USE_KEY)),
        );
        let consent = ProviderAddress::parse(&value(CONSENT_ADDRESS_KEY))
            .ok()
            .zip(value(CONSENT_STATEMENT_KEY).parse::<u32>().ok())
            .zip(value(CONSENT_DAY_KEY).parse::<Date>().ok())
            .map(|((address, statement), day)| ConsentRecord {
                address,
                statement,
                day,
            });
        Self {
            enabled: ai_switch::enabled_in(text),
            saved,
            consent,
        }
    }

    /// Whether AI is switched on (D-177).
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The provider, or why there is none to use.
    pub fn provider(&self) -> Result<&ProviderSettings, AiError> {
        match &self.saved {
            Ok(settings) if settings.model.is_empty() => Err(AiError::NoProvider),
            Ok(settings) => Ok(settings),
            Err(error) => Err(error.clone()),
        }
    }

    /// The server and the key's use as saved, even before a model is
    /// chosen: enough to ask the provider which models it offers.
    pub fn connection(&self) -> Result<&ProviderSettings, AiError> {
        self.saved.as_ref().map_err(Clone::clone)
    }

    /// The consent the user gave, if a complete one is recorded.
    pub fn consent(&self) -> Option<&ConsentRecord> {
        self.consent.as_ref()
    }
}

/// Where the AI settings are read from: the user's settings file, or a
/// file a test names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigFile {
    path: Option<PathBuf>,
}

impl ConfigFile {
    /// The settings file of this user on this computer, where KConfig keeps
    /// it (D-177); on systems where that isn't known yet AI reads as off.
    pub fn user() -> Self {
        Self {
            path: ai_switch::settings_file(),
        }
    }

    /// The settings file at `path`.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
        }
    }

    /// Reads the file now. A missing or unreadable file reads as off.
    pub fn read(&self) -> AiConfig {
        let text = self
            .path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        AiConfig::from_text(&text)
    }
}

/// A value as KConfig wrote it, with its escapes read: `\s`, `\t`, `\n`,
/// `\r` and `\\`. Any other escape makes the value unusable.
fn unescape(value: &str) -> Option<String> {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next()? {
            's' => ' ',
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            '\\' => '\\',
            _ => return None,
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Place;

    const FULL: &str = "[General]\nProviderAddress=https://elsewhere.example/v1\n\n\
        [AI]\nEnabled=true\nProviderAddress=https://api.example.com/v1\n\
        ProviderModel=\\sbig model\nProviderKey=save\n\
        ConsentAddress=https://api.example.com/v1\nConsentStatement=1\n\
        ConsentDay=2026-10-25\n";

    #[test]
    fn everything_is_read_from_the_ai_group() {
        let config = AiConfig::from_text(FULL);
        assert!(config.enabled());
        let provider = config.provider().unwrap();
        assert_eq!(provider.address.as_str(), "https://api.example.com/v1");
        assert_eq!(provider.address.place(), Place::AnotherComputer);
        assert_eq!(provider.model, "big model");
        assert_eq!(provider.key_use, KeyUse::Saved);
        let consent = config.consent().unwrap();
        assert_eq!(consent.address.as_str(), "https://api.example.com/v1");
        assert_eq!(consent.statement, 1);
        assert_eq!(consent.day, jiff::civil::date(2026, 10, 25));
    }

    #[test]
    fn missing_or_odd_settings_read_as_nothing() {
        let empty = AiConfig::from_text("");
        assert!(!empty.enabled());
        assert_eq!(empty.provider(), Err(AiError::NoProvider));
        assert_eq!(empty.consent(), None);

        let plain = AiConfig::from_text(
            "[AI]\nEnabled=true\nProviderAddress=http://192.168.1.2:11434/v1\nProviderModel=m\n",
        );
        assert_eq!(plain.provider(), Err(AiError::PlainHttp));

        let no_model = AiConfig::from_text(
            "[AI]\nProviderAddress=http://localhost:11434/v1\nProviderModel=\n",
        );
        assert_eq!(no_model.provider(), Err(AiError::NoProvider));

        let bad_escape = AiConfig::from_text(
            "[AI]\nProviderAddress=http://localhost:11434/v1\nProviderModel=a\\qb\n",
        );
        assert_eq!(bad_escape.provider(), Err(AiError::NoProvider));

        let half_consent = AiConfig::from_text(
            "[AI]\nConsentAddress=https://api.example.com/v1\nConsentStatement=1\n",
        );
        assert_eq!(half_consent.consent(), None);
    }

    #[test]
    fn key_use_words_round_trip_and_unknown_asks() {
        for key_use in [
            KeyUse::None,
            KeyUse::AskEachTime,
            KeyUse::UntilClose,
            KeyUse::Saved,
        ] {
            assert_eq!(KeyUse::from_word(key_use.as_word()), key_use);
        }
        assert_eq!(KeyUse::from_word(""), KeyUse::None);
        assert_eq!(KeyUse::from_word("keychain"), KeyUse::AskEachTime);
    }

    #[test]
    fn entries_read_back() {
        let settings =
            ProviderSettings::new(" http://localhost:1234/v1/ ", " qwen3 ", KeyUse::UntilClose)
                .unwrap();
        let mut text = String::from("[AI]\n");
        for (key, value) in settings.entries() {
            text.push_str(&format!("{key}={value}\n"));
        }
        assert_eq!(AiConfig::from_text(&text).provider(), Ok(&settings));
        assert_eq!(settings.address.as_str(), "http://localhost:1234/v1");
        assert_eq!(settings.model, "qwen3");
    }

    #[test]
    fn settings_never_come_from_a_book() {
        // Threat T-24: this crate can't open a book, so a book can neither
        // switch AI on nor carry a consent.
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("kassensturz-store"));
        assert!(!manifest.contains("rusqlite"));
    }

    #[test]
    fn a_missing_file_is_off() {
        let config = ConfigFile::at("/nonexistent/kassensturzrc").read();
        assert!(!config.enabled());
    }
}
