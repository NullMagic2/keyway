//! Module: keyway_core::key_codes
//! Purpose: Typed evdev key codes, key events, modifiers and shortcut key names.
//! Created: 2026-11-08
//! Architecture: Shared vocabulary of the shortcut, Alt code and interpreter machines.
//! Values match linux/input-event-codes.h.

/// A physical key, identified by its evdev code rather than the character it types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyCode(u16);

impl KeyCode {
    pub const LEFT_CTRL: Self = Self(29);
    pub const RIGHT_CTRL: Self = Self(97);
    pub const LEFT_SHIFT: Self = Self(42);
    pub const RIGHT_SHIFT: Self = Self(54);
    pub const LEFT_ALT: Self = Self(56);
    pub const RIGHT_ALT: Self = Self(100);
    pub const LEFT_META: Self = Self(125);
    pub const RIGHT_META: Self = Self(126);

    pub const A: Self = Self(30);
    pub const B: Self = Self(48);
    pub const C: Self = Self(46);
    pub const D: Self = Self(32);
    pub const E: Self = Self(18);
    pub const F: Self = Self(33);

    pub const KP0: Self = Self(82);
    pub const KP1: Self = Self(79);
    pub const KP2: Self = Self(80);
    pub const KP3: Self = Self(81);
    pub const KP4: Self = Self(75);
    pub const KP5: Self = Self(76);
    pub const KP6: Self = Self(77);
    pub const KP7: Self = Self(71);
    pub const KP8: Self = Self(72);
    pub const KP9: Self = Self(73);
    pub const KP_PLUS: Self = Self(78);

    /// Tapped so applications do not see a bare Alt or Super press. The default
    /// keymaps assign this key no symbol, so nothing reacts to it.
    pub const MASK_KEY: Self = Self(194);

    pub const ESCAPE: Self = Self(1);

    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Press,
    Repeat,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: KeyCode,
    pub action: KeyAction,
}

impl KeyEvent {
    pub const fn press(key: KeyCode) -> Self {
        Self {
            key,
            action: KeyAction::Press,
        }
    }

    pub const fn repeat(key: KeyCode) -> Self {
        Self {
            key,
            action: KeyAction::Repeat,
        }
    }

    pub const fn release(key: KeyCode) -> Self {
        Self {
            key,
            action: KeyAction::Release,
        }
    }
}

/// What must happen to the physical key event that was just read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyDisposition {
    Forward,
    Swallow,
}

/// A logical modifier. Alt means Left Alt, as on Windows, because Right Alt is AltGr
/// on many layouts; both Ctrl, Shift and Super keys count as the same modifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    LeftAlt,
    RightAlt,
    Shift,
    Super,
}

