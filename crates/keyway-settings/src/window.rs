//! Module: keyway_settings::window
//! Purpose: The settings window: its pages, its state, and what each control does.
//! Created: 2026-11-08
//! Architecture: The desktop owns layouts and the agent follows the configuration file, so
//! every change is written at once and this window keeps nothing else of its own.

use crate::add_layout_dialog::AddLayoutDialog;
use crate::alt_codes_page::AltCodesPage;
use crate::capture_dialog::{CaptureDialog, CaptureResult};
use crate::i18n::{Language, Message, text};
use crate::input_sources::InputSourceSettings;
use crate::layouts_page::{LayoutDisplay, LayoutRow, LayoutsPage, RestartStage};
use crate::load_extension_dialog::LoadExtensionDialog;
use crate::shortcuts_page::{ShortcutButton, ShortcutSlot, ShortcutsPage};
use crate::source_list::{SourceEntry, add_source, move_source, remove_source};
use crate::widgets::{self, LayoutBadges};
use gtk::glib;
use gtk::prelude::*;
use keyway_core::code_pages::{CodePageCatalog, LayoutCodePages};
use keyway_core::config::{Config, KeyboardTheme};
use keyway_core::layout_id::LayoutId;
use keyway_core::layout_labels::LayoutLabels;
use keyway_desktop::config_file::{
    CONFIG_FILE_NAME, back_up_invalid_config, config_directory, load_config, save_config,
};
use keyway_desktop::desktop_shell::DesktopShell;
use keyway_desktop::layout_catalog::{CatalogEntry, SYSTEM_RULES_PATH, load_catalog};
use keyway_desktop::restart::{self, Part, RestartReport};
use std::cell::{Cell, RefCell};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::thread;

const WINDOW_WIDTH: i32 = 620;
const PAGE_MIN_HEIGHT: i32 = 420;
const PAGE_MAX_HEIGHT: i32 = 680;

pub const PAGE_LAYOUTS: &str = "layouts";
pub const PAGE_SHORTCUTS: &str = "shortcuts";
pub const PAGE_ALT_CODES: &str = "alt_codes";

/// Applies one on-or-off option to the configuration.
type ConfigToggle = fn(&mut Config, bool);

/// Long lists scroll inside the page, so the window itself never outgrows the screen.
fn scrolling_page(page: &gtk::Box) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(PAGE_MIN_HEIGHT)
        .max_content_height(PAGE_MAX_HEIGHT)
        .propagate_natural_height(true)
        .child(page)
        .build()
}

const fn part_message(part: Part) -> Message {
    match part {
        Part::Extension => Message::PartExtension,
        Part::Agent => Message::PartAgent,
        Part::Router => Message::PartRouter,
    }
}

thread_local! {
    /// Lets a background thread reach the window, whose widgets must stay on this thread.
    static RUNNING_APP: RefCell<Option<std::rc::Weak<SettingsApp>>> = const { RefCell::new(None) };
}

struct State {
    config: Config,
    config_path: PathBuf,
    config_needs_backup: bool,
    active_source: Option<String>,
    layout_rows: Vec<LayoutRow>,
    shortcut_buttons: Vec<ShortcutButton>,
    load_extension_dialog: Option<LoadExtensionDialog>,
}

/// The running window. Its parts are public so the window test can operate real widgets.
pub struct SettingsApp {
    pub language: Language,
    pub window: gtk::Window,
    pub stack: gtk::Stack,
    pub banner: gtk::InfoBar,
    banner_text: gtk::Label,
    pub conflict_banner: gtk::InfoBar,
    pub use_only_keyway: gtk::Button,
    pub layouts: LayoutsPage,
    pub shortcuts: ShortcutsPage,
    pub alt_codes: AltCodesPage,
    input_sources: InputSourceSettings,
    catalog: Vec<CatalogEntry>,
    page_catalog: CodePageCatalog,
    layout_pages: LayoutCodePages,
    badges: LayoutBadges,
    state: RefCell<State>,
    loading_controls: Cell<bool>,
}

