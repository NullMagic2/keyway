//! Module: keyway_settings::load_extension_dialog
//! Purpose: The steps that give a running GNOME Shell the extension it cannot be asked to load.
//! Created: 2026-11-09
//! Architecture: Only shows and copies the line that keyway_desktop::restart writes; the person
//! runs it in the Shell's own Looking Glass, which nothing outside the Shell can reach.

use crate::i18n::{Language, Message, text};
use crate::widgets;
use gtk::gdk;
use gtk::prelude::*;

const DIALOG_WIDTH: i32 = 520;
const CONTENT_SPACING: i32 = 12;
const CONTENT_MARGIN: i32 = 20;
const LINE_SPACING: i32 = 8;

/// A live dialog, whose parts the window test also drives.
#[derive(Clone, Debug)]
pub struct LoadExtensionDialog {
    pub dialog: gtk::Dialog,
    pub line: gtk::Entry,
    pub copy_button: gtk::Button,
}

impl LoadExtensionDialog {
    pub fn present(parent: &gtk::Window, language: Language, line: &str) -> Self {
        let dialog = gtk::Dialog::builder()
            .title(text(language, Message::LoadExtensionTitle))
            .transient_for(parent)
            .modal(true)
            .use_header_bar(1)
            .default_width(DIALOG_WIDTH)
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(CONTENT_SPACING)
            .margin(CONTENT_MARGIN)
            .build();
        let intro = widgets::label(text(language, Message::LoadExtensionIntro), &[]);
        intro.set_line_wrap(true);
        content.add(&intro);
        content.add(&widgets::label(
            text(language, Message::LoadExtensionSteps),
            &[],
        ));

        // The whole line stays selectable, for anyone who would rather copy it by hand.
        let line_entry = gtk::Entry::builder()
            .text(line)
            .editable(false)
            .hexpand(true)
            .build();
        line_entry.style_context().add_class("keyway-code");

        let copy_button = gtk::Button::with_label(text(language, Message::CopyLine));
        copy_button.style_context().add_class("suggested-action");

        let line_row = gtk::Box::builder().spacing(LINE_SPACING).build();
        line_row.add(&line_entry);
        line_row.add(&copy_button);
        content.add(&line_row);
        dialog.content_area().add(&content);

        {
            let line = line.to_string();

            copy_button.connect_clicked(move |button| {
                gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(&line);
                button.set_label(text(language, Message::LineCopied));
            });
        }

        dialog.connect_response(|dialog, _response| dialog.close());
        dialog.show_all();
        copy_button.grab_focus();

        Self {
            dialog,
            line: line_entry,
            copy_button,
        }
    }
}
