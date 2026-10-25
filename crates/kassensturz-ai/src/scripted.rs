// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A provider that KassenSturz plays itself, inside the app, for tests and
//! for trying the chat without any AI (D-193).
//!
//! [`ScriptedProvider`] answers from a function instead of a server: it
//! opens no connection and no socket, and nothing it gets leaves the
//! process. Its answers are the JSON text an OpenAI-compatible provider
//! sends and are read by the same code as a real provider's
//! ([`crate::openai`]), so a call for a function that wasn't offered is
//! left out the same way (threat T-31). It still checks the AI switch at
//! every call (threat T-28): while AI is off it answers `ai.off` and its
//! function is never run.

use std::fmt;

use kassensturz_platform::Secret;

use crate::Provider;
use crate::address::ProviderAddress;
use crate::chat::{FunctionSpec, Message, Reply};
use crate::config::ConfigFile;
use crate::error::AiError;
use crate::http::Limits;
use crate::openai;
use crate::request::PreparedRequest;

/// The address a scripted request names in its preview. It is on this
/// computer and is never connected to.
pub const SCRIPTED_ADDRESS: &str = "http://localhost/kassensturz-scripted-model";

/// The model a scripted request names.
pub const SCRIPTED_MODEL: &str = "kassensturz-scripted-model";

/// The function that plays the model: it gets the messages and the names of
/// the functions offered, and answers with the body of a `chat/completions`
/// reply, or with the error a provider could give.
pub type Respond = dyn Fn(&[Message], &[String]) -> Result<String, AiError> + Send + Sync;

/// A provider played by a function on this computer.
pub struct ScriptedProvider {
    config: ConfigFile,
    respond: Box<Respond>,
}

impl fmt::Debug for ScriptedProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScriptedProvider")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl ScriptedProvider {
    /// A provider that answers with `respond` while the settings in
    /// `config` switch AI on.
    pub fn new(
        config: ConfigFile,
        respond: impl Fn(&[Message], &[String]) -> Result<String, AiError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            config,
            respond: Box::new(respond),
        }
    }

    fn check_on(&self) -> Result<(), AiError> {
        if self.config.read().enabled() {
            Ok(())
        } else {
            Err(AiError::Off)
        }
    }
}

impl Provider for ScriptedProvider {
    fn prepare(
        &self,
        messages: Vec<Message>,
        functions: Vec<FunctionSpec>,
    ) -> Result<PreparedRequest, AiError> {
        self.check_on()?;
        PreparedRequest::new(
            ProviderAddress::parse(SCRIPTED_ADDRESS)?,
            SCRIPTED_MODEL.to_owned(),
            messages,
            functions,
        )
    }

    fn send(&self, request: &PreparedRequest, _key: Option<&Secret>) -> Result<Reply, AiError> {
        self.check_on()?;
        let offered = request.function_names();
        let body = (self.respond)(request.messages(), &offered)?;
        if u64::try_from(body.len()).unwrap_or(u64::MAX) > Limits::default().reply_bytes {
            return Err(AiError::ReplyTooLarge);
        }
        openai::parse_chat_reply(body.as_bytes(), &offered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(enabled: bool) -> ConfigFile {
        let folder = std::env::temp_dir().join(format!(
            "kassensturz-ai-scripted-{}-{enabled}",
            std::process::id()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("kassensturzrc");
        std::fs::write(&path, format!("[AI]\nEnabled={enabled}\n")).unwrap();
        ConfigFile::at(path)
    }

    fn hello(_: &[Message], _: &[String]) -> Result<String, AiError> {
        Ok(r#"{"choices":[{"finish_reason":"stop","message":{"content":"Hello"}}]}"#.into())
    }

    #[test]
    fn it_answers_through_the_real_reader() {
        let provider = ScriptedProvider::new(config(true), hello);
        let request = provider
            .prepare(vec![Message::user("Hi")], Vec::new())
            .unwrap();
        assert_eq!(request.address().as_str(), SCRIPTED_ADDRESS);
        assert_eq!(request.address().place(), crate::Place::ThisComputer);
        assert_eq!(provider.send(&request, None).unwrap().text, "Hello");

        let calls = ScriptedProvider::new(config(true), |_, _| {
            Ok(
                r#"{"choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[
                {"id":"a","function":{"name":"send_money","arguments":"{}"}}]}}]}"#
                    .into(),
            )
        });
        let reply = calls.send(&request, None).unwrap();
        assert!(reply.calls.is_empty(), "a function not offered is left out");
        assert_eq!(reply.ignored_calls, ["a"]);
    }

    #[test]
    fn it_runs_nothing_while_ai_is_off() {
        let provider = ScriptedProvider::new(config(false), |_, _| {
            panic!("the script must not run while AI is off")
        });
        assert_eq!(
            provider.prepare(vec![Message::user("Hi")], Vec::new()),
            Err(AiError::Off)
        );
    }
}