impl Modifier {
    /// Listed in the order chords are written.
    pub const ALL: [Self; 5] = [
        Self::Ctrl,
        Self::LeftAlt,
        Self::RightAlt,
        Self::Shift,
        Self::Super,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::LeftAlt => "Alt",
            Self::RightAlt => "RightAlt",
            Self::Shift => "Shift",
            Self::Super => "Super",
        }
    }

    pub const fn from_key(key: KeyCode) -> Option<Self> {
        match key {
            KeyCode::LEFT_CTRL | KeyCode::RIGHT_CTRL => Some(Self::Ctrl),
            KeyCode::LEFT_ALT => Some(Self::LeftAlt),
            KeyCode::RIGHT_ALT => Some(Self::RightAlt),
            KeyCode::LEFT_SHIFT | KeyCode::RIGHT_SHIFT => Some(Self::Shift),
            KeyCode::LEFT_META | KeyCode::RIGHT_META => Some(Self::Super),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModifierSet(u8);

impl ModifierSet {
    pub const EMPTY: Self = Self(0);

    const fn bit(modifier: Modifier) -> u8 {
        1 << modifier as u8
    }

    pub const fn with(self, modifier: Modifier) -> Self {
        Self(self.0 | Self::bit(modifier))
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, modifier: Modifier) -> bool {
        self.0 & Self::bit(modifier) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }
}

/// Tracks which physical modifier keys are down.
#[derive(Clone, Debug, Default)]
pub struct HeldModifiers {
    keys: Vec<KeyCode>,
}

impl HeldModifiers {
    /// Applies an event; keys that are not modifiers and repeats change nothing.
    pub fn update(&mut self, event: KeyEvent) {
        if Modifier::from_key(event.key).is_none() {
            return;
        }

        match event.action {
            KeyAction::Press if !self.keys.contains(&event.key) => self.keys.push(event.key),
            KeyAction::Release => self.keys.retain(|key| *key != event.key),
            KeyAction::Press | KeyAction::Repeat => {}
        }
    }

    pub fn active(&self) -> ModifierSet {
        let modifiers = self.keys.iter().filter_map(|key| Modifier::from_key(*key));

        modifiers.fold(ModifierSet::EMPTY, ModifierSet::with)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// A key that may end a shortcut, with the name used for it in configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NamedKey {
    pub name: &'static str,
    pub code: KeyCode,
}

impl NamedKey {
    /// Finds a key by its configuration name, ignoring case.
    pub fn from_name(name: &str) -> Option<Self> {
        let is_match = |key: &&Self| key.name.eq_ignore_ascii_case(name);

        NAMED_KEYS.iter().find(is_match).copied()
    }

    /// Finds the key with this code, if it can be part of a shortcut.
    pub fn from_code(code: KeyCode) -> Option<Self> {
        NAMED_KEYS.iter().find(|key| key.code == code).copied()
    }

    /// True for keys that type nothing, so a shortcut can use one without modifiers.
    pub fn is_usable_alone(self) -> bool {
        KEYS_USABLE_ALONE.contains(&self.name)
    }
}

const fn named(name: &'static str, raw: u16) -> NamedKey {
    NamedKey {
        name,
        code: KeyCode(raw),
    }
}

const KEYS_USABLE_ALONE: &[&str] = &[
    "CapsLock",
    "NumLock",
    "ScrollLock",
    "Pause",
    "Menu",
    "Insert",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
];

const NAMED_KEYS: &[NamedKey] = &[
    named("Esc", 1),
    named("1", 2),
    named("2", 3),
    named("3", 4),
    named("4", 5),
    named("5", 6),
    named("6", 7),
    named("7", 8),
    named("8", 9),
    named("9", 10),
    named("0", 11),
    named("Minus", 12),
    named("Equal", 13),
    named("Backspace", 14),
    named("Tab", 15),
    named("Q", 16),
    named("W", 17),
    named("E", 18),
    named("R", 19),
    named("T", 20),
    named("Y", 21),
    named("U", 22),
    named("I", 23),
    named("O", 24),
    named("P", 25),
    named("LeftBracket", 26),
    named("RightBracket", 27),
    named("Enter", 28),
    named("A", 30),
    named("S", 31),
    named("D", 32),
    named("F", 33),
    named("G", 34),
    named("H", 35),
    named("J", 36),
    named("K", 37),
    named("L", 38),
    named("Semicolon", 39),
    named("Apostrophe", 40),
    named("Grave", 41),
    named("Backslash", 43),
    named("Z", 44),
    named("X", 45),
    named("C", 46),
    named("V", 47),
    named("B", 48),
    named("N", 49),
    named("M", 50),
    named("Comma", 51),
    named("Dot", 52),
    named("Slash", 53),
    named("Space", 57),
    named("CapsLock", 58),
    named("F1", 59),
    named("F2", 60),
    named("F3", 61),
    named("F4", 62),
    named("F5", 63),
    named("F6", 64),
    named("F7", 65),
    named("F8", 66),
    named("F9", 67),
    named("F10", 68),
    named("NumLock", 69),
    named("ScrollLock", 70),
    named("F11", 87),
    named("F12", 88),
    named("Home", 102),
    named("Up", 103),
    named("PageUp", 104),
    named("Left", 105),
    named("Right", 106),
    named("End", 107),
    named("Down", 108),
    named("PageDown", 109),
    named("Insert", 110),
    named("Delete", 111),
    named("Pause", 119),
    named("Menu", 127),
];
