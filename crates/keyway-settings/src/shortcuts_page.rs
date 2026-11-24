//! Module: keyway_settings::shortcuts_page
//! Purpose: The page that shows the switching shortcuts and direct layout shortcuts.
//! Created: 2026-11-08
//! Architecture: Rebuilt from the configuration on every change; window.rs handles actions.

use crate::add_layout_dialog::translated_description;
use crate::i18n::{Language, Message, text};
use crate::source_list::SourceEntry;
use crate::widgets::{self, DIM, SMALL};
use gtk::prelude::*;
use keyway_core::config::{SwitchOrder, SwitchingConfig};
use keyway_core::hotkey::Chord;
use keyway_core::layout_id::LayoutId;
use keyway_desktop::layout_catalog::CatalogEntry;

const ORDER_LIST_ID: &str = "list";
const ORDER_RECENT_ID: &str = "recent";

/// The shortcut each row sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShortcutSlot {
    Next,
    Previous,
    Direct(LayoutId),
}

/// A button that opens the capture dialog for one shortcut.
#[derive(Clone, Debug)]
pub struct ShortcutButton {
    pub slot: ShortcutSlot,
    pub button: gtk::Button,
}

#[derive(Clone, Debug)]
pub struct ShortcutsPage {
    pub root: gtk::Box,
    pub switching: gtk::ListBox,
    pub direct: gtk::ListBox,
    pub order: gtk::ComboBoxText,
}

fn shortcut_button(language: Language, chord: Option<&Chord>) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);

    match chord {
        Some(chord) => button.add(&widgets::keycaps(chord)),
        None => button.add(&widgets::label(
            text(language, Message::NotSet),
            &[DIM, SMALL],
        )),
    }

    button
}

impl ShortcutsPage {
    pub fn new(language: Language) -> Self {
        let root = widgets::page();
        root.add(&widgets::section(
            text(language, Message::Switching),
            Some(text(language, Message::SwitchingNote)),
        ));

        let switching = widgets::card();
        root.add(&switching);

        let order = gtk::ComboBoxText::new();
        order.append(Some(ORDER_LIST_ID), text(language, Message::OrderList));
        order.append(Some(ORDER_RECENT_ID), text(language, Message::OrderRecent));
        let order_card = widgets::card();
        order_card.add(&widgets::row(
            text(language, Message::SwitchingOrder),
            None,
            None,
            Some(order.upcast_ref()),
        ));
        root.add(&order_card);

        root.add(&widgets::section(
            text(language, Message::JumpToLayout),
            Some(text(language, Message::JumpToLayoutNote)),
        ));

        let direct = widgets::card();
        root.add(&direct);

        Self {
            root,
            switching,
            direct,
            order,
        }
    }

    pub fn order_id(order: SwitchOrder) -> &'static str {
        match order {
            SwitchOrder::List => ORDER_LIST_ID,
            SwitchOrder::Recent => ORDER_RECENT_ID,
        }
    }

    pub fn order_from_id(id: &str) -> SwitchOrder {
        if id == ORDER_RECENT_ID {
            SwitchOrder::Recent
        } else {
            SwitchOrder::List
        }
    }

    /// Replaces the rows and returns their buttons for the window to connect.
    pub fn show_shortcuts(
        &self,
        language: Language,
        switching: &SwitchingConfig,
        sources: &[SourceEntry],
        catalog: &[CatalogEntry],
    ) -> Vec<ShortcutButton> {
        for list in [&self.switching, &self.direct] {
            for child in list.children() {
                list.remove(&child);
            }
        }

        let mut buttons = Vec::new();

        let next = shortcut_button(language, Some(&switching.next));
        self.switching.add(&widgets::row(
            text(language, Message::NextLayout),
            Some(text(language, Message::ReleaseNote)),
            None,
            Some(next.upcast_ref()),
        ));
        buttons.push(ShortcutButton {
            slot: ShortcutSlot::Next,
            button: next,
        });

        let previous = shortcut_button(language, switching.previous.as_ref());
        self.switching.add(&widgets::row(
            text(language, Message::PreviousLayout),
            None,
            None,
            Some(previous.upcast_ref()),
        ));
        buttons.push(ShortcutButton {
            slot: ShortcutSlot::Previous,
            button: previous,
        });

        for source in sources {
            let Ok(id) = LayoutId::try_from(source.id.clone()) else {
                continue;
            };

            let known = catalog.iter().find(|item| item.id == id);
            let title = known.map_or_else(
                || source.id.clone(),
                |item| translated_description(&item.description),
            );
            let button = shortcut_button(language, switching.direct.get(&id));
            self.direct
                .add(&widgets::row(&title, None, None, Some(button.upcast_ref())));
            buttons.push(ShortcutButton {
                slot: ShortcutSlot::Direct(id),
                button,
            });
        }

        self.switching.show_all();
        self.direct.show_all();

        buttons
    }
}
