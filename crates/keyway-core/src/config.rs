//! Module: keyway_core::config
//! Purpose: Typed, validated user configuration with Windows-like defaults.
//! Created: 2026-11-08
//! Architecture: The agent and settings window parse this from the file they read;
//! data/config.example.toml documents every option and is checked against the defaults.

use crate::alt_code::{AltCodeRules, NumLockPolicy};
use crate::code_pages::CodePagePair;
use crate::hotkey::{Chord, ShortcutAction, ShortcutBinding};
use crate::layout_id::LayoutId;
use crate::protocol::RouterConfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MAX_CONFIG_BYTES: usize = 64 * 1024;
const WRITTEN_FILE_HEADER: &str =
    "# Written by Keyway settings. All options: config.example.toml\n\n";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub switching: SwitchingConfig,
    pub alt_codes: AltCodesConfig,
    pub indicator: IndicatorConfig,
    pub keyboard: KeyboardConfig,
    pub devices: DevicesConfig,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SwitchOrder {
    #[default]
    List,
    Recent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SwitchingConfig {
    pub next: Chord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<Chord>,
    pub order: SwitchOrder,
    pub direct: BTreeMap<LayoutId, Chord>,
}

impl SwitchingConfig {
    /// Whether next and previous reach a layout. One with its own shortcut is reached by that
    /// shortcut alone.
    pub fn cycles_through(&self, layout: &LayoutId) -> bool {
        !self.direct.contains_key(layout)
    }
}

impl Default for SwitchingConfig {
    fn default() -> Self {
        Self {
            next: Chord::ALT_SHIFT,
            previous: None,
            order: SwitchOrder::default(),
            direct: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AltCodesConfig {
    pub enabled: bool,
    pub numlock: NumLockPolicy,
    pub hex_numpad: bool,
    pub unicode_above_255: bool,
    pub code_pages: BTreeMap<LayoutId, CodePagePair>,
}

impl Default for AltCodesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            numlock: NumLockPolicy::default(),
            hex_numpad: false,
            unicode_above_255: false,
            code_pages: BTreeMap::new(),
        }
    }
}

/// How the taskbar shows the active layout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IndicatorConfig {
    pub flags: bool,
}

impl Default for IndicatorConfig {
    fn default() -> Self {
        Self { flags: true }
    }
}

/// The on-screen keyboard's colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardTheme {
    #[default]
    Light,
    Dark,
}

impl KeyboardTheme {
    pub const ALL: [Self; 2] = [Self::Light, Self::Dark];

    /// The name the configuration and the desktop extensions know this theme by.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.name() == name)
    }
}

