//! Module: keyway_settings::widgets
//! Purpose: The small set of building blocks every settings page is made from.
//! Created: 2026-11-08
//! Architecture: Keeps the pages consistent; spacing and classes are decided only here.

use gtk::prelude::*;
use gtk::{cairo, gdk, gdk_pixbuf, gio, glib};
use keyway_core::hotkey::Chord;
use keyway_core::layout_labels::{LayoutLabel, LayoutLabels};
use std::cell::RefCell;
use std::collections::HashMap;

const STYLE_SHEET: &str = include_str!("style.css");
const CHIP_WIDTH: i32 = 38;
const FLAG_WIDTH: i32 = 20;
const FLAG_HEIGHT: i32 = 15;
const BADGE_SPACING: i32 = 8;

/// Flags are drawn at twice their size, so they stay sharp on a doubled display.
const FLAG_RENDER_SCALE: i32 = 2;
const ROW_VERTICAL_MARGIN: i32 = 9;
const ROW_START_MARGIN: i32 = 14;
const ROW_END_MARGIN: i32 = 10;
const ROW_SPACING: i32 = 12;
const PAGE_SPACING: i32 = 8;
const PAGE_TOP_MARGIN: i32 = 14;
const PAGE_BOTTOM_MARGIN: i32 = 20;
const PAGE_SIDE_MARGIN: i32 = 22;
const SECTION_TOP_MARGIN: i32 = 6;
const KEY_SPACING: i32 = 4;
const WRAP_WIDTH_CHARS: i32 = 60;
const CHORD_SEPARATOR: char = '+';

pub const DIM: &str = "keyway-dim";
pub const SMALL: &str = "keyway-small";

/// Installs the style sheet for the whole application.
pub fn install_style() -> Result<(), glib::Error> {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(STYLE_SHEET.as_bytes())?;

    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    Ok(())
}

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);

    for class in classes {
        label.style_context().add_class(class);
    }

    label
}

fn wrapping_label(text: &str, classes: &[&str]) -> gtk::Label {
    let label = label(text, classes);
    label.set_line_wrap(true);
    label.set_max_width_chars(WRAP_WIDTH_CHARS);

    label
}

pub fn page() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(PAGE_SPACING)
        .margin_top(PAGE_TOP_MARGIN)
        .margin_bottom(PAGE_BOTTOM_MARGIN)
        .margin_start(PAGE_SIDE_MARGIN)
        .margin_end(PAGE_SIDE_MARGIN)
        .build()
}

pub fn section(title: &str, note: Option<&str>) -> gtk::Box {
    let section = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(SECTION_TOP_MARGIN)
        .build();
    section.add(&label(title, &["keyway-section-title"]));

    if let Some(note) = note {
        section.add(&wrapping_label(note, &[DIM, SMALL]));
    }

    section
}

pub fn card() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.style_context().add_class("keyway-card");

    list
}

/// A row's subtitle, for a row that changes what it says.
pub fn note(text: &str) -> gtk::Label {
    wrapping_label(text, &[DIM, SMALL])
}

/// One card row: optional leading widget, title with optional subtitle, optional trailing.
pub fn row(
    title: &str,
    subtitle: Option<&str>,
    leading: Option<&gtk::Widget>,
    trailing: Option<&gtk::Widget>,
) -> gtk::ListBoxRow {
    row_with_note(title, subtitle.map(note).as_ref(), leading, trailing)
}

/// A card row whose subtitle the caller keeps.
pub fn row_with_note(
    title: &str,
    note: Option<&gtk::Label>,
    leading: Option<&gtk::Widget>,
    trailing: Option<&gtk::Widget>,
) -> gtk::ListBoxRow {
    let content = gtk::Box::builder()
        .spacing(ROW_SPACING)
        .margin_top(ROW_VERTICAL_MARGIN)
        .margin_bottom(ROW_VERTICAL_MARGIN)
        .margin_start(ROW_START_MARGIN)
        .margin_end(ROW_END_MARGIN)
        .build();

    if let Some(leading) = leading {
        content.pack_start(leading, false, false, 0);
    }

    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(1)
        .valign(gtk::Align::Center)
        .build();
    text.add(&wrapping_label(title, &[]));

    if let Some(note) = note {
        text.add(note);
    }

    content.pack_start(&text, true, true, 0);

    if let Some(trailing) = trailing {
        trailing.set_valign(gtk::Align::Center);
        content.pack_end(trailing, false, false, 0);
    }

    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.add(&content);

    row
}