impl SettingsApp {
    /// Builds the window from the system's layouts and the saved configuration, in the
    /// language saved there or else the desktop's.
    pub fn new(desktop_language: Language) -> Result<Rc<Self>, Box<dyn Error>> {
        let config_path = config_directory()?.join(CONFIG_FILE_NAME);
        let catalog = load_catalog(Path::new(SYSTEM_RULES_PATH))?;

        Self::with_parts(desktop_language, config_path, catalog)
    }

    /// Builds the window from explicit inputs.
    pub fn with_parts(
        desktop_language: Language,
        config_path: PathBuf,
        catalog: Vec<CatalogEntry>,
    ) -> Result<Rc<Self>, Box<dyn Error>> {
        widgets::install_style()?;

        let (config, config_needs_backup) = match load_config(&config_path) {
            Ok(config) => (config, false),
            Err(_) => (Config::default(), true),
        };
        let language = config.interface.language.unwrap_or(desktop_language);

        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(WINDOW_WIDTH, -1);
        window.set_title(text(language, Message::WindowTitle));

        let stack = gtk::Stack::new();
        let input_sources = InputSourceSettings::default();
        let layouts = LayoutsPage::new(language, input_sources.has_keyboard_panel());
        let shortcuts = ShortcutsPage::new(language);
        let alt_codes = AltCodesPage::new(language);
        stack.add_titled(
            &scrolling_page(&layouts.root),
            PAGE_LAYOUTS,
            text(language, Message::PageLayouts),
        );
        stack.add_titled(
            &scrolling_page(&shortcuts.root),
            PAGE_SHORTCUTS,
            text(language, Message::PageShortcuts),
        );
        stack.add_titled(
            &scrolling_page(&alt_codes.root),
            PAGE_ALT_CODES,
            text(language, Message::PageAltCodes),
        );

        let switcher = gtk::StackSwitcher::new();
        switcher.set_stack(Some(&stack));
        let header = gtk::HeaderBar::new();
        header.set_show_close_button(true);
        header.set_custom_title(Some(&switcher));

        // The switcher hides this, but the header bar's title is what the desktop's window
        // list and app switcher are told.
        header.set_title(Some(text(language, Message::WindowTitle)));
        window.set_titlebar(Some(&header));

        // Left out of show_all, so the bar only appears when there is something to say.
        let banner = gtk::InfoBar::new();
        banner.set_message_type(gtk::MessageType::Warning);
        banner.set_no_show_all(true);
        let banner_text = widgets::label("", &[]);
        banner_text.set_line_wrap(true);
        banner.content_area().add(&banner_text);

        let conflict_banner = gtk::InfoBar::new();
        conflict_banner.set_message_type(gtk::MessageType::Warning);
        conflict_banner.set_no_show_all(true);
        let conflict_text = widgets::label(text(language, Message::SystemShortcutConflict), &[]);
        conflict_text.set_line_wrap(true);
        conflict_text.show();
        conflict_banner.content_area().add(&conflict_text);
        let use_only_keyway = gtk::Button::with_label(text(language, Message::UseOnlyKeyway));
        use_only_keyway.show();
        conflict_banner.add_action_widget(&use_only_keyway, gtk::ResponseType::Accept);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.add(&banner);
        content.add(&conflict_banner);
        content.add(&stack);
        window.add(&content);

        let app = Rc::new(Self {
            language,
            window,
            stack,
            banner,
            banner_text,
            conflict_banner,
            use_only_keyway,
            layouts,
            shortcuts,
            alt_codes,
            input_sources,
            catalog,
            page_catalog: CodePageCatalog::load()?,
            layout_pages: LayoutCodePages::load()?,
            badges: LayoutBadges::new(LayoutLabels::load()?),
            state: RefCell::new(State {
                config,
                config_path,
                config_needs_backup,
                active_source: None,
                layout_rows: Vec::new(),
                shortcut_buttons: Vec::new(),
                load_extension_dialog: None,
            }),
            loading_controls: Cell::new(false),
        });

        RUNNING_APP.with(|running| *running.borrow_mut() = Some(Rc::downgrade(&app)));
        Self::connect_controls(&app);
        app.refresh_all();

        if config_needs_backup {
            app.show_banner(Message::ConfigInvalid);
        }

        Ok(app)
    }

