// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Plugins page's object (#791 step 3, AGENTS.md §7, D-851): the plugins
//! the user added, a file the user picked to look at before adding it, and
//! removing a plugin's copy. QML gets the manifests' texts as plain text and
//! the extension points and permissions as their words; the page owns the
//! plain-language explanations (D-034). Texts from a plugin file are data,
//! shown as plain text (AGENTS.md §8).

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QUrl, QVariant};
use kassensturz_plugin_api::Manifest;

use crate::book_session::local_path;
use crate::errors::ErrorText;
use crate::plugins::{self, Added, Listed, Refused};
use crate::variant::{error_object, list, map, text};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt's string type.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qurl.h");
        /// A file the user picked.
        type QUrl = cxx_qt_lib::QUrl;

        include!("cxx-qt-lib/qvariant.h");
        /// A value of any Qt type.
        type QVariant = cxx_qt_lib::QVariant;
    }

    extern "RustQt" {
        /// The plugins the user added, sorted by name: `items`, each an
        /// object with `file` and `ok`; with `ok`, the manifest's `id`,
        /// `name`, `version`, `author`, `description`, `license` and
        /// `homepage` (texts, "" when not given), `points` and
        /// `permissions` (their manifest words); without it, `error`, the
        /// code and parameters of why it can't be used. `pending`, the file
        /// the user picked to add, as the same kind of object plus
        /// `replaces` and `installedVersion`, or empty.
        #[qobject]
        #[qml_element]
        #[qproperty(QVariant, items, READ, NOTIFY)]
        #[qproperty(QVariant, pending, READ, NOTIFY)]
        type Plugins = super::PluginsRust;
    }

    extern "RustQt" {
        /// Reads the plugin folder again. `ai_enabled` is the AI switch:
        /// while it is off, AI plugins are not listed (D-013).
        #[qinvokable]
        fn refresh(self: Pin<&mut Plugins>, ai_enabled: bool);

        /// Reads and checks a file the user picked, and keeps it as
        /// `pending` for `add`. Returns an empty object, or the code of why
        /// it can't be added.
        #[qinvokable]
        fn inspect(self: Pin<&mut Plugins>, file: &QUrl, ai_enabled: bool) -> QVariant;

        /// Adds the `pending` file. A plugin with the same ID is replaced
        /// only with `replace`. Returns an empty object, or the code of why
        /// it wasn't added; `pending` is cleared either way.
        #[qinvokable]
        fn add(self: Pin<&mut Plugins>, replace: bool, ai_enabled: bool) -> QVariant;

        /// Forgets the `pending` file.
        #[qinvokable]
        fn cancel(self: Pin<&mut Plugins>);

        /// Removes the copy `file` of a listed plugin. Returns whether it is
        /// gone.
        #[qinvokable]
        fn remove(self: Pin<&mut Plugins>, file: &QString, ai_enabled: bool) -> bool;
    }

    impl cxx_qt::Initialize for Plugins {}
}

/// The Rust side of [`qobject::Plugins`].
#[derive(Default)]
pub struct PluginsRust {
    items: QVariant,
    pending: QVariant,
    pending_bytes: Option<Vec<u8>>,
}

impl cxx_qt::Initialize for qobject::Plugins {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh(false);
    }
}

impl qobject::Plugins {
    fn refresh(mut self: Pin<&mut Self>, ai_enabled: bool) {
        let listed = plugins::folder()
            .map(|folder| plugins::list(&folder, ai_enabled))
            .unwrap_or_default();
        self.as_mut().rust_mut().items = items_object(&listed);
        self.as_mut().items_changed();
    }

    fn inspect(mut self: Pin<&mut Self>, file: &QUrl, ai_enabled: bool) -> QVariant {
        self.as_mut().cancel();
        let Some(path) = local_path(file) else {
            return error_object(&ErrorText::plain("input.not-a-local-file"));
        };
        let Some(folder) = plugins::folder() else {
            return error_object(&plugins::error_text(&Refused::NoFolder));
        };
        let checked = plugins::read(&path)
            .and_then(|bytes| plugins::check(&bytes, ai_enabled).map(|manifest| (bytes, manifest)));
        match checked {
            Ok((bytes, manifest)) => {
                let added = plugins::would_add(&folder, &manifest);
                let pending = pending_object(&manifest, &added);
                let mut model = self.as_mut().rust_mut();
                model.pending = pending;
                model.pending_bytes = Some(bytes);
                self.as_mut().pending_changed();
                map([])
            }
            Err(refused) => error_object(&plugins::error_text(&refused)),
        }
    }

    fn add(mut self: Pin<&mut Self>, replace: bool, ai_enabled: bool) -> QVariant {
        let bytes = self.as_mut().rust_mut().pending_bytes.take();
        self.as_mut().cancel();
        let Some(bytes) = bytes else {
            return error_object(&ErrorText::plain("input.no-such-entry"));
        };
        let Some(folder) = plugins::folder() else {
            return error_object(&plugins::error_text(&Refused::NoFolder));
        };
        let result = match plugins::add(&folder, &bytes, ai_enabled, replace) {
            Ok((_, Added::Replaces { .. })) if !replace => {
                error_object(&ErrorText::plain("input.plugin-replace-not-confirmed"))
            }
            Ok(_) => map([]),
            Err(refused) => error_object(&plugins::error_text(&refused)),
        };
        self.refresh(ai_enabled);
        result
    }

    fn cancel(mut self: Pin<&mut Self>) {
        let mut model = self.as_mut().rust_mut();
        model.pending_bytes = None;
        model.pending = QVariant::default();
        self.as_mut().pending_changed();
    }

