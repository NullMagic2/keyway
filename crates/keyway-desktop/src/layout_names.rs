//! Module: keyway_desktop::layout_names
//! Purpose: Name layouts in one of Keyway's languages, from the system's own translations.
//! Created: 2026-11-25
//! Architecture: Reads the compiled catalog the desktop itself takes layout names from, for a
//! language that need not be the desktop's. Read fresh, so a newly installed language counts
//! at once. Where the system has no catalog for a language, callers keep the desktop's names.

use crate::layout_catalog::{SYSTEM_RULES_PATH, load_catalog};
use keyway_core::language::Language;
use keyway_core::layout_id::LayoutId;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Where systems keep translations; Ubuntu keeps those of its language packs apart.
const LOCALE_DIRECTORIES: [&str; 2] = ["/usr/share/locale", "/usr/share/locale-langpack"];
const CATALOG_FILE: &str = "LC_MESSAGES/xkeyboard-config.mo";
const MAX_CATALOG_BYTES: u64 = 4 * 1024 * 1024;

/// The start of a compiled catalog, as written by a machine of either byte order.
const CATALOG_MAGIC: u32 = 0x9504_12de;
const COUNT_OFFSET: usize = 8;
const ORIGINALS_OFFSET: usize = 12;
const TRANSLATIONS_OFFSET: usize = 16;
const WORD_BYTES: usize = 4;
const ENTRY_BYTES: usize = 8;

/// The translations of layout descriptions into one language.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameCatalog {
    names: HashMap<String, String>,
}

impl NameCatalog {
    /// Loads the system's catalog for a language. None if the system has none. English needs
    /// none, because the descriptions are written in it.
    pub fn load(language: Language) -> Option<Self> {
        if language == Language::English {
            return Some(Self::default());
        }

        language.locales().iter().find_map(|locale| {
            LOCALE_DIRECTORIES.iter().find_map(|directory| {
                let path = Path::new(directory).join(locale).join(CATALOG_FILE);
                let size = fs::metadata(&path).ok()?.len();

                if size > MAX_CATALOG_BYTES {
                    return None;
                }

                Self::parse(&fs::read(path).ok()?)
            })
        })
    }

    /// Reads a compiled gettext catalog. None for anything else.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let magic = bytes.get(..WORD_BYTES)?.try_into().ok()?;
        let big_endian = match (u32::from_le_bytes(magic), u32::from_be_bytes(magic)) {
            (CATALOG_MAGIC, _) => false,
            (_, CATALOG_MAGIC) => true,
            _ => return None,
        };
        let word = |offset: usize| {
            let word = bytes.get(offset..offset.checked_add(WORD_BYTES)?)?;
            let word = word.try_into().ok()?;
            let value = if big_endian {
                u32::from_be_bytes(word)
            } else {
                u32::from_le_bytes(word)
            };

            usize::try_from(value).ok()
        };
        let text_at = |table: usize, index: usize| {
            let entry = table.checked_add(index.checked_mul(ENTRY_BYTES)?)?;
            let length = word(entry)?;
            let start = word(entry.checked_add(WORD_BYTES)?)?;

            std::str::from_utf8(bytes.get(start..start.checked_add(length)?)?).ok()
        };
        let originals = word(ORIGINALS_OFFSET)?;
        let translations = word(TRANSLATIONS_OFFSET)?;
        let mut names = HashMap::new();

        for index in 0..word(COUNT_OFFSET)? {
            let original = text_at(originals, index)?;
            let translation = text_at(translations, index)?;

            // The entry without an original is the catalog's own description.
            if !original.is_empty() && !translation.is_empty() {
                names.insert(original.to_string(), translation.to_string());
            }
        }

        Some(Self { names })
    }

    /// The description in this catalog's language, or as it is where the catalog lacks it.
    pub fn name<'a>(&'a self, description: &'a str) -> &'a str {
        self.names
            .get(description)
            .map_or(description, String::as_str)
    }
}

/// Names each of these layouts in a language, by the id the desktop knows it by. Leaves out
/// what the system's rules do not describe, such as an input method. Empty if the system has
/// no translations for the language.
pub fn names_of<'a>(
    language: Language,
    layouts: impl Iterator<Item = (&'a LayoutId, &'a LayoutId)>,
) -> HashMap<String, String> {
    let Some(names) = NameCatalog::load(language) else {
        return HashMap::new();
    };
    let Ok(catalog) = load_catalog(Path::new(SYSTEM_RULES_PATH)) else {
        return HashMap::new();
    };

    layouts
        .filter_map(|(id, xkb_id)| {
            let entry = catalog.iter().find(|entry| entry.id == *xkb_id)?;

            Some((id.to_string(), names.name(&entry.description).to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compiles a catalog as msgfmt does, for a machine of either byte order.
    fn compiled(entries: &[(&str, &str)], big_endian: bool) -> Vec<u8> {
        let word = |value: usize| {
            let value = u32::try_from(value).unwrap();

            if big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            }
        };
        let header_bytes = TRANSLATIONS_OFFSET + 3 * WORD_BYTES;
        let table_bytes = entries.len() * ENTRY_BYTES;
        let mut tables = [Vec::new(), Vec::new()];
        let mut texts = Vec::new();

        for (original, translation) in entries {
            for (table, text) in tables.iter_mut().zip([original, translation]) {
                table.extend(word(text.len()));
                table.extend(word(header_bytes + 2 * table_bytes + texts.len()));
                texts.extend(text.as_bytes());
                texts.push(0);
            }
        }

        let mut bytes = Vec::new();
        bytes.extend(word(usize::try_from(CATALOG_MAGIC).unwrap()));
        bytes.extend(word(0));
        bytes.extend(word(entries.len()));
        bytes.extend(word(header_bytes));
        bytes.extend(word(header_bytes + table_bytes));
        bytes.extend(word(0));
        bytes.extend(word(0));
        bytes.extend(tables.concat());
        bytes.extend(texts);

        bytes
    }

    const ENTRIES: [(&str, &str); 3] = [
        ("", "Content-Type: text/plain; charset=UTF-8\n"),
        ("Greek", "Grego"),
        ("Portuguese (Brazil)", "Português (Brasil)"),
    ];

    #[test]
    fn a_catalog_of_either_byte_order_gives_its_translations() {
        for big_endian in [false, true] {
            let catalog = NameCatalog::parse(&compiled(&ENTRIES, big_endian)).unwrap();

            assert_eq!(catalog.name("Greek"), "Grego");
            assert_eq!(catalog.name("Portuguese (Brazil)"), "Português (Brasil)");
            assert_eq!(catalog.name("Klingon"), "Klingon");
            assert_eq!(catalog.name(""), "");
        }
    }

    #[test]
    fn anything_else_is_not_a_catalog() {
        let catalog = compiled(&ENTRIES, false);

        assert_eq!(NameCatalog::parse(b""), None);
        assert_eq!(NameCatalog::parse(b"not a catalog at all"), None);
        assert_eq!(NameCatalog::parse(&catalog[..catalog.len() - 30]), None);
    }

    #[test]
    fn english_names_are_the_descriptions_themselves() {
        let catalog = NameCatalog::load(Language::English).unwrap();

        assert_eq!(catalog.name("Greek"), "Greek");
    }
}