    pub fn window(&self) -> &gtk::Window {
        &self.window
    }

    pub fn layout_rows(&self) -> Vec<LayoutRow> {
        self.state.borrow().layout_rows.clone()
    }

    pub fn shortcut_buttons(&self) -> Vec<ShortcutButton> {
        self.state.borrow().shortcut_buttons.clone()
    }

    pub fn config(&self) -> Config {
        self.state.borrow().config.clone()
    }

    pub fn sources(&self) -> Vec<SourceEntry> {
        self.input_sources.sources()
    }

    /// The dialog a restart opened, if GNOME Shell could not be asked to load the extension.
    pub fn load_extension_dialog(&self) -> Option<LoadExtensionDialog> {
        self.state.borrow().load_extension_dialog.clone()
    }

    /// The window, for work that finished on another thread.
    fn running() -> Option<Rc<Self>> {
        RUNNING_APP.with(|running| running.borrow().as_ref().and_then(std::rc::Weak::upgrade))
    }

    fn show_banner(&self, message: Message) {
        self.banner_text.set_text(text(self.language, message));
        self.banner_text.show();
        self.banner.show();
    }

    /// Restarts Keyway away from this thread, as the password dialog stays open for as long as
    /// the person leaves it.
    fn start_restart(&self) {
        self.layouts.restart.show_stage(RestartStage::Running);

        thread::spawn(|| {
            let report = restart::restart();

            glib::idle_add_once(move || {
                if let Some(app) = Self::running() {
                    app.show_restart_report(&report);
                }
            });
        });
    }

    /// Names each part that did not restart, with the system's own reason, in the restart row
    /// itself. An extension that only the person can load gets its steps instead.
    fn show_restart_report(&self, report: &RestartReport) {
        let failures: Vec<String> = report
            .parts
            .iter()
            .filter(|(part, _failure)| !(report.needs_looking_glass && *part == Part::Extension))
            .filter_map(|(part, failure)| {
                let name = text(self.language, part_message(*part));

                failure.as_ref().map(|failure| format!("{name}: {failure}"))
            })
            .collect();
        let summary = text(self.language, Message::RestartIncomplete);
        let reasons = format!("{summary}\n{}", failures.join("\n"));
        let stage = if report.succeeded() {
            RestartStage::Done
        } else if failures.is_empty() {
            RestartStage::Idle
        } else {
            RestartStage::Failed(&reasons)
        };

        self.layouts.restart.show_stage(stage);

        if report.needs_looking_glass {
            let dialog = LoadExtensionDialog::present(
                &self.window,
                self.language,
                &restart::looking_glass_line(),
            );

            self.state.borrow_mut().load_extension_dialog = Some(dialog);
        }
    }

