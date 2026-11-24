//! Module: keyway_settings::input_sources
//! Purpose: Read and write the desktop's layout list and its own shortcuts in GSettings.
//! Created: 2026-11-08
//! Architecture: These belong to the desktop, whose indicator and the agent both follow them,
//! so writing here is all that adding or removing a layout takes. DesktopKeys names where each
//! desktop keeps them; this is the same list its own keyboard settings panel edits.

use crate::source_list::{SourceEntry, sources_from_xkb_query};
use crate::system_shortcuts::{without_modifier_only, without_xkb_switching};
use gtk::gio;
use gtk::gio::prelude::*;
use gtk::glib::ToVariant;
use std::env;
use std::path::Path;
use std::process::Command;

const CURRENT_DESKTOP_VARIABLE: &str = "XDG_CURRENT_DESKTOP";
const CURRENT_DESKTOP_SEPARATOR: char = ':';
const CINNAMON_DESKTOP_NAME: &str = "X-Cinnamon";

/// Only Cinnamon 6.6 and later have this module. Their schema alone does not tell, because
/// Cinnamon 6.4 already ships the schema without anything reading it.
const CINNAMON_INPUT_SOURCE_MANAGER: &str = "/usr/share/cinnamon/js/ui/keyboardManager.js";
const XKB_QUERY_COMMAND: &str = "setxkbmap";
const XKB_QUERY_ARGUMENT: &str = "-query";

/// Cinnamon's panel opens on its typing options otherwise, and ignores a tab it does not have.
/// The form --tab=layouts crashes the panel in Cinnamon 6.4, though its own help shows it.
const CINNAMON_LAYOUTS_TAB: &str = "-t layouts";

/// An X server keyboard map has at most this many groups.
const XKB_GROUP_LIMIT: usize = 4;
const SWITCH_BINDING_KEYS: [&str; 2] = ["switch-input-source", "switch-input-source-backward"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayoutFormat {
    /// Pairs of kind and id, such as ("xkb", "gr+simple").
    InputSources,
    /// libgnomekbd strings, such as "gr\tsimple", where an empty list means the X server's.
    XkbGroups,
}

/// Where one desktop keeps its keyboard settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DesktopKeys {
    layout_schema: &'static str,
    layout_key: &'static str,
    layout_format: LayoutFormat,
    layout_limit: Option<usize>,
    options_key: &'static str,
    per_window_schema: &'static str,
    per_window_key: &'static str,
    switch_binding_schema: Option<&'static str>,
    keyboard_panel: &'static str,
    /// What makes that panel open on its layout list, if it does not already.
    keyboard_panel_tab: Option<&'static str>,
}

const GNOME_KEYS: DesktopKeys = DesktopKeys {
    layout_schema: "org.gnome.desktop.input-sources",
    layout_key: "sources",
    layout_format: LayoutFormat::InputSources,
    layout_limit: None,
    options_key: "xkb-options",
    per_window_schema: "org.gnome.desktop.input-sources",
    per_window_key: "per-window",
    switch_binding_schema: Some("org.gnome.desktop.wm.keybindings"),
    keyboard_panel: "gnome-keyboard-panel.desktop",
    keyboard_panel_tab: None,
};

const CINNAMON_INPUT_SOURCE_KEYS: DesktopKeys = DesktopKeys {
    layout_schema: "org.cinnamon.desktop.input-sources",
    layout_key: "sources",
    layout_format: LayoutFormat::InputSources,
    layout_limit: None,
    options_key: "xkb-options",
    per_window_schema: "org.cinnamon.desktop.input-sources",
    per_window_key: "per-window",
    switch_binding_schema: Some("org.cinnamon.desktop.keybindings.wm"),
    keyboard_panel: "cinnamon-settings-keyboard.desktop",
    keyboard_panel_tab: Some(CINNAMON_LAYOUTS_TAB),
};

const CINNAMON_XKB_GROUP_KEYS: DesktopKeys = DesktopKeys {
    layout_schema: "org.gnome.libgnomekbd.keyboard",
    layout_key: "layouts",
    layout_format: LayoutFormat::XkbGroups,
    layout_limit: Some(XKB_GROUP_LIMIT),
    options_key: "options",
    per_window_schema: "org.gnome.libgnomekbd.desktop",
    per_window_key: "group-per-window",
    switch_binding_schema: None,
    keyboard_panel: "cinnamon-settings-keyboard.desktop",
    keyboard_panel_tab: Some(CINNAMON_LAYOUTS_TAB),
};

