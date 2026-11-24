//! Module: keyway_core::hotkey
//! Purpose: Parse shortcut chords and recognize them in a stream of key events.
//! Created: 2026-11-08
//! Architecture: Fed by the key interpreter; follows the Windows rule that
//! modifier-only chords fire on release and keyed chords fire on press.

use crate::key_codes::{
    HeldModifiers, KeyAction, KeyCode, KeyDisposition, KeyEvent, Modifier, ModifierSet, NamedKey,
};
use crate::layout_id::LayoutId;
use serde::{Deserialize, Serialize};
use std::fmt;

const CHORD_SEPARATOR: char = '+';
const MIN_MODIFIERS_WITHOUT_KEY: u32 = 2;

/// A shortcut: modifiers plus an optional final key.
///
/// A chord with no final key holds at least two modifiers, so a plain Alt or Shift tap can
/// never switch layouts. A key alone must be one that types nothing, such as CapsLock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Chord {
    modifiers: ModifierSet,
    key: Option<NamedKey>,
}

impl Chord {
    /// The default next-layout shortcut, Alt+Shift, matching the Windows default.
    pub const ALT_SHIFT: Self = Self {
        modifiers: ModifierSet::EMPTY
            .with(Modifier::LeftAlt)
            .with(Modifier::Shift),
        key: None,
    };

    /// True when the chord has no final key, so it fires on release.
    pub const fn is_modifier_only(self) -> bool {
        self.key.is_none()
    }

    /// Parses text such as Alt+Shift, Ctrl+Shift+1 or CapsLock, ignoring case.
    pub fn parse(text: &str) -> Result<Self, ChordError> {
        let mut modifiers = ModifierSet::EMPTY;
        let mut key = None;

        for part in text.split(CHORD_SEPARATOR).map(str::trim) {
            let modifier = Modifier::ALL
                .into_iter()
                .find(|m| m.name().eq_ignore_ascii_case(part));

            if let Some(modifier) = modifier {
                if modifiers.contains(modifier) {
                    return Err(ChordError::RepeatedModifier(part.to_string()));
                }

                modifiers = modifiers.with(modifier);
            } else if key.is_some() {
                return Err(ChordError::SeveralKeys(text.to_string()));
            } else {
                let named_key = NamedKey::from_name(part);
                key = Some(named_key.ok_or_else(|| ChordError::UnknownKey(part.to_string()))?);
            }
        }

        Self::from_parts(modifiers, key)
    }

    /// Builds a chord from what was pressed, enforcing the rules for a usable shortcut.
    pub fn from_parts(modifiers: ModifierSet, key: Option<NamedKey>) -> Result<Self, ChordError> {
        let chord = Self { modifiers, key };

        let has_enough_modifiers = match key {
            Some(named_key) => !modifiers.is_empty() || named_key.is_usable_alone(),
            None => modifiers.len() >= MIN_MODIFIERS_WITHOUT_KEY,
        };

        if !has_enough_modifiers {
            return Err(ChordError::TooFewModifiers(chord.to_string()));
        }

        Ok(chord)
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let held = Modifier::ALL
            .into_iter()
            .filter(|m| self.modifiers.contains(*m));
        let names: Vec<&str> = held
            .map(Modifier::name)
            .chain(self.key.map(|k| k.name))
            .collect();

        formatter.write_str(&names.join("+"))
    }
}

impl TryFrom<String> for Chord {
    type Error = ChordError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Chord> for String {
    fn from(chord: Chord) -> Self {
        chord.to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChordError {
    UnknownKey(String),
    RepeatedModifier(String),
    SeveralKeys(String),
    TooFewModifiers(String),
}

impl fmt::Display for ChordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownKey(name) => write!(formatter, "unknown key name: {name:?}"),
            Self::RepeatedModifier(name) => write!(formatter, "repeated modifier: {name}"),
            Self::SeveralKeys(chord) => write!(formatter, "more than one final key: {chord}"),
            Self::TooFewModifiers(chord) => {
                write!(formatter, "needs more modifiers to be usable: {chord}")
            }
        }
    }
}