/// How the on-screen keyboard looks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeyboardConfig {
    pub theme: KeyboardTheme,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DevicesConfig {
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    TooLarge(usize),
    Syntax(String),
    DuplicateShortcut(Chord),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge(size) => {
                write!(
                    formatter,
                    "configuration of {size} bytes exceeds {MAX_CONFIG_BYTES}"
                )
            }
            Self::Syntax(message) => write!(formatter, "invalid configuration: {message}"),
            Self::DuplicateShortcut(chord) => {
                write!(formatter, "shortcut {chord} is assigned more than once")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Parses and validates configuration text. Unknown options and shortcuts assigned
    /// twice are errors, so a mistake is never silently ignored.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        if text.len() > MAX_CONFIG_BYTES {
            return Err(ConfigError::TooLarge(text.len()));
        }

        let config: Self =
            toml::from_str(text).map_err(|error| ConfigError::Syntax(error.to_string()))?;

        config.validate()?;

        Ok(config)
    }

    /// Checks what the type system cannot: no shortcut may be assigned twice.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut seen_chords = BTreeSet::new();

        for binding in self.shortcut_bindings() {
            if !seen_chords.insert(binding.chord) {
                return Err(ConfigError::DuplicateShortcut(binding.chord));
            }
        }

        Ok(())
    }

    /// Renders the file the settings window saves; parsing it gives this configuration back.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        let body = toml::to_string(self).map_err(|error| ConfigError::Syntax(error.to_string()))?;

        Ok(format!("{WRITTEN_FILE_HEADER}{body}"))
    }

    pub fn shortcut_bindings(&self) -> Vec<ShortcutBinding> {
        let next = ShortcutBinding {
            chord: self.switching.next,
            action: ShortcutAction::NextLayout,
        };
        let previous = self.switching.previous.map(|chord| ShortcutBinding {
            chord,
            action: ShortcutAction::PreviousLayout,
        });
        let direct = self
            .switching
            .direct
            .iter()
            .map(|(layout, chord)| ShortcutBinding {
                chord: *chord,
                action: ShortcutAction::SelectLayout(layout.clone()),
            });

        std::iter::once(next)
            .chain(previous)
            .chain(direct)
            .collect()
    }

    /// Builds what the router needs; the agent supplies whether the active layout
    /// leaves Right Alt free.
    pub fn router_config(&self, allow_right_alt: bool) -> RouterConfig {
        RouterConfig {
            shortcuts: self.shortcut_bindings(),
            alt_codes: AltCodeRules {
                enabled: self.alt_codes.enabled,
                numlock: self.alt_codes.numlock,
                hex_numpad: self.alt_codes.hex_numpad,
                allow_right_alt,
            },
            excluded_devices: self.devices.exclude.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE_CONFIG: &str = include_str!("../../../data/config.example.toml");

    #[test]
    fn the_example_file_documents_the_real_defaults() {
        assert_eq!(Config::parse(EXAMPLE_CONFIG).unwrap(), Config::default());
    }

    #[test]
    fn an_empty_file_uses_the_defaults() {
        let config = Config::parse("").unwrap();

        assert_eq!(config, Config::default());
        assert_eq!(config.switching.next, Chord::ALT_SHIFT);
        assert!(config.alt_codes.enabled);
    }

    #[test]
    fn options_are_read_from_their_sections() {
        let text = r#"
            [switching]
            next = "Ctrl+Shift"
            previous = "Super+Space"
            order = "recent"

            [switching.direct]
            "gr+simple" = "Ctrl+Shift+2"

            [alt_codes]
            numlock = "required"
            hex_numpad = true

            [alt_codes.code_pages]
            br = { oem = 860, ansi = 1252 }
        "#;
        let config = Config::parse(text).unwrap();
        let router = config.router_config(true);

        assert_eq!(config.switching.order, SwitchOrder::Recent);
        assert_eq!(config.alt_codes.code_pages.len(), 1);
        assert_eq!(router.shortcuts.len(), 3);
        assert_eq!(router.alt_codes.numlock, NumLockPolicy::Required);
        assert!(router.alt_codes.hex_numpad);
        assert!(router.alt_codes.allow_right_alt);
    }

    #[test]
    fn mistakes_are_errors() {
        let is_syntax_error = |text| matches!(Config::parse(text), Err(ConfigError::Syntax(_)));

        assert!(is_syntax_error("unknown_option = true"));
        assert!(is_syntax_error("[keyboard]\ntheme = \"sepia\""));
        assert!(is_syntax_error("[switching]\nnext = \"Alt\""));
        assert!(is_syntax_error(
            "[alt_codes.code_pages]\nbr = { oem = 932, ansi = 1252 }"
        ));
    }

    #[test]
    fn a_shortcut_cannot_be_assigned_twice() {
        let text = "[switching]\nnext = \"Alt+Shift\"\nprevious = \"Shift+Alt\"";

        assert_eq!(
            Config::parse(text),
            Err(ConfigError::DuplicateShortcut(Chord::ALT_SHIFT))
        );
    }

    #[test]
    fn saved_files_parse_back_to_the_same_configuration() {
        let text = r#"
            [switching]
            next = "Ctrl+Shift"
            previous = "Super+Space"
            order = "recent"

            [switching.direct]
            "gr+simple" = "Ctrl+Shift+2"

            [alt_codes]
            numlock = "required"
            hex_numpad = true

            [alt_codes.code_pages]
            br = { oem = 860, ansi = 1252 }

            [indicator]
            flags = false

            [keyboard]
            theme = "dark"

            [devices]
            exclude = ["Macro pad"]
        "#;
        let config = Config::parse(text).unwrap();
        let defaults = Config::default();

        assert_eq!(config.keyboard.theme, KeyboardTheme::Dark);
        assert_eq!(defaults.keyboard.theme, KeyboardTheme::Light);
        assert_eq!(Config::parse(&config.to_toml().unwrap()).unwrap(), config);
        assert_eq!(
            Config::parse(&defaults.to_toml().unwrap()).unwrap(),
            defaults
        );
    }

    #[test]
    fn oversized_files_are_rejected_before_parsing() {
        let oversized = "#".repeat(MAX_CONFIG_BYTES + 1);

        assert!(matches!(
            Config::parse(&oversized),
            Err(ConfigError::TooLarge(_))
        ));
    }
}