/// Chooses the keys from XDG_CURRENT_DESKTOP, a list such as ubuntu:GNOME. Every desktop other
/// than Cinnamon gets GNOME's, which the package's dependencies always provide.
fn keys_for_desktop(current_desktop: &str, has_input_source_manager: bool) -> DesktopKeys {
    let is_cinnamon = current_desktop
        .split(CURRENT_DESKTOP_SEPARATOR)
        .any(|name| name == CINNAMON_DESKTOP_NAME);

    match (is_cinnamon, has_input_source_manager) {
        (false, _) => GNOME_KEYS,
        (true, true) => CINNAMON_INPUT_SOURCE_KEYS,
        (true, false) => CINNAMON_XKB_GROUP_KEYS,
    }
}

/// Opens a schema that may not be installed. A plain gio::Settings aborts the process then.
fn optional_settings(schema: &str) -> Option<gio::Settings> {
    gio::SettingsSchemaSource::default()?
        .lookup(schema, true)
        .map(|_schema| gio::Settings::new(schema))
}

#[derive(Clone, Debug)]
pub struct InputSourceSettings {
    keys: DesktopKeys,
    layouts: gio::Settings,
    per_window: gio::Settings,
    switch_bindings: Option<gio::Settings>,
}

impl Default for InputSourceSettings {
    fn default() -> Self {
        let current_desktop = env::var(CURRENT_DESKTOP_VARIABLE).unwrap_or_default();
        let has_manager = Path::new(CINNAMON_INPUT_SOURCE_MANAGER).exists();
        let keys = keys_for_desktop(&current_desktop, has_manager);

        Self {
            keys,
            layouts: gio::Settings::new(keys.layout_schema),
            per_window: gio::Settings::new(keys.per_window_schema),
            switch_bindings: keys.switch_binding_schema.and_then(optional_settings),
        }
    }
}

impl InputSourceSettings {
    pub fn sources(&self) -> Vec<SourceEntry> {
        match self.keys.layout_format {
            LayoutFormat::InputSources => {
                let raw: Vec<(String, String)> = self
                    .layouts
                    .value(self.keys.layout_key)
                    .get()
                    .unwrap_or_default();

                raw.into_iter()
                    .map(|(kind, id)| SourceEntry { kind, id })
                    .collect()
            }
            LayoutFormat::XkbGroups => {
                let raw = self.layouts.strv(self.keys.layout_key);

                if raw.is_empty() {
                    return server_layouts();
                }

                raw.iter()
                    .map(|layout| SourceEntry::from_group_layout(layout))
                    .collect()
            }
        }
    }

    pub fn set_sources(&self, sources: &[SourceEntry]) -> Result<(), String> {
        let written = match self.keys.layout_format {
            LayoutFormat::InputSources => {
                let raw: Vec<(String, String)> = sources
                    .iter()
                    .map(|entry| (entry.kind.clone(), entry.id.clone()))
                    .collect();

                self.layouts
                    .set_value(self.keys.layout_key, &raw.to_variant())
            }
            LayoutFormat::XkbGroups => {
                let raw: Vec<String> = sources.iter().map(SourceEntry::to_group_layout).collect();

                self.layouts
                    .set_value(self.keys.layout_key, &raw.to_variant())
            }
        };

        written.map_err(|error| error.to_string())
    }

    /// How many layouts this desktop can hold, if it has a limit.
    pub fn layout_limit(&self) -> Option<usize> {
        self.keys.layout_limit
    }

    pub fn per_window(&self) -> bool {
        self.per_window.boolean(self.keys.per_window_key)
    }

    pub fn set_per_window(&self, enabled: bool) -> Result<(), String> {
        self.per_window
            .set_boolean(self.keys.per_window_key, enabled)
            .map_err(|error| error.to_string())
    }

    /// Calls changed whenever the list changes, including from other programs.
    pub fn connect_sources_changed(&self, changed: impl Fn() + 'static) {
        self.layouts
            .connect_changed(Some(self.keys.layout_key), move |_settings, _key| changed());
    }