    fn connect_controls(app: &Rc<Self>) {
        let weak = Rc::downgrade(app);
        let with_app = move |action: &dyn Fn(&Rc<Self>)| {
            if let Some(app) = weak.upgrade() {
                action(&app);
            }
        };

        {
            let with_app = with_app.clone();

            app.input_sources.connect_sources_changed(move || {
                with_app(&|app| app.refresh_all());
            });
        }

        {
            let with_app = with_app.clone();

            app.input_sources.connect_system_shortcuts_changed(move || {
                with_app(&|app| app.refresh_conflict());
            });
        }

        {
            let with_app = with_app.clone();

            app.use_only_keyway.connect_clicked(move |_button| {
                with_app(&|app| {
                    if app.input_sources.remove_modifier_only_switching().is_err() {
                        app.show_banner(Message::SaveFailed);
                    }
                });
            });
        }

        if let Some(button) = &app.layouts.system_keyboard {
            let with_app = with_app.clone();

            button.connect_clicked(move |_button| {
                with_app(&|app| {
                    if app.input_sources.open_keyboard_panel().is_err() {
                        app.show_banner(Message::OpenFailed);
                    }
                });
            });
        }

        {
            let with_app = with_app.clone();

            app.layouts.restart.button.connect_clicked(move |_button| {
                with_app(&|app| app.start_restart());
            });
        }

        {
            let with_app = with_app.clone();

            app.layouts.add_button.connect_clicked(move |_button| {
                with_app(&|app| {
                    Self::open_add_layout(app);
                });
            });
        }

        {
            let with_app = with_app.clone();

            app.layouts
                .per_window
                .connect_state_set(move |_switch, enabled| {
                    with_app(&|app| {
                        if !app.loading_controls.get()
                            && app.input_sources.set_per_window(enabled).is_err()
                        {
                            app.show_banner(Message::SaveFailed);
                        }
                    });

                    glib::Propagation::Proceed
                });
        }

        {
            let with_app = with_app.clone();

            app.layouts.flags.connect_state_set(move |_switch, shown| {
                with_app(&|app| {
                    if !app.loading_controls.get() {
                        app.change_config(|config| config.indicator.flags = shown);
                        app.refresh_layout_rows(&app.sources());
                    }
                });

                glib::Propagation::Proceed
            });
        }

        {
            let with_app = with_app.clone();

            app.layouts.keyboard_theme.connect_changed(move |combo| {
                let theme = combo
                    .active_id()
                    .and_then(|id| KeyboardTheme::from_name(&id));

                with_app(&|app| {
                    if let Some(theme) = theme {
                        app.change_config(|config| config.keyboard.theme = theme);
                    }
                });
            });
        }

        {
            let with_app = with_app.clone();

            app.shortcuts.order.connect_changed(move |combo| {
                let id = combo
                    .active_id()
                    .map(|id| id.to_string())
                    .unwrap_or_default();

                with_app(&|app| {
                    app.change_config(|config| {
                        config.switching.order = ShortcutsPage::order_from_id(&id);
                    });
                });
            });
        }

        {
            let with_app = with_app.clone();

            app.alt_codes.numlock.connect_changed(move |combo| {
                let id = combo
                    .active_id()
                    .map(|id| id.to_string())
                    .unwrap_or_default();

                with_app(&|app| {
                    app.change_config(|config| {
                        config.alt_codes.numlock = AltCodesPage::numlock_from_id(&id);
                    });
                });
            });
        }

        let switches: [(&gtk::Switch, ConfigToggle); 3] = [
            (&app.alt_codes.enabled, |config, on| {
                config.alt_codes.enabled = on
            }),
            (&app.alt_codes.hex_numpad, |config, on| {
                config.alt_codes.hex_numpad = on
            }),
            (&app.alt_codes.unicode_above_255, |config, on| {
                config.alt_codes.unicode_above_255 = on;
            }),
        ];

        for (switch, apply) in switches {
            let with_app = with_app.clone();

            switch.connect_state_set(move |_switch, on| {
                with_app(&|app| app.change_config(|config| apply(config, on)));

                glib::Propagation::Proceed
            });
        }
    }

    /// Applies a change and saves it at once, unless the window is only showing saved values.
    fn change_config(&self, change: impl Fn(&mut Config)) {
        if self.loading_controls.get() {
            return;
        }

        let saved = {
            let mut state = self.state.borrow_mut();

            // The taskbar menu may have saved a language since this window read the file.
            if let Ok(saved) = load_config(&state.config_path) {
                state.config.interface = saved.interface;
            }

            change(&mut state.config);

            let path = state.config_path.clone();
            let backed_up = state.config_needs_backup && back_up_invalid_config(&path).is_ok();

            if backed_up {
                state.config_needs_backup = false;
            }

            save_config(&path, &state.config).map(|()| backed_up)
        };

        match saved {
            Ok(true) => self.show_banner(Message::ConfigBackedUp),
            Ok(false) => {}
            Err(_) => self.show_banner(Message::SaveFailed),
        }
    }

