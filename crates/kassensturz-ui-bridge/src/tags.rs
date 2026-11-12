// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tags as the screens show and take them (US4.8, D-712, D-713): free
//! labels on a whole transaction or on one of its parts, e.g. "Tax" or
//! "Holiday 2026". A tag never moves money and never changes a sum.
//!
//! The screens get tags as `{id, name}` objects and give back the same, with
//! an empty `id` for a tag the user typed that the book doesn't have yet:
//! it is made in the same user action as the save, so one Undo takes back
//! both (D-086).

use std::collections::HashMap;

use kassensturz_core::{Tag, TagId};
use kassensturz_store::{Book, StoreError};

/// A tag as the screens show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagText {
    /// Its ID as text.
    pub id: String,
    /// Its name.
    pub name: String,
}

/// A tag as a screen gives it back: an existing one by `id`, or a new one
/// by `name` with an empty `id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TagInput<'a> {
    /// The ID as text, or empty for a new tag.
    pub id: &'a str,
    /// The name; used only when `id` is empty.
    pub name: &'a str,
}

/// A tag in the list to choose from, with how often it is used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagChoice {
    /// The tag.
    pub tag: TagText,
    /// How many transactions carry it on the whole of them.
    pub transactions: usize,
    /// How many parts carry it as one of their own.
    pub parts: usize,
}

/// Every tag of the book, sorted by name, with how often each is used.
pub fn tag_choices(book: &Book) -> Result<Vec<TagChoice>, StoreError> {
    let uses = book.tag_uses()?;
    Ok(book
        .tags()?
        .into_iter()
        .map(|tag| {
            let used = uses.get(&tag.id).copied().unwrap_or_default();
            TagChoice {
                tag: TagText {
                    id: tag.id.to_string(),
                    name: tag.name,
                },
                transactions: used.transactions,
                parts: used.parts,
            }
        })
        .collect())
}

/// The names of the book's tags by ID, for showing tags read from a
/// transaction.
pub fn names(book: &Book) -> Result<HashMap<TagId, String>, StoreError> {
    Ok(book
        .tags()?
        .into_iter()
        .map(|tag| (tag.id, tag.name))
        .collect())
}

/// `tags` as the screens show them, in their order; a tag the book no
/// longer has is left out.
pub fn texts(names: &HashMap<TagId, String>, tags: &[TagId]) -> Vec<TagText> {
    tags.iter()
        .filter_map(|tag| {
            names.get(tag).map(|name| TagText {
                id: tag.to_string(),
                name: name.clone(),
            })
        })
        .collect()
}

