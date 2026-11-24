//! Module: keyway_settings::layouts_page
//! Purpose: The page that lists, reorders and removes layouts.
//! Created: 2026-11-08
//! Architecture: Rebuilt from the desktop's list on every change; window.rs handles actions.

use crate::add_layout_dialog::translated_description;
use crate::i18n::{Language, Message, text};
use crate::source_list::SourceEntry;
use crate::widgets::LayoutBadges;
use crate::widgets::{self, DIM, SMALL};
use gtk::prelude::*;
use keyway_core::config::KeyboardTheme;
use keyway_desktop::layout_catalog::CatalogEntry;

const ROW_ACTION_SPACING: i32 = 2;
const IN_USE_MARGIN: u32 = 8;
const ICON_UP: &str = "go-up-symbolic";
const ICON_DOWN: &str = "go-down-symbolic";
const ICON_REMOVE: &str = "user-trash-symbolic";
const ICON_DONE: &str = "emblem-ok-symbolic";
const ICON_PROBLEM: &str = "dialog-warning-symbolic";
const RESTART_SPACING: i32 = 10;
const DONE_SPACING: i32 = 5;

/// The buttons of one layout row, in list order.
#[derive(Clone, Debug)]
pub struct LayoutRow {
    pub entry: SourceEntry,
    pub in_use: bool,
    pub up: gtk::Button,
    pub down: gtk::Button,
    pub remove: gtk::Button,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartStage<'a> {
    Idle,
    Running,
    Done,

    /// With what did not restart, and why.
    Failed(&'a str),
}

/// The restart row's parts: a spinner beside the button while Keyway restarts, then either a
/// confirmation or a warning, which stays until the next restart. The reasons for a warning
/// take the place of the row's note, so that they show where the person is looking.
#[derive(Clone, Debug)]
pub struct RestartControl {
    pub button: gtk::Button,
    pub spinner: gtk::Spinner,
    pub done: gtk::Box,
    pub problem: gtk::Image,
    pub note: gtk::Label,
    usual_note: &'static str,
}

impl RestartControl {
    /// Returns the control and the row that holds all of it.
    fn new(language: Language) -> (Self, gtk::ListBoxRow) {
        let usual_note = text(language, Message::RestartKeywayNote);
        let note = widgets::note(usual_note);
        let problem = gtk::Image::from_icon_name(Some(ICON_PROBLEM), gtk::IconSize::Button);
        let spinner = gtk::Spinner::new();
        let done = gtk::Box::builder().spacing(DONE_SPACING).build();
        let done_icon = gtk::Image::from_icon_name(Some(ICON_DONE), gtk::IconSize::Button);
        let done_text = widgets::label(text(language, Message::Restarted), &[DIM, SMALL]);
        done.add(&done_icon);
        done.add(&done_text);
        done_icon.show();
        done_text.show();

        // Left out of show_all, so each only appears at its stage.
        spinner.set_no_show_all(true);
        done.set_no_show_all(true);
        problem.set_no_show_all(true);

        let button = gtk::Button::with_label(text(language, Message::Restart));
        let holder = gtk::Box::builder().spacing(RESTART_SPACING).build();
        holder.add(&spinner);
        holder.add(&done);
        holder.add(&problem);
        holder.add(&button);

        let row = widgets::row_with_note(
            text(language, Message::RestartKeyway),
            Some(&note),
            None,
            Some(holder.upcast_ref()),
        );
        let control = Self {
            button,
            spinner,
            done,
            problem,
            note,
            usual_note,
        };

        (control, row)
    }

    /// The button cannot start a second restart while one is under way.
    pub fn show_stage(&self, stage: RestartStage<'_>) {
        let running = stage == RestartStage::Running;
        let reasons = match stage {
            RestartStage::Failed(reasons) => Some(reasons),
            _ => None,
        };

        self.button.set_sensitive(!running);
        self.spinner.set_visible(running);
        self.spinner.set_active(running);
        self.done.set_visible(stage == RestartStage::Done);
        self.problem.set_visible(reasons.is_some());
        self.note.set_text(reasons.unwrap_or(self.usual_note));

        // Reasons are not a side note, so they are not dimmed like one.
        if reasons.is_some() {
            self.note.style_context().remove_class(DIM);
        } else {
            self.note.style_context().add_class(DIM);
        }
    }
}

#[derive(Clone, Debug)]
pub struct LayoutsPage {
    pub root: gtk::Box,
    pub list: gtk::ListBox,
    pub add_button: gtk::Button,
    pub per_window: gtk::Switch,
    pub flags: gtk::Switch,
    pub keyboard_theme: gtk::ComboBoxText,
    pub system_keyboard: Option<gtk::Button>,
    pub restart: RestartControl,
}

/// How the rows show each layout: the system's names, the taskbar's badges, and whether flags
/// are on.
#[derive(Clone, Copy)]
pub struct LayoutDisplay<'a> {
    pub catalog: &'a [CatalogEntry],
    pub badges: &'a LayoutBadges,
    pub flags_shown: bool,
}