    /// Shows the current sources, configuration and code pages in every page.
    pub fn refresh_all(self: &Rc<Self>) {
        self.loading_controls.set(true);

        let config = self.config();
        let sources = self.sources();

        self.layouts
            .per_window
            .set_active(self.input_sources.per_window());
        self.layouts.flags.set_active(config.indicator.flags);
        self.layouts
            .keyboard_theme
            .set_active_id(Some(config.keyboard.theme.name()));
        self.shortcuts
            .order
            .set_active_id(Some(ShortcutsPage::order_id(config.switching.order)));
        self.alt_codes.enabled.set_active(config.alt_codes.enabled);
        self.alt_codes
            .numlock
            .set_active_id(Some(AltCodesPage::numlock_id(config.alt_codes.numlock)));
        self.alt_codes
            .hex_numpad
            .set_active(config.alt_codes.hex_numpad);
        self.alt_codes
            .unicode_above_255
            .set_active(config.alt_codes.unicode_above_255);
        self.alt_codes.show_code_pages(
            self.language,
            &sources,
            &config.alt_codes,
            &self.layout_pages,
            &self.page_catalog,
            &self.catalog,
        );

        self.loading_controls.set(false);

        self.refresh_layout_rows(&sources);
        self.refresh_shortcut_buttons(&sources, &config);
        self.refresh_add_button(&sources);
        self.refresh_conflict();
    }

    /// A desktop's limit is explained on the button, so it never just looks broken.
    fn refresh_add_button(&self, sources: &[SourceEntry]) {
        let at_limit = self
            .input_sources
            .layout_limit()
            .is_some_and(|limit| sources.len() >= limit);
        let tooltip = at_limit.then(|| text(self.language, Message::LayoutLimitReached));

        self.layouts.add_button.set_sensitive(!at_limit);
        self.layouts.add_button.set_tooltip_text(tooltip);
    }

    /// Warns only while one press could really switch twice: a shortcut with a final key on
    /// Keyway's side never reaches the desktop, so it cannot collide.
    fn refresh_conflict(&self) {
        let switching = self.config().switching;
        let keyway_uses_modifiers = switching.next.is_modifier_only()
            || switching
                .previous
                .is_some_and(|chord| chord.is_modifier_only());
        let conflict = keyway_uses_modifiers && self.input_sources.has_modifier_only_switching();

        self.conflict_banner.set_visible(conflict);
    }

    fn refresh_layout_rows(self: &Rc<Self>, sources: &[SourceEntry]) {
        let (active, flags_shown) = {
            let state = self.state.borrow();

            (state.active_source.clone(), state.config.indicator.flags)
        };
        let display = LayoutDisplay {
            catalog: &self.catalog,
            badges: &self.badges,
            flags_shown,
        };
        let rows = self
            .layouts
            .show_sources(self.language, sources, active.as_deref(), display);

        for (index, row) in rows.iter().enumerate() {
            let weak = Rc::downgrade(self);
            let edit = move |change: &dyn Fn(&mut Vec<SourceEntry>) -> bool| {
                if let Some(app) = weak.upgrade() {
                    app.edit_sources(change);
                }
            };

            let on_up = edit.clone();
            row.up.connect_clicked(move |_button| {
                on_up(&|sources| move_source(sources, index, true));
            });

            let on_down = edit.clone();
            row.down.connect_clicked(move |_button| {
                on_down(&|sources| move_source(sources, index, false));
            });

            row.remove.connect_clicked(move |_button| {
                edit(&|sources| remove_source(sources, index));
            });
        }

        self.state.borrow_mut().layout_rows = rows;
    }

    fn refresh_shortcut_buttons(self: &Rc<Self>, sources: &[SourceEntry], config: &Config) {
        let buttons =
            self.shortcuts
                .show_shortcuts(self.language, &config.switching, sources, &self.catalog);

        for shortcut in &buttons {
            let weak = Rc::downgrade(self);
            let slot = shortcut.slot.clone();

            shortcut.button.connect_clicked(move |_button| {
                if let Some(app) = weak.upgrade() {
                    Self::open_capture(&app, slot.clone());
                }
            });
        }

        self.state.borrow_mut().shortcut_buttons = buttons;
    }

