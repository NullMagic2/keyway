//! Module: keyway_settings::alt_codes_page
//! Purpose: The page for Alt code options, with live examples for each layout.
//! Created: 2026-11-08
//! Architecture: Examples are decoded with the same code the agent uses to type them, so
//! this page cannot show one thing while the keyboard does another.

use crate::add_layout_dialog::translated_description;
use crate::i18n::{Language, Message, text};
use crate::source_list::SourceEntry;
use crate::widgets::{self, DIM, SMALL};
use gtk::prelude::*;
use keyway_core::alt_code::{AltCodeEntry, AltCodeRadix, NumLockPolicy};
use keyway_core::code_pages::{CodePageCatalog, CodePagePair, LayoutCodePages};
use keyway_core::config::AltCodesConfig;
use keyway_core::layout_id::LayoutId;
use keyway_desktop::layout_catalog::CatalogEntry;

const NUMLOCK_IGNORED_ID: &str = "ignored";
const NUMLOCK_REQUIRED_ID: &str = "required";
const EXAMPLE_CODES: [&str; 2] = ["130", "0225"];
const NO_CHARACTER: char = '–';

#[derive(Clone, Debug)]
pub struct AltCodesPage {
    pub root: gtk::Box,
    pub enabled: gtk::Switch,
    pub numlock: gtk::ComboBoxText,
    pub hex_numpad: gtk::Switch,
    pub unicode_above_255: gtk::Switch,
    pub code_pages: gtk::ListBox,
}

/// Formats examples for a layout, such as "Alt+130 = é   Alt+0225 = á".
pub fn example_text(pages: CodePagePair, catalog: &CodePageCatalog) -> String {
    let examples: Vec<String> = EXAMPLE_CODES
        .iter()
        .map(|digits| {
            let decoded = AltCodeEntry::new(AltCodeRadix::Decimal, (*digits).to_string())
                .ok()
                .and_then(|entry| entry.decode(pages, false, catalog));

            format!("Alt+{digits} = {}", decoded.unwrap_or(NO_CHARACTER))
        })
        .collect();

    examples.join("    ")
}

impl AltCodesPage {
    pub fn new(language: Language) -> Self {
        let root = widgets::page();
        root.add(&widgets::section(
            text(language, Message::AltCodes),
            Some(text(language, Message::AltCodesNote)),
        ));

        let enabled = gtk::Switch::new();
        let numlock = gtk::ComboBoxText::new();
        numlock.append(
            Some(NUMLOCK_IGNORED_ID),
            text(language, Message::NumLockIgnored),
        );
        numlock.append(
            Some(NUMLOCK_REQUIRED_ID),
            text(language, Message::NumLockRequired),
        );
        let hex_numpad = gtk::Switch::new();
        let unicode_above_255 = gtk::Switch::new();

        let options = widgets::card();
        options.add(&widgets::row(
            text(language, Message::TypeByCode),
            None,
            None,
            Some(enabled.upcast_ref()),
        ));
        options.add(&widgets::row(
            text(language, Message::NumLock),
            Some(text(language, Message::NumLockNote)),
            None,
            Some(numlock.upcast_ref()),
        ));
        options.add(&widgets::row(
            text(language, Message::HexCodes),
            Some(text(language, Message::HexCodesNote)),
            None,
            Some(hex_numpad.upcast_ref()),
        ));
        options.add(&widgets::row(
            text(language, Message::UnicodeCodes),
            Some(text(language, Message::UnicodeCodesNote)),
            None,
            Some(unicode_above_255.upcast_ref()),
        ));
        root.add(&options);

        root.add(&widgets::section(
            text(language, Message::CodePages),
            Some(text(language, Message::CodePagesNote)),
        ));

        let code_pages = widgets::card();
        root.add(&code_pages);

        Self {
            root,
            enabled,
            numlock,
            hex_numpad,
            unicode_above_255,
            code_pages,
        }
    }

    pub const fn numlock_id(policy: NumLockPolicy) -> &'static str {
        match policy {
            NumLockPolicy::Ignored => NUMLOCK_IGNORED_ID,
            NumLockPolicy::Required => NUMLOCK_REQUIRED_ID,
        }
    }

    pub fn numlock_from_id(id: &str) -> NumLockPolicy {
        if id == NUMLOCK_REQUIRED_ID {
            NumLockPolicy::Required
        } else {
            NumLockPolicy::Ignored
        }
    }

    /// Replaces the code page rows, one per listed XKB layout.
    pub fn show_code_pages(
        &self,
        language: Language,
        sources: &[SourceEntry],
        config: &AltCodesConfig,
        layout_pages: &LayoutCodePages,
        page_catalog: &CodePageCatalog,
        layout_catalog: &[CatalogEntry],
    ) {
        for child in self.code_pages.children() {
            self.code_pages.remove(&child);
        }

        for source in sources.iter().filter(|source| source.is_xkb()) {
            let Ok(id) = LayoutId::try_from(source.id.clone()) else {
                continue;
            };

            let pages = layout_pages.pages_for(&id, &config.code_pages);
            let known = layout_catalog.iter().find(|item| item.id == id);
            let title = known.map_or_else(
                || source.id.clone(),
                |item| translated_description(language, &item.description),
            );
            let page_numbers = format!(
                "OEM {} · ANSI {}",
                u16::from(pages.oem),
                u16::from(pages.ansi)
            );
            let examples = widgets::label(&example_text(pages, page_catalog), &[DIM, SMALL]);

            self.code_pages.add(&widgets::row(
                &title,
                Some(&page_numbers),
                None,
                Some(examples.upcast_ref()),
            ));
        }

        self.code_pages.show_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyway_core::code_pages::CodePageId;

    fn pages(oem: u16, ansi: u16) -> CodePagePair {
        CodePagePair {
            oem: CodePageId::try_from(oem).unwrap(),
            ansi: CodePageId::try_from(ansi).unwrap(),
        }
    }

    #[test]
    fn examples_come_from_the_layouts_code_pages() {
        let catalog = CodePageCatalog::load().unwrap();

        assert_eq!(
            example_text(pages(850, 1252), &catalog),
            "Alt+130 = é    Alt+0225 = á"
        );
        assert_eq!(
            example_text(pages(737, 1253), &catalog),
            "Alt+130 = Γ    Alt+0225 = α"
        );
    }
}