impl LayoutsPage {
    /// Offers the system's keyboard settings only when has_system_keyboard says they exist.
    pub fn new(language: Language, has_system_keyboard: bool) -> Self {
        let root = widgets::page();
        root.add(&widgets::section(
            text(language, Message::YourLayouts),
            Some(text(language, Message::YourLayoutsNote)),
        ));

        let list = widgets::card();
        root.add(&list);

        let add_row = gtk::Box::builder().margin_top(4).build();
        let add_button = gtk::Button::with_label(text(language, Message::AddLayoutButton));
        add_button.style_context().add_class("suggested-action");
        add_row.pack_end(&add_button, false, false, 0);
        root.add(&add_row);

        root.add(&widgets::section(text(language, Message::Options), None));

        let per_window = gtk::Switch::new();
        let options = widgets::card();
        options.add(&widgets::row(
            text(language, Message::PerWindow),
            Some(text(language, Message::PerWindowNote)),
            None,
            Some(per_window.upcast_ref()),
        ));

        let flags = gtk::Switch::new();
        options.add(&widgets::row(
            text(language, Message::ShowFlags),
            Some(text(language, Message::ShowFlagsNote)),
            None,
            Some(flags.upcast_ref()),
        ));

        let keyboard_theme = gtk::ComboBoxText::new();

        for theme in KeyboardTheme::ALL {
            let name = match theme {
                KeyboardTheme::Light => Message::ThemeLight,
                KeyboardTheme::Dark => Message::ThemeDark,
            };

            keyboard_theme.append(Some(theme.name()), text(language, name));
        }

        options.add(&widgets::row(
            text(language, Message::KeyboardColors),
            Some(text(language, Message::KeyboardColorsNote)),
            None,
            Some(keyboard_theme.upcast_ref()),
        ));

        let system_keyboard = has_system_keyboard.then(|| {
            let button = gtk::Button::with_label(text(language, Message::Open));
            options.add(&widgets::row(
                text(language, Message::SystemKeyboard),
                Some(text(language, Message::SystemKeyboardNote)),
                None,
                Some(button.upcast_ref()),
            ));

            button
        });

        let (restart, restart_row) = RestartControl::new(language);
        options.add(&restart_row);
        root.add(&options);

        Self {
            root,
            list,
            add_button,
            per_window,
            flags,
            keyboard_theme,
            system_keyboard,
            restart,
        }
    }

    /// Replaces the rows and returns their buttons for the window to connect. Up on the first
    /// row, down on the last, and remove on an only layout are insensitive.
    pub fn show_sources(
        &self,
        language: Language,
        sources: &[SourceEntry],
        active: Option<&str>,
        display: LayoutDisplay<'_>,
    ) -> Vec<LayoutRow> {
        for child in self.list.children() {
            self.list.remove(&child);
        }

        let last_index = sources.len().saturating_sub(1);
        let rows: Vec<LayoutRow> = sources
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let in_use = active == Some(entry.id.as_str());
                let row = self.add_row(language, entry, in_use, display);
                row.up.set_sensitive(index > 0);
                row.down.set_sensitive(index < last_index);
                row.remove.set_sensitive(sources.len() > 1);

                row
            })
            .collect();

        self.list.show_all();

        rows
    }

    fn add_row(
        &self,
        language: Language,
        entry: &SourceEntry,
        in_use: bool,
        display: LayoutDisplay<'_>,
    ) -> LayoutRow {
        let known = display
            .catalog
            .iter()
            .find(|item| item.id.as_str() == entry.id);
        let title = known.map_or_else(
            || entry.id.clone(),
            |item| translated_description(&item.description),
        );
        let short_name = known.map_or(entry.id.as_str(), |item| item.short_name.as_str());

        let up = widgets::flat_icon_button(ICON_UP, text(language, Message::MoveUp));
        let down = widgets::flat_icon_button(ICON_DOWN, text(language, Message::MoveDown));
        let remove = widgets::flat_icon_button(ICON_REMOVE, text(language, Message::RemoveLayout));
        let actions = gtk::Box::builder().spacing(ROW_ACTION_SPACING).build();

        if in_use {
            let in_use_label = widgets::label(text(language, Message::InUse), &[DIM, SMALL]);
            actions.pack_start(&in_use_label, false, false, IN_USE_MARGIN);
        }

        actions.add(&up);
        actions.add(&down);
        actions.add(&remove);

        let badge = display
            .badges
            .badge(&entry.id, short_name, display.flags_shown, in_use);
        let row = widgets::row(
            &title,
            Some(&entry.id),
            Some(badge.upcast_ref()),
            Some(actions.upcast_ref()),
        );
        self.list.add(&row);

        LayoutRow {
            entry: entry.clone(),
            in_use,
            up,
            down,
            remove,
        }
    }
}
