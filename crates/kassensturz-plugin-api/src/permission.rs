// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a plugin can do and what it may ask for (AGENTS.md §7, D-754,
//! D-756, D-848).

/// A place in KassenSturz a plugin can fill.
///
/// AGENTS.md §7 names the first four; AI provider is D-754, Theme and UI
/// density are D-756. Settings pages, Wizards and Dashboard widgets come
/// with plugin API v1.1 after Gold (#818, D-756, D-759).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtensionPoint {
    /// Reads a file the user picked and hands back statements, which go
    /// through the same import preview as a built-in importer (D-069).
    Importer,
    /// Writes the book, or a part of it, into a file the user picked.
    Exporter,
    /// Computes a report from the book.
    Report,
    /// Reacts to events, such as an import, and may propose changes the user
    /// confirms.
    Automation,
    /// Talks to an AI service that isn't OpenAI-compatible (D-754). Hidden
    /// and never loaded while AI is off (D-013).
    AiProvider,
    /// Colours, accent, fonts and corner radius. Data only, no code (D-756).
    Theme,
    /// Switches KassenSturz itself defines, such as hiding explanation
    /// texts. Data only, no code (D-756).
    UiDensity,
}

impl ExtensionPoint {
    /// Every extension point of API v1, in the order the Plugins tab lists
    /// them.
    pub const ALL: [Self; 7] = [
        Self::Importer,
        Self::Exporter,
        Self::Report,
        Self::Automation,
        Self::AiProvider,
        Self::Theme,
        Self::UiDensity,
    ];

    /// The word a manifest uses for it, also its error keyword.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Importer => "importer",
            Self::Exporter => "exporter",
            Self::Report => "report",
            Self::Automation => "automation",
            Self::AiProvider => "ai_provider",
            Self::Theme => "theme",
            Self::UiDensity => "ui_density",
        }
    }

    /// The extension point a manifest word names, if any.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|point| point.key() == key)
    }

    /// Whether a plugin fills it with data only, without any WASM code
    /// (D-756). A data-only plugin asks for no permission and can't be
    /// combined with an extension point that runs code.
    pub const fn is_data_only(self) -> bool {
        matches!(self, Self::Theme | Self::UiDensity)
    }

    /// Whether it belongs to the AI features, so KassenSturz hides the
    /// plugin and never loads it while AI is off (D-013, D-754).
    pub const fn is_ai(self) -> bool {
        matches!(self, Self::AiProvider)
    }

    /// The permissions a plugin filling this extension point may ask for
    /// (D-848). Nothing else is ever granted to it.
    pub const fn allowed_permissions(self) -> &'static [Permission] {
        match self {
            Self::Importer | Self::Exporter | Self::Report => &[Permission::ReadBook],
            Self::Automation => &[Permission::ReadBook, Permission::ProposeChanges],
            Self::AiProvider => &[Permission::Network],
            Self::Theme | Self::UiDensity => &[],
        }
    }
}

/// Something a plugin asks to be allowed (D-848). The Plugins tab shows
/// each one in plain words before the user adds the plugin.
///
/// There is deliberately no permission for files or the database: an
/// importer gets the bytes of the file the user picked, an exporter hands
/// back the bytes the host writes to the file the user picked, and nothing
/// ever reaches the database directly (AGENTS.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Permission {
    /// Read the book: accounts, categories, transactions, plans, budgets
    /// and tags, through a read-only view.
    ReadBook,
    /// Hand back a change set, which the user reviews and confirms like the
    /// AI's changes (one Undo, change history); the plugin never writes.
    ProposeChanges,
    /// Connect to other computers. Only an AI provider may ask for it, and
    /// consent and the preview of what is sent stay in KassenSturz (D-754).
    Network,
}

impl Permission {
    /// Every permission of API v1, from the least to the most reaching.
    pub const ALL: [Self; 3] = [Self::ReadBook, Self::ProposeChanges, Self::Network];

    /// The word a manifest uses for it, also its error keyword.
    pub const fn key(self) -> &'static str {
        match self {
            Self::ReadBook => "read_book",
            Self::ProposeChanges => "propose_changes",
            Self::Network => "network",
        }
    }

    /// The permission a manifest word names, if any.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|permission| permission.key() == key)
    }
}