    /// Writes an edited list to the desktop. The change signal then refreshes the window.
    fn edit_sources(&self, change: &dyn Fn(&mut Vec<SourceEntry>) -> bool) {
        let mut sources = self.sources();

        if change(&mut sources) && self.input_sources.set_sources(&sources).is_err() {
            self.show_banner(Message::SaveFailed);
        }
    }

    /// Opens the layout picker. Returned so the window test can operate it.
    pub fn open_add_layout(app: &Rc<Self>) -> Option<AddLayoutDialog> {
        let already_added: Vec<String> = app.sources().into_iter().map(|entry| entry.id).collect();
        let weak = Rc::downgrade(app);

        AddLayoutDialog::present(
            &app.window,
            app.language,
            &app.catalog,
            &app.badges,
            app.config().indicator.flags,
            &already_added,
            move |id: LayoutId| {
                if let Some(app) = weak.upgrade() {
                    app.edit_sources(&|sources| add_source(sources, SourceEntry::xkb(id.as_str())));
                }
            },
        )
    }

    /// Opens the capture dialog for one shortcut. Returned so the window test can operate it.
    pub fn open_capture(app: &Rc<Self>, slot: ShortcutSlot) -> CaptureDialog {
        let weak = Rc::downgrade(app);
        let can_clear = slot != ShortcutSlot::Next;

        CaptureDialog::present(
            &app.window,
            app.language,
            can_clear,
            move |dialog, result| {
                weak.upgrade()
                    .is_none_or(|app| app.apply_capture(dialog, &slot, result))
            },
        )
    }

    /// Applies a chosen shortcut, refusing one that another action already uses.
    fn apply_capture(
        self: &Rc<Self>,
        dialog: &CaptureDialog,
        slot: &ShortcutSlot,
        result: CaptureResult,
    ) -> bool {
        let mut candidate = self.config();

        match (slot, result) {
            (ShortcutSlot::Next, CaptureResult::Set(chord)) => candidate.switching.next = chord,
            (ShortcutSlot::Next, CaptureResult::Clear) => return false,
            (ShortcutSlot::Previous, CaptureResult::Set(chord)) => {
                candidate.switching.previous = Some(chord);
            }
            (ShortcutSlot::Previous, CaptureResult::Clear) => candidate.switching.previous = None,
            (ShortcutSlot::Direct(id), CaptureResult::Set(chord)) => {
                candidate.switching.direct.insert(id.clone(), chord);
            }
            (ShortcutSlot::Direct(id), CaptureResult::Clear) => {
                candidate.switching.direct.remove(id);
            }
        }

        if candidate.validate().is_err() {
            dialog.show_refusal(self.language, Message::ShortcutInUse);

            return false;
        }

        self.change_config(|config| *config = candidate.clone());
        self.refresh_all();

        true
    }

    /// Shows which layout the desktop is using, once it is known.
    pub fn set_active_source(self: &Rc<Self>, active: Option<String>) {
        self.state.borrow_mut().active_source = active;
        self.refresh_all();
    }

    /// Follows the active layout. The D-Bus call happens away from this thread, so a stalled
    /// Shell cannot freeze the window; without the extension, no layout is marked in use.
    pub fn start_watching_desktop(self: &Rc<Self>) {
        let Ok(shell) = DesktopShell::connect() else {
            return;
        };

        let request_update = {
            let shell = shell.clone();

            move || {
                let shell = shell.clone();

                thread::spawn(move || {
                    let active = shell
                        .snapshot()
                        .ok()
                        .map(|snapshot| snapshot.active.to_string());

                    glib::idle_add_once(move || {
                        if let Some(app) = Self::running() {
                            app.set_active_source(active);
                        }
                    });
                });
            }
        };

        let on_change = request_update.clone();

        if shell.watch(move |_event| on_change(), || {}).is_ok() {
            request_update();
        }
    }
}
