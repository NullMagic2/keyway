//! Module: keyway_desktop::layout_catalog
//! Purpose: Read every layout and variant the system offers from its XKB rules file.
//! Created: 2026-11-08
//! Architecture: Feeds the settings window's picker. The file is read fresh, so Keyway
//! supports exactly what is installed and carries no list of its own.

use keyway_core::layout_id::LayoutId;
use roxmltree::{Document, Node, ParsingOptions};
use std::fs;
use std::io;
use std::path::Path;

pub const SYSTEM_RULES_PATH: &str = "/usr/share/X11/xkb/rules/evdev.xml";
const MAX_RULES_BYTES: u64 = 16 * 1024 * 1024;

/// One layout or variant that can be added as an input source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: LayoutId,
    pub short_name: String,
    pub description: String,
}

/// Reads the rules file, checking its size first because parsing keeps it all in memory.
pub fn load_catalog(path: &Path) -> io::Result<Vec<CatalogEntry>> {
    if fs::metadata(path)?.len() > MAX_RULES_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "XKB rules file is too large",
        ));
    }

    parse_catalog(&fs::read_to_string(path)?)
}

pub fn parse_catalog(text: &str) -> io::Result<Vec<CatalogEntry>> {
    // The system's rules file declares a DOCTYPE; it comes from the system, so that is fine.
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let document = Document::parse_with_options(text, options)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut entries = Vec::new();
    let layouts = document
        .descendants()
        .filter(|node| node.has_tag_name("layout"));

    for layout in layouts {
        let Some(base) = child(layout, "configItem") else {
            continue;
        };
        let Some(base_entry) = entry_from(base, None, "") else {
            continue;
        };
        let base_name = base_entry.id.clone();
        let base_short_name = base_entry.short_name.clone();
        entries.push(base_entry);

        let variants = layout
            .descendants()
            .filter(|node| node.has_tag_name("variant"));

        for variant in variants {
            let variant_entry = child(variant, "configItem")
                .and_then(|item| entry_from(item, Some(&base_name), &base_short_name));

            entries.extend(variant_entry);
        }
    }

    Ok(entries)
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|candidate| candidate.has_tag_name(name))
}

fn child_text<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name).and_then(|found| found.text())
}

fn entry_from(
    item: Node<'_, '_>,
    base: Option<&LayoutId>,
    base_short: &str,
) -> Option<CatalogEntry> {
    let name = child_text(item, "name")?;
    let description = child_text(item, "description")?;
    let id_text = match base {
        Some(base) => format!("{base}+{name}"),
        None => name.to_string(),
    };

    Some(CatalogEntry {
        id: LayoutId::try_from(id_text).ok()?,
        short_name: child_text(item, "shortDescription")
            .unwrap_or(base_short)
            .to_string(),
        description: description.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE xkbConfigRegistry SYSTEM "xkb.dtd">
        <xkbConfigRegistry version="1.1">
          <layoutList>
            <layout>
              <configItem>
                <name>gr</name>
                <!-- Keyboard indicator for Greek layouts -->
                <shortDescription>gr</shortDescription>
                <description>Greek</description>
              </configItem>
              <variantList>
                <variant>
                  <configItem>
                    <name>simple</name>
                    <description>Greek (simple)</description>
                  </configItem>
                </variant>
                <variant>
                  <configItem>
                    <name>extended</name>
                    <shortDescription>ex</shortDescription>
                    <description>Greek (extended)</description>
                  </configItem>
                </variant>
              </variantList>
            </layout>
          </layoutList>
        </xkbConfigRegistry>"#;

    #[test]
    fn layouts_and_variants_become_entries() {
        let entries = parse_catalog(SAMPLE).unwrap();
        let ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();

        assert_eq!(ids, ["gr", "gr+simple", "gr+extended"]);
        assert_eq!(entries[1].description, "Greek (simple)");
    }

    #[test]
    fn variants_inherit_the_short_name_unless_they_have_their_own() {
        let entries = parse_catalog(SAMPLE).unwrap();

        assert_eq!(entries[1].short_name, "gr");
        assert_eq!(entries[2].short_name, "ex");
    }

    #[test]
    fn malformed_files_are_errors() {
        assert!(parse_catalog("<layout>").is_err());
    }

    #[test]
    fn the_system_rules_file_lists_the_users_layouts() {
        let entries = load_catalog(Path::new(SYSTEM_RULES_PATH)).unwrap();
        let has = |id: &str| entries.iter().any(|entry| entry.id.as_str() == id);

        assert!(entries.len() > 100);
        assert!(has("br") && has("gr+simple") && has("us"));
    }
}