    fn xkb_options(&self) -> Vec<String> {
        strings(&self.layouts.strv(self.keys.options_key))
    }

    fn switch_bindings_of(settings: &gio::Settings, key: &str) -> Vec<String> {
        strings(&settings.strv(key))
    }

    /// True when the desktop itself switches layouts on modifier keys alone.
    pub fn has_modifier_only_switching(&self) -> bool {
        let options = self.xkb_options();

        if without_xkb_switching(&options) != options {
            return true;
        }

        self.switch_bindings.as_ref().is_some_and(|settings| {
            SWITCH_BINDING_KEYS.iter().any(|key| {
                let bindings = Self::switch_bindings_of(settings, key);

                without_modifier_only(&bindings) != bindings
            })
        })
    }

    /// Removes only those shortcuts, keeping every other option and binding.
    pub fn remove_modifier_only_switching(&self) -> Result<(), String> {
        let options = without_xkb_switching(&self.xkb_options());

        self.layouts
            .set_value(self.keys.options_key, &options.to_variant())
            .map_err(|error| error.to_string())?;

        let Some(settings) = &self.switch_bindings else {
            return Ok(());
        };

        for key in SWITCH_BINDING_KEYS {
            let bindings = without_modifier_only(&Self::switch_bindings_of(settings, key));

            settings
                .set_value(key, &bindings.to_variant())
                .map_err(|error| error.to_string())?;
        }

        Ok(())
    }

    /// Calls changed whenever one of those shortcuts may have changed.
    pub fn connect_system_shortcuts_changed(&self, changed: impl Fn() + Clone + 'static) {
        let on_options = changed.clone();

        self.layouts
            .connect_changed(Some(self.keys.options_key), move |_settings, _key| {
                on_options();
            });

        if let Some(settings) = &self.switch_bindings {
            for key in SWITCH_BINDING_KEYS {
                let changed = changed.clone();

                settings.connect_changed(Some(key), move |_settings, _key| changed());
            }
        }
    }

    /// True when the desktop's own keyboard settings panel is installed.
    pub fn has_keyboard_panel(&self) -> bool {
        gio::DesktopAppInfo::new(self.keys.keyboard_panel).is_some()
    }

    /// Opens that panel on the page with the layout list.
    pub fn open_keyboard_panel(&self) -> Result<(), String> {
        let panel = gio::DesktopAppInfo::new(self.keys.keyboard_panel)
            .ok_or_else(|| format!("{} is not installed", self.keys.keyboard_panel))?;

        let launched = match (self.keys.keyboard_panel_tab, panel.commandline()) {
            (Some(tab), Some(command)) => gio::AppInfo::create_from_commandline(
                format!("{} {tab}", command.display()),
                None,
                gio::AppInfoCreateFlags::NONE,
            )
            .and_then(|app| app.launch(&[], gio::AppLaunchContext::NONE)),
            _ => panel.launch(&[], gio::AppLaunchContext::NONE),
        };

        launched.map_err(|error| error.to_string())
    }
}

fn strings(values: &gtk::glib::StrV) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

/// The X server's layouts, for a list that is still empty. Empty if they cannot be read.
fn server_layouts() -> Vec<SourceEntry> {
    let Ok(output) = Command::new(XKB_QUERY_COMMAND)
        .arg(XKB_QUERY_ARGUMENT)
        .output()
    else {
        return Vec::new();
    };

    sources_from_xkb_query(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ubuntu_and_plain_gnome_use_the_same_keys() {
        assert_eq!(keys_for_desktop("ubuntu:GNOME", false), GNOME_KEYS);
        assert_eq!(keys_for_desktop("GNOME", false), GNOME_KEYS);
    }

    #[test]
    fn cinnamon_keys_follow_its_layout_system() {
        assert_eq!(
            keys_for_desktop("X-Cinnamon", false),
            CINNAMON_XKB_GROUP_KEYS
        );
        assert_eq!(
            keys_for_desktop("X-Cinnamon", true),
            CINNAMON_INPUT_SOURCE_KEYS
        );
    }

    #[test]
    fn an_unknown_desktop_gets_the_keys_that_are_always_installed() {
        assert_eq!(keys_for_desktop("", true), GNOME_KEYS);
    }
}