    fn remove(self: Pin<&mut Self>, file: &QString, ai_enabled: bool) -> bool {
        let file = file.to_string();
        let removed = plugins::folder().is_some_and(|folder| plugins::remove(&folder, &file));
        self.refresh(ai_enabled);
        removed
    }
}

/// The listed plugins as QML reads them.
fn items_object(listed: &[Listed]) -> QVariant {
    list(listed.iter().map(|listed| match &listed.entry {
        Ok(manifest) => plugin_object(&listed.file, manifest, []),
        Err(error) => map([
            ("file", text(&listed.file)),
            ("ok", QVariant::from(&false)),
            ("error", error_object(error)),
        ]),
    }))
}

/// A picked file's plugin, with what adding it would do.
fn pending_object(manifest: &Manifest, added: &Added) -> QVariant {
    let (replaces, installed) = match added {
        Added::New => (false, String::new()),
        Added::Replaces { installed } => (
            true,
            installed
                .map(|version| version.to_string())
                .unwrap_or_default(),
        ),
    };
    plugin_object(
        "",
        manifest,
        [
            ("replaces", QVariant::from(&replaces)),
            ("installedVersion", text(&installed)),
        ],
    )
}

fn plugin_object<'a>(
    file: &str,
    manifest: &Manifest,
    more: impl IntoIterator<Item = (&'a str, QVariant)>,
) -> QVariant {
    let optional = |value: &Option<String>| text(value.as_deref().unwrap_or_default());
    map([
        ("file", text(file)),
        ("ok", QVariant::from(&true)),
        ("id", text(manifest.id.as_str())),
        ("name", text(&manifest.name)),
        ("version", text(&manifest.version.to_string())),
        ("author", optional(&manifest.author)),
        ("description", optional(&manifest.description)),
        ("license", optional(&manifest.license)),
        ("homepage", optional(&manifest.homepage)),
        (
            "points",
            list(
                manifest
                    .extension_points
                    .iter()
                    .map(|point| text(point.key())),
            ),
        ),
        (
            "permissions",
            list(
                manifest
                    .permissions
                    .iter()
                    .map(|permission| text(permission.key())),
            ),
        ),
    ]
    .into_iter()
    .chain(more))
}

#[cfg(test)]
mod tests {
    use kassensturz_plugin_api::PluginVersion;
    use serde_json::json;

    use super::*;
    use crate::variant::read_back::{flag_of, keys, list_of, object_of, text_of, texts_of};

    fn manifest() -> Manifest {
        Manifest::parse(
            json!({
                "manifest_version": 1,
                "id": "org.example.csv",
                "name": "Example <b>CSV</b>",
                "version": "1.2.0",
                "api": "1.0",
                "extension_points": ["automation", "importer"],
                "permissions": ["propose_changes", "read_book"],
                "author": "Example Author",
                "homepage": "https://example.org/csv"
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn the_listed_plugins_read_back_as_qml_reads_them() {
        let listed = [
            Listed {
                file: "org.example.csv.wasm".into(),
                entry: Ok(manifest()),
            },
            Listed {
                file: "damaged.wasm".into(),
                entry: Err(ErrorText::plain("plugin.not-a-plugin")),
            },
        ];
        let items = items_object(&listed);
        let items = crate::variant::read_back::elements(&items).expect("an array");
        assert_eq!(items.len(), 2);
        assert_eq!(
            keys(&items[0]),
            [
                "author",
                "description",
                "file",
                "homepage",
                "id",
                "license",
                "name",
                "ok",
                "permissions",
                "points",
                "version"
            ]
        );
        assert!(flag_of(&items[0], "ok"));
        assert_eq!(text_of(&items[0], "file"), "org.example.csv.wasm");
        // Text from the file is passed on as it is; the page shows it as
        // plain text.
        assert_eq!(text_of(&items[0], "name"), "Example <b>CSV</b>");
        assert_eq!(text_of(&items[0], "version"), "1.2.0");
        assert_eq!(text_of(&items[0], "author"), "Example Author");
        assert_eq!(text_of(&items[0], "description"), "");
        assert_eq!(text_of(&items[0], "license"), "");
        assert_eq!(text_of(&items[0], "homepage"), "https://example.org/csv");
        assert_eq!(texts_of(&items[0], "points"), ["importer", "automation"]);
        assert_eq!(
            texts_of(&items[0], "permissions"),
            ["read_book", "propose_changes"]
        );

        assert_eq!(keys(&items[1]), ["error", "file", "ok"]);
        assert!(!flag_of(&items[1], "ok"));
        assert_eq!(
            text_of(&object_of(&items[1], "error"), "code"),
            "plugin.not-a-plugin"
        );
    }

    #[test]
    fn the_pending_file_says_what_adding_would_do() {
        let new = pending_object(&manifest(), &Added::New);
        assert!(!flag_of(&new, "replaces"));
        assert_eq!(text_of(&new, "installedVersion"), "");
        assert_eq!(text_of(&new, "file"), "");
        assert_eq!(list_of(&new, "points").len(), 2);

        let replaces = pending_object(
            &manifest(),
            &Added::Replaces {
                installed: PluginVersion::parse("1.0.0"),
            },
        );
        assert!(flag_of(&replaces, "replaces"));
        assert_eq!(text_of(&replaces, "installedVersion"), "1.0.0");

        let unreadable = pending_object(&manifest(), &Added::Replaces { installed: None });
        assert!(flag_of(&unreadable, "replaces"));
        assert_eq!(text_of(&unreadable, "installedVersion"), "");
    }
}