pub fn chip(short_name: &str, active: bool) -> gtk::Label {
    let chip = gtk::Label::new(Some(&short_name.to_uppercase()));
    chip.style_context().add_class("keyway-chip");
    chip.set_size_request(CHIP_WIDTH, -1);
    chip.set_valign(gtk::Align::Center);

    if active {
        chip.style_context().add_class("keyway-active");
    }

    chip
}

/// Shows layouts as the taskbar does, with each flag drawn once and kept, because the layout
/// picker shows hundreds of layouts.
pub struct LayoutBadges {
    labels: LayoutLabels,
    flags: RefCell<HashMap<String, Option<cairo::Surface>>>,
}

impl LayoutBadges {
    pub fn new(labels: LayoutLabels) -> Self {
        Self {
            labels,
            flags: RefCell::new(HashMap::new()),
        }
    }

    /// A layout's flag when flags are shown, then its code. With flags shown, a layout without
    /// one keeps the space, so the codes line up. A layout the label table does not know shows
    /// the desktop's short name.
    pub fn badge(
        &self,
        source_id: &str,
        short_name: &str,
        flags_shown: bool,
        active: bool,
    ) -> gtk::Box {
        let label = self.labels.label_for(source_id);
        let badge = gtk::Box::builder()
            .spacing(BADGE_SPACING)
            .valign(gtk::Align::Center)
            .build();

        if flags_shown {
            let flag = label
                .and_then(|label| self.flag_surface(label))
                .map_or_else(gtk::Image::new, |surface| {
                    let image = gtk::Image::from_surface(Some(&surface));
                    image.style_context().add_class("keyway-flag");

                    image
                });
            flag.set_size_request(FLAG_WIDTH, FLAG_HEIGHT);
            flag.set_valign(gtk::Align::Center);
            badge.add(&flag);
        }

        let code = label.map_or(short_name, |label| label.code.as_str());
        badge.add(&chip(code, active));

        badge
    }

    fn flag_surface(&self, label: &LayoutLabel) -> Option<cairo::Surface> {
        let name = label.flag.as_ref()?;
        let mut flags = self.flags.borrow_mut();

        flags
            .entry(name.clone())
            .or_insert_with(|| label.flag_svg().and_then(draw_flag))
            .clone()
    }
}

/// Draws a flag, or returns None when the system cannot read SVG images.
fn draw_flag(svg: &'static [u8]) -> Option<cairo::Surface> {
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(svg));
    let pixbuf = gdk_pixbuf::Pixbuf::from_stream_at_scale(
        &stream,
        FLAG_WIDTH * FLAG_RENDER_SCALE,
        FLAG_HEIGHT * FLAG_RENDER_SCALE,
        true,
        None::<&gio::Cancellable>,
    )
    .ok()?;

    pixbuf.create_surface(FLAG_RENDER_SCALE, None::<&gdk::Window>)
}

/// Shows a shortcut as individual keycaps.
pub fn keycaps(chord: &Chord) -> gtk::Box {
    let keys = gtk::Box::builder().spacing(KEY_SPACING).build();
    let text = chord.to_string();

    for key in text.split(CHORD_SEPARATOR) {
        let cap = gtk::Label::new(Some(key));
        cap.style_context().add_class("keyway-key");
        keys.add(&cap);
    }

    keys
}

pub fn flat_icon_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(Some(icon_name), gtk::IconSize::Button);
    button.set_relief(gtk::ReliefStyle::None);
    button.set_tooltip_text(Some(tooltip));

    button
}
