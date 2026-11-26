// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What goes to a provider and what comes back: messages, the functions a
//! model may call, and its reply.
//!
//! Everything in a [`Reply`] comes from the provider and is untrusted
//! (threat T-21, X-8): its text is shown as plain text, and a function
//! call is only a request that the chat (M5.4) checks against the fixed
//! list of read functions (D-170). A call for a function that wasn't
//! offered is not handed on at all (threat T-31).

use std::fmt;

use base64::Engine as _;
use serde_json::Value;

/// Who a message is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// KassenSturz's own instructions to the model.
    System,
    /// The user.
    User,
    /// The model.
    Assistant,
    /// The result of a function the model called.
    Tool,
}

impl Role {
    /// The role's name in the OpenAI-compatible API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// The image format of an [`ImageAttachment`] sent to a multimodal model
/// (D-990, D-996).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageMediaType {
    /// PNG (`image/png`).
    Png,
    /// JPEG (`image/jpeg`).
    Jpeg,
}

impl ImageMediaType {
    /// The MIME type used in `data:<mime>;base64,...` URLs.
    pub fn as_mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    /// Parses a MIME type (`image/png` or `image/jpeg`, case-insensitive).
    pub fn from_mime(mime: &str) -> Option<Self> {
        if mime.eq_ignore_ascii_case("image/png") {
            Some(Self::Png)
        } else if mime.eq_ignore_ascii_case("image/jpeg") || mime.eq_ignore_ascii_case("image/jpg")
        {
            Some(Self::Jpeg)
        } else {
            None
        }
    }
}

/// An image attached to a [`Message`], serialized as a
/// `data:<mime>;base64,...` `image_url` part for multimodal models
/// (D-990, D-996).
#[derive(Clone, PartialEq, Eq)]
pub struct ImageAttachment {
    /// The image's media type.
    pub media_type: ImageMediaType,
    /// The encoded PNG or JPEG bytes.
    pub data: Vec<u8>,
}

impl fmt::Debug for ImageAttachment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageAttachment")
            .field("media_type", &self.media_type)
            .field("bytes", &self.data.len())
            .finish()
    }
}

impl ImageAttachment {
    /// Creates an image attachment with `media_type` and encoded image `data`.
    pub fn new(media_type: ImageMediaType, data: Vec<u8>) -> Self {
        Self { media_type, data }
    }

    /// Creates a PNG image attachment.
    pub fn png(data: Vec<u8>) -> Self {
        Self::new(ImageMediaType::Png, data)
    }

    /// Creates a JPEG image attachment.
    pub fn jpeg(data: Vec<u8>) -> Self {
        Self::new(ImageMediaType::Jpeg, data)
    }

    /// Encodes this attachment as a `data:<mime>;base64,...` URI.
    pub fn data_uri(&self) -> String {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&self.data);
        format!("data:{};base64,{encoded}", self.media_type.as_mime())
    }
}

/// One message of a conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Who it is from.
    pub role: Role,
    /// Its text.
    pub content: String,
    /// The functions the model called in this message ([`Role::Assistant`]
    /// only).
    pub calls: Vec<ToolCall>,
    /// The call this message answers ([`Role::Tool`] only).
    pub call_id: Option<String>,
    /// Images attached to this message ([`Role::User`] or [`Role::Tool`]
    /// only, D-990, D-996).
    pub images: Vec<ImageAttachment>,
}

impl Message {
    fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            calls: Vec::new(),
            call_id: None,
            images: Vec::new(),
        }
    }

    /// KassenSturz's instructions to the model.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    /// What the user wrote.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    /// What the user wrote, with attached images for a multimodal model
    /// (D-990, D-996).
    pub fn user_with_images(content: impl Into<String>, images: Vec<ImageAttachment>) -> Self {
        Self {
            images,
            ..Self::new(Role::User, content)
        }
    }

    /// What the model answered earlier, with the functions it called.
    pub fn assistant(content: impl Into<String>, calls: Vec<ToolCall>) -> Self {
        Self {
            calls,
            ..Self::new(Role::Assistant, content)
        }
    }

    /// The result of the call `call_id`.
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            call_id: Some(call_id.into()),
            ..Self::new(Role::Tool, content)
        }
    }

    /// The result of the call `call_id`, with attached images emitted in a
    /// portable follow-up `role: "user"` `image_url` message (D-990, D-996).
    pub fn tool_result_with_images(
        call_id: impl Into<String>,
        content: impl Into<String>,
        images: Vec<ImageAttachment>,
    ) -> Self {
        Self {
            images,
            ..Self::tool_result(call_id, content)
        }
    }

    /// Attaches `images` to this message.
    pub fn with_images(mut self, images: Vec<ImageAttachment>) -> Self {
        self.images = images;
        self
    }

    /// Whether this message carries images that will be sent to the provider
    /// ([`Role::User`] or [`Role::Tool`]).
    pub fn has_images(&self) -> bool {
        matches!(self.role, Role::User | Role::Tool) && !self.images.is_empty()
    }
}

/// A function the model may call: its name, what it does, and its
/// parameters as a JSON Schema object.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionSpec {
    /// The function's name.
    pub name: String,
    /// What it does, for the model.
    pub description: String,
    /// Its parameters, as a JSON Schema object.
    pub parameters: Value,
}

/// A function the model asked to call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    /// The provider's ID of the call, which the result names.
    pub id: String,
    /// The function, always one that was offered.
    pub name: String,
    /// The arguments as the model wrote them, a JSON text that the chat
    /// checks before it uses any of it.
    pub arguments: String,
}

/// Why the model stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Finish {
    /// It finished its answer.
    Done,
    /// It wants functions called.
    Calls,
    /// It hit the provider's length limit.
    Length,
    /// The provider filtered the answer, or gave another reason.
    Other,
}

/// What the model answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// Its text, possibly empty when it only calls functions.
    pub text: String,
    /// The functions it asked to call that were offered.
    pub calls: Vec<ToolCall>,
    /// The IDs of calls for functions that weren't offered. They are not
    /// carried out; the chat answers each with an error (threat T-31).
    pub ignored_calls: Vec<String>,
    /// Why it stopped.
    pub finish: Finish,
    /// How many tokens the provider counted in the request it answered,
    /// when it says (`usage.prompt_tokens`).
    pub prompt_tokens: Option<u64>,
    /// How many tokens the provider counted in its answer, thinking
    /// included, when it says (`usage.completion_tokens`).
    pub completion_tokens: Option<u64>,
}

/// One model a provider offers, with what its list says about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    /// The model's name, as the provider calls it.
    pub id: String,
    /// How many tokens it can read at once, when the server's list says so;
    /// never guessed (D-320).
    pub context_tokens: Option<u64>,
}