/// The tags a screen gave back, as IDs, in their order and each once. A
/// new tag is added to the book, or, if one has its name already (ignoring
/// capitals and spaces around it), that one is taken. Call this inside the
/// user action that saves them, so one Undo takes back both. An ID the
/// book doesn't have is refused (`store.unknown-tag`), and so is a name the
/// book can't keep (`store.empty-name`, `store.tag-name`,
/// `store.tag-too-many`).
pub fn resolve(book: &mut Book, wanted: &[TagInput<'_>]) -> Result<Vec<TagId>, StoreError> {
    let mut known: Option<Vec<Tag>> = None;
    let mut ids = Vec::with_capacity(wanted.len());
    for input in wanted {
        let id = if input.id.is_empty() {
            let name = kassensturz_store::checked_tag_name(input.name)?;
            let tags = match &mut known {
                Some(tags) => tags,
                None => known.insert(book.tags()?),
            };
            let lower = name.to_lowercase();
            match tags
                .iter()
                .find(|tag| tag.name.trim().to_lowercase() == lower)
            {
                Some(tag) => tag.id,
                None => {
                    let tag = Tag {
                        id: TagId::generate(),
                        name,
                    };
                    book.add_tag(&tag)?;
                    let id = tag.id;
                    tags.push(tag);
                    id
                }
            }
        } else {
            let tags = match &mut known {
                Some(tags) => tags,
                None => known.insert(book.tags()?),
            };
            tags.iter()
                .find(|tag| tag.id.to_string() == input.id)
                .map(|tag| tag.id)
                .ok_or_else(|| StoreError::UnknownTag(TagId::generate()))?
        };
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// The tag whose ID is `id` (as text); one the book doesn't have is
/// refused (`store.unknown-tag`).
fn find(book: &Book, id: &str) -> Result<TagId, StoreError> {
    book.tags()?
        .into_iter()
        .find(|tag| tag.id.to_string() == id)
        .map(|tag| tag.id)
        .ok_or_else(|| StoreError::UnknownTag(TagId::generate()))
}

/// Renames the tag `id` (as text) everywhere it is used; Undo takes it
/// back (D-086).
pub fn rename(book: &mut Book, id: &str, name: &str) -> Result<(), StoreError> {
    let tag = find(book, id)?;
    book.rename_tag(tag, name)
}

/// Removes the tag `id` from every transaction and part and from the book,
/// in one user action that one Undo takes back (Q-495).
pub fn remove(book: &mut Book, id: &str) -> Result<(), StoreError> {
    let tag = find(book, id)?;
    book.remove_tag(tag)
}

/// Puts the tag `into` wherever `from` is and removes `from`, in one user
/// action that one Undo takes back.
pub fn merge(book: &mut Book, from: &str, into: &str) -> Result<(), StoreError> {
    let from = find(book, from)?;
    let into = find(book, into)?;
    book.merge_tags(from, into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_tags_are_made_once_and_known_names_are_reused() {
        let mut book = Book::create_in_memory().expect("book");
        let holiday = Tag {
            id: TagId::generate(),
            name: "Holiday".into(),
        };
        book.add_tag(&holiday).expect("added");
        let id = holiday.id.to_string();
        let resolved = resolve(
            &mut book,
            &[
                TagInput {
                    id: "",
                    name: " holiday ",
                },
                TagInput {
                    id: "",
                    name: "Tax",
                },
                TagInput {
                    id: "",
                    name: "TAX",
                },
                TagInput { id: &id, name: "" },
            ],
        )
        .expect("resolved");
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0], holiday.id);
        let choices = tag_choices(&book).expect("choices");
        let names: Vec<&str> = choices
            .iter()
            .map(|choice| choice.tag.name.as_str())
            .collect();
        assert_eq!(names, ["Holiday", "Tax"]);
        assert!(matches!(
            resolve(
                &mut book,
                &[TagInput {
                    id: "not-an-id",
                    name: ""
                }]
            ),
            Err(StoreError::UnknownTag(_))
        ));
        assert!(matches!(
            resolve(
                &mut book,
                &[TagInput {
                    id: "",
                    name: "a\nb"
                }]
            ),
            Err(StoreError::TagName { .. })
        ));
        let shown = texts(&names_of(&book), &resolved);
        assert_eq!(shown[1].name, "Tax");
    }

    #[test]
    fn a_tag_is_renamed_merged_and_removed_by_its_id_as_text() {
        let mut book = Book::create_in_memory().expect("book");
        let wanted = [
            TagInput {
                id: "",
                name: "Holiday",
            },
            TagInput {
                id: "",
                name: "Trip",
            },
        ];
        let made = book
            .as_one_action(kassensturz_store::Action::AddTransaction, |book| {
                resolve(book, &wanted)
            })
            .expect("made");
        let (holiday, trip) = (made[0].to_string(), made[1].to_string());
        rename(&mut book, &holiday, "Holiday 2026").expect("renamed");
        assert!(matches!(
            rename(&mut book, &trip, "holiday 2026"),
            Err(StoreError::DuplicateTagName { .. })
        ));
        merge(&mut book, &trip, &holiday).expect("merged");
        let left = names_of(&book);
        assert_eq!(left.values().collect::<Vec<_>>(), ["Holiday 2026"]);
        assert!(matches!(
            remove(&mut book, &trip),
            Err(StoreError::UnknownTag(_))
        ));
        remove(&mut book, &holiday).expect("removed");
        assert!(names_of(&book).is_empty());
        // Each was one step that Undo takes back.
        book.undo().expect("undone");
        assert_eq!(names_of(&book).len(), 1);
        book.undo().expect("undone");
        assert_eq!(names_of(&book).len(), 2);
    }

    fn names_of(book: &Book) -> HashMap<TagId, String> {
        names(book).expect("names")
    }
}
