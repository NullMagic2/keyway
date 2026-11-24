//! Module: keyway_settings::add_layout_dialog
//! Purpose: The searchable picker over every layout and variant on the system.
//! Created: 2026-11-08
//! Architecture: Reports the chosen layout; the window adds it to the desktop's list.

use crate::i18n::{Language, Message, text};
use crate::widgets::{self, DIM, LayoutBadges, SMALL};
use gtk::glib;
use gtk::prelude::*;
use keyway_core::layout_id::LayoutId;
use keyway_desktop::layout_catalog::CatalogEntry;
use std::cell::RefCell;
use std::rc::Rc;

const DIALOG_WIDTH: i32 = 460;
const LIST_HEIGHT: i32 = 340;
const CONTENT_SPACING: i32 = 10;
const CONTENT_MARGIN: i32 = 16;
const XKB_TRANSLATION_DOMAIN: &str = "xkeyboard-config";

/// Returns a layout description in the desktop's language, from the system's translations.
pub fn translated_description(description: &str) -> String {
    glib::dgettext(Some(XKB_TRANSLATION_DOMAIN), description).to_string()
}

struct PickerRow {
    id: LayoutId,
    search_text: String,
    available: bool,
}

/// A live picker dialog, whose parts the window test also drives.
#[derive(Clone, Debug)]
pub struct AddLayoutDialog {
    pub dialog: gtk::Dialog,
    pub search: gtk::SearchEntry,
    pub list: gtk::ListBox,
    pub add_button: gtk::Button,
}

impl AddLayoutDialog {
    pub fn present(
        parent: &gtk::Window,
        language: Language,
        catalog: &[CatalogEntry],
        badges: &LayoutBadges,
        flags_shown: bool,
        already_added: &[String],
        on_add: impl Fn(LayoutId) + 'static,
    ) -> Option<Self> {
        let dialog = gtk::Dialog::builder()
            .title(text(language, Message::AddLayoutTitle))
            .transient_for(parent)
            .modal(true)
            .use_header_bar(1)
            .default_width(DIALOG_WIDTH)
            .build();
        dialog.add_button(text(language, Message::Cancel), gtk::ResponseType::Cancel);

        let add_button = dialog
            .add_button(text(language, Message::Add), gtk::ResponseType::Accept)
            .downcast::<gtk::Button>()
            .ok()?;
        add_button.style_context().add_class("suggested-action");
        add_button.set_sensitive(false);

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(CONTENT_SPACING)
            .margin(CONTENT_MARGIN)
            .build();
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some(text(language, Message::SearchPlaceholder)));
        content.add(&search);

        let list = widgets::card();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(false);
        let scrolled = gtk::ScrolledWindow::builder()
            .min_content_height(LIST_HEIGHT)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&list)
            .build();
        content.add(&scrolled);
        dialog.content_area().add(&content);

        let rows = Rc::new(RefCell::new(Vec::with_capacity(catalog.len())));

        for entry in catalog {
            let description = translated_description(&entry.description);
            let available = !already_added.iter().any(|id| id == entry.id.as_str());
            let added_label = widgets::label(text(language, Message::Added), &[DIM, SMALL]);
            let badge = badges.badge(entry.id.as_str(), &entry.short_name, flags_shown, false);
            let row = widgets::row(
                &description,
                None,
                Some(badge.upcast_ref()),
                (!available).then_some(added_label.upcast_ref()),
            );
            row.set_activatable(available);
            row.set_selectable(available);
            list.add(&row);

            rows.borrow_mut().push(PickerRow {
                id: entry.id.clone(),
                search_text: format!("{description} {} {}", entry.description, entry.id)
                    .to_lowercase(),
                available,
            });
        }

        {
            let rows = Rc::clone(&rows);
            let search = search.clone();

            list.set_filter_func(Some(Box::new(move |row| {
                let query = search.text().to_lowercase();
                let rows = rows.borrow();
                let index = usize::try_from(row.index()).ok();
                let picker_row = index.and_then(|index| rows.get(index));

                picker_row.is_some_and(|picker_row: &PickerRow| {
                    picker_row.search_text.contains(query.trim())
                })
            })));
        }

        {
            let list = list.clone();

            search.connect_search_changed(move |_search| list.invalidate_filter());
        }

        {
            let add_button = add_button.clone();

            list.connect_row_selected(move |_list, row| {
                add_button.set_sensitive(row.is_some());
            });
        }

        {
            let dialog = dialog.clone();

            list.connect_row_activated(move |_list, _row| {
                dialog.response(gtk::ResponseType::Accept);
            });
        }

        {
            let list = list.clone();

            dialog.connect_response(move |dialog, response| {
                let chosen = list.selected_row().and_then(|row| {
                    let rows = rows.borrow();
                    let index = usize::try_from(row.index()).ok()?;
                    let picker_row = rows.get(index)?;

                    picker_row.available.then(|| picker_row.id.clone())
                });

                if response == gtk::ResponseType::Accept
                    && let Some(id) = chosen
                {
                    on_add(id);
                }

                dialog.close();
            });
        }

        dialog.show_all();

        Some(Self {
            dialog,
            search,
            list,
            add_button,
        })
    }
}