impl std::error::Error for ChordError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutAction {
    NextLayout,
    PreviousLayout,
    SelectLayout(LayoutId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutBinding {
    pub chord: Chord,
    pub action: ShortcutAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotkeyStep {
    pub disposition: KeyDisposition,
    pub action: Option<ShortcutAction>,
}

impl HotkeyStep {
    const FORWARD: Self = Self {
        disposition: KeyDisposition::Forward,
        action: None,
    };
    const SWALLOW: Self = Self {
        disposition: KeyDisposition::Swallow,
        action: None,
    };
}

/// Recognizes shortcut chords. Modifier keys are always forwarded; only the final key
/// of a matched chord is swallowed.
#[derive(Clone, Debug)]
pub struct HotkeyMachine {
    bindings: Vec<ShortcutBinding>,
    held: HeldModifiers,
    peak_modifiers: ModifierSet,
    other_key_pressed: bool,
    swallowed_keys: Vec<KeyCode>,
}

impl HotkeyMachine {
    pub fn new(bindings: Vec<ShortcutBinding>) -> Self {
        Self {
            bindings,
            held: HeldModifiers::default(),
            peak_modifiers: ModifierSet::EMPTY,
            other_key_pressed: false,
            swallowed_keys: Vec::new(),
        }
    }

    pub fn handle(&mut self, event: KeyEvent) -> HotkeyStep {
        let is_modifier = Modifier::from_key(event.key).is_some();

        match event.action {
            KeyAction::Repeat if self.swallowed_keys.contains(&event.key) => HotkeyStep::SWALLOW,
            KeyAction::Repeat => HotkeyStep::FORWARD,
            KeyAction::Press if is_modifier => self.handle_modifier_press(event),
            KeyAction::Press => self.handle_key_press(event.key),
            KeyAction::Release if is_modifier => self.handle_modifier_release(event),
            KeyAction::Release => self.handle_key_release(event.key),
        }
    }

    fn handle_modifier_press(&mut self, event: KeyEvent) -> HotkeyStep {
        self.held.update(event);
        self.peak_modifiers = self.peak_modifiers.union(self.held.active());

        HotkeyStep::FORWARD
    }

    fn handle_key_press(&mut self, key: KeyCode) -> HotkeyStep {
        // Only a key pressed during a chord spoils it; ordinary typing before one does not.
        if !self.held.is_empty() {
            self.other_key_pressed = true;
        }

        let active = self.held.active();
        let matches = |binding: &&ShortcutBinding| {
            binding.chord.modifiers == active && binding.chord.key.map(|k| k.code) == Some(key)
        };

        let Some(binding) = self.bindings.iter().find(matches) else {
            return HotkeyStep::FORWARD;
        };

        let action = Some(binding.action.clone());
        self.swallowed_keys.push(key);

        HotkeyStep {
            disposition: KeyDisposition::Swallow,
            action,
        }
    }

    fn handle_modifier_release(&mut self, event: KeyEvent) -> HotkeyStep {
        let action = self.modifier_only_action();

        self.held.update(event);

        // After firing, the chord may fire again while its other modifier stays held.
        if action.is_some() {
            self.peak_modifiers = self.held.active();
        }

        if self.held.is_empty() {
            self.peak_modifiers = ModifierSet::EMPTY;
            self.other_key_pressed = false;
        }

        HotkeyStep {
            disposition: KeyDisposition::Forward,
            action,
        }
    }

    fn handle_key_release(&mut self, key: KeyCode) -> HotkeyStep {
        let was_swallowed = self.swallowed_keys.contains(&key);

        if !was_swallowed {
            return HotkeyStep::FORWARD;
        }

        self.swallowed_keys.retain(|swallowed| *swallowed != key);

        HotkeyStep::SWALLOW
    }

    fn modifier_only_action(&self) -> Option<ShortcutAction> {
        if self.other_key_pressed {
            return None;
        }

        let matches = |binding: &&ShortcutBinding| {
            binding.chord.key.is_none() && binding.chord.modifiers == self.peak_modifiers
        };

        self.bindings
            .iter()
            .find(matches)
            .map(|binding| binding.action.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine_with(chord_text: &str) -> HotkeyMachine {
        let chord = Chord::parse(chord_text).unwrap();

        HotkeyMachine::new(vec![ShortcutBinding {
            chord,
            action: ShortcutAction::NextLayout,
        }])
    }

    #[test]
    fn parses_and_prints_chords_in_canonical_form() {
        assert_eq!(Chord::parse("shift+alt").unwrap().to_string(), "Alt+Shift");
        assert_eq!(
            Chord::parse("Ctrl+Shift+1").unwrap().to_string(),
            "Ctrl+Shift+1"
        );
        assert_eq!(Chord::parse("capslock").unwrap().to_string(), "CapsLock");
        assert_eq!(Chord::ALT_SHIFT, Chord::parse("Alt+Shift").unwrap());
        assert_eq!(Chord::parse("ctrl+a").unwrap().to_string(), "Ctrl+A");
    }

    #[test]
    fn rejects_chords_that_cannot_work() {
        assert!(matches!(
            Chord::parse("Alt"),
            Err(ChordError::TooFewModifiers(_))
        ));
        assert!(matches!(
            Chord::parse("Alt+Alt+A"),
            Err(ChordError::RepeatedModifier(_))
        ));
        assert!(matches!(
            Chord::parse("A+B"),
            Err(ChordError::SeveralKeys(_))
        ));
        assert!(matches!(
            Chord::parse("Alt+Nope"),
            Err(ChordError::UnknownKey(_))
        ));
    }

    #[test]
    fn modifier_only_chord_fires_on_first_release_only() {
        let mut machine = machine_with("Alt+Shift");

        assert_eq!(
            machine.handle(KeyEvent::press(KeyCode::LEFT_ALT)),
            HotkeyStep::FORWARD
        );
        assert_eq!(
            machine.handle(KeyEvent::press(KeyCode::LEFT_SHIFT)),
            HotkeyStep::FORWARD
        );

        let first_release = machine.handle(KeyEvent::release(KeyCode::LEFT_SHIFT));
        let second_release = machine.handle(KeyEvent::release(KeyCode::LEFT_ALT));

        assert_eq!(first_release.action, Some(ShortcutAction::NextLayout));
        assert_eq!(first_release.disposition, KeyDisposition::Forward);
        assert_eq!(second_release, HotkeyStep::FORWARD);
    }

    #[test]
    fn modifier_only_chord_fires_again_while_alt_stays_held() {
        let mut machine = machine_with("Alt+Shift");
        machine.handle(KeyEvent::press(KeyCode::LEFT_ALT));

        for _ in 0..2 {
            machine.handle(KeyEvent::press(KeyCode::RIGHT_SHIFT));
            let release = machine.handle(KeyEvent::release(KeyCode::RIGHT_SHIFT));

            assert_eq!(release.action, Some(ShortcutAction::NextLayout));
        }
    }

    #[test]
    fn another_key_during_the_chord_stops_it_firing() {
        let mut machine = machine_with("Alt+Shift");
        machine.handle(KeyEvent::press(KeyCode::LEFT_ALT));
        machine.handle(KeyEvent::press(KeyCode::LEFT_SHIFT));
        machine.handle(KeyEvent::press(KeyCode::F));
        machine.handle(KeyEvent::release(KeyCode::F));

        assert_eq!(
            machine
                .handle(KeyEvent::release(KeyCode::LEFT_SHIFT))
                .action,
            None
        );
        assert_eq!(
            machine.handle(KeyEvent::release(KeyCode::LEFT_ALT)).action,
            None
        );

        machine.handle(KeyEvent::press(KeyCode::LEFT_ALT));
        machine.handle(KeyEvent::press(KeyCode::LEFT_SHIFT));

        let next_chord = machine.handle(KeyEvent::release(KeyCode::LEFT_ALT));

        assert_eq!(next_chord.action, Some(ShortcutAction::NextLayout));
    }

    #[test]
    fn typing_before_the_chord_does_not_stop_it_firing() {
        let mut machine = machine_with("Alt+Shift");
        machine.handle(KeyEvent::press(KeyCode::F));
        machine.handle(KeyEvent::release(KeyCode::F));
        machine.handle(KeyEvent::press(KeyCode::LEFT_ALT));
        machine.handle(KeyEvent::press(KeyCode::LEFT_SHIFT));

        let release = machine.handle(KeyEvent::release(KeyCode::LEFT_SHIFT));

        assert_eq!(release.action, Some(ShortcutAction::NextLayout));
    }

    #[test]
    fn right_alt_does_not_count_as_alt() {
        let mut machine = machine_with("Alt+Shift");
        machine.handle(KeyEvent::press(KeyCode::RIGHT_ALT));
        machine.handle(KeyEvent::press(KeyCode::LEFT_SHIFT));

        assert_eq!(
            machine
                .handle(KeyEvent::release(KeyCode::LEFT_SHIFT))
                .action,
            None
        );
    }

    #[test]
    fn keyed_chord_fires_on_press_and_swallows_its_key() {
        let mut machine = machine_with("Ctrl+Shift+F");
        machine.handle(KeyEvent::press(KeyCode::LEFT_CTRL));
        machine.handle(KeyEvent::press(KeyCode::RIGHT_SHIFT));

        let press = machine.handle(KeyEvent::press(KeyCode::F));
        let repeat = machine.handle(KeyEvent::repeat(KeyCode::F));
        machine.handle(KeyEvent::release(KeyCode::RIGHT_SHIFT));
        let release = machine.handle(KeyEvent::release(KeyCode::F));

        assert_eq!(press.action, Some(ShortcutAction::NextLayout));
        assert_eq!(press.disposition, KeyDisposition::Swallow);
        assert_eq!(repeat, HotkeyStep::SWALLOW);
        assert_eq!(release, HotkeyStep::SWALLOW);
    }

    #[test]
    fn keyed_chord_needs_the_exact_modifiers() {
        let mut machine = machine_with("Ctrl+F");
        machine.handle(KeyEvent::press(KeyCode::LEFT_CTRL));
        machine.handle(KeyEvent::press(KeyCode::LEFT_SHIFT));

        assert_eq!(
            machine.handle(KeyEvent::press(KeyCode::F)),
            HotkeyStep::FORWARD
        );
    }
}
