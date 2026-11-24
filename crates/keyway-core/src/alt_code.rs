//! Module: keyway_core::alt_code
//! Purpose: Recognize Alt plus keypad sequences and decode them into characters.
//! Created: 2026-11-08
//! Architecture: The router runs the machine and sends finished entries to the agent,
//! which decodes them, so the router never needs code page tables.

use crate::code_pages::{CodePageCatalog, CodePagePair};
use crate::key_codes::{
    HeldModifiers, KeyAction, KeyCode, KeyDisposition, KeyEvent, Modifier, ModifierSet,
};
use serde::{Deserialize, Serialize};
use std::fmt;

const MAX_DIGITS: usize = 16;
const LEADING_ZERO: char = '0';
const CODE_PAGE_SIZE: u64 = 256;
const HIGHEST_CODE_PAGE_VALUE: u64 = CODE_PAGE_SIZE - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AltCodeRadix {
    Decimal,
    Hexadecimal,
}

impl AltCodeRadix {
    const fn base(self) -> u32 {
        match self {
            Self::Decimal => 10,
            Self::Hexadecimal => 16,
        }
    }
}

/// A completed sequence: one to MAX_DIGITS digits valid in its radix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawAltCodeEntry")]
pub struct AltCodeEntry {
    radix: AltCodeRadix,
    digits: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAltCodeEntry {
    radix: AltCodeRadix,
    digits: String,
}

impl TryFrom<RawAltCodeEntry> for AltCodeEntry {
    type Error = AltCodeEntryError;

    fn try_from(raw: RawAltCodeEntry) -> Result<Self, Self::Error> {
        Self::new(raw.radix, raw.digits)
    }
}

impl AltCodeEntry {
    pub fn new(radix: AltCodeRadix, digits: String) -> Result<Self, AltCodeEntryError> {
        let all_valid = digits.chars().all(|digit| digit.is_digit(radix.base()));

        if digits.is_empty() || digits.len() > MAX_DIGITS || !all_valid {
            return Err(AltCodeEntryError(digits));
        }

        Ok(Self { radix, digits })
    }

    /// Decodes the entry following the Windows rules. Returns None when the code
    /// names no character or a control character, so nothing is typed.
    pub fn decode(
        &self,
        pages: CodePagePair,
        unicode_above_255: bool,
        catalog: &CodePageCatalog,
    ) -> Option<char> {
        let value = u64::from_str_radix(&self.digits, self.radix.base()).ok()?;

        let character = match self.radix {
            AltCodeRadix::Hexadecimal => unicode_scalar(value),
            AltCodeRadix::Decimal if unicode_above_255 && value > HIGHEST_CODE_PAGE_VALUE => {
                unicode_scalar(value)
            }
            AltCodeRadix::Decimal => {
                let page = if self.digits.starts_with(LEADING_ZERO) {
                    pages.ansi
                } else {
                    pages.oem
                };
                let byte = u8::try_from(value % CODE_PAGE_SIZE).ok()?;

                catalog.table(page)?.decode(byte)
            }
        };

        character.filter(|decoded| !decoded.is_control())
    }
}

fn unicode_scalar(value: u64) -> Option<char> {
    char::from_u32(u32::try_from(value).ok()?)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AltCodeEntryError(String);

impl fmt::Display for AltCodeEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Alt code digits: {:?}", self.0)
    }
}

impl std::error::Error for AltCodeEntryError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NumLockPolicy {
    #[default]
    Ignored,
    Required,
}

/// The rules the router needs to recognize sequences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AltCodeRules {
    pub enabled: bool,
    pub numlock: NumLockPolicy,
    pub hex_numpad: bool,
    /// True only while the active layout has no AltGr level on Right Alt.
    pub allow_right_alt: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AltCodeStep {
    pub disposition: KeyDisposition,
    pub entry: Option<AltCodeEntry>,
}

impl AltCodeStep {
    const FORWARD: Self = Self {
        disposition: KeyDisposition::Forward,
        entry: None,
    };
    const SWALLOW: Self = Self {
        disposition: KeyDisposition::Swallow,
        entry: None,
    };
}

#[derive(Clone, Debug)]
struct Sequence {
    alt_key: KeyCode,
    radix: AltCodeRadix,
    digits: String,
}

impl Sequence {
    fn digit_for(&self, key: KeyCode) -> Option<char> {
        keypad_digit(key).or(match self.radix {
            AltCodeRadix::Decimal => None,
            AltCodeRadix::Hexadecimal => hex_letter(key),
        })
    }
}

const fn keypad_digit(key: KeyCode) -> Option<char> {
    match key {
        KeyCode::KP0 => Some('0'),
        KeyCode::KP1 => Some('1'),
        KeyCode::KP2 => Some('2'),
        KeyCode::KP3 => Some('3'),
        KeyCode::KP4 => Some('4'),
        KeyCode::KP5 => Some('5'),
        KeyCode::KP6 => Some('6'),
        KeyCode::KP7 => Some('7'),
        KeyCode::KP8 => Some('8'),
        KeyCode::KP9 => Some('9'),
        _ => None,
    }
}

/// Hex letters are read by key position, so they sit at their QWERTY places on every
/// layout. The router has no keymap to do better.
const fn hex_letter(key: KeyCode) -> Option<char> {
    match key {
        KeyCode::A => Some('A'),
        KeyCode::B => Some('B'),
        KeyCode::C => Some('C'),
        KeyCode::D => Some('D'),
        KeyCode::E => Some('E'),
        KeyCode::F => Some('F'),
        _ => None,
    }
}

/// Recognizes sequences. Digits are held only until the sequence ends and are
/// reported once, after the Alt release that completes it.
#[derive(Clone, Debug)]
pub struct AltCodeMachine {
    rules: AltCodeRules,
    held: HeldModifiers,
    sequence: Option<Sequence>,
    swallowed_keys: Vec<KeyCode>,
}

impl AltCodeMachine {
    pub fn new(rules: AltCodeRules) -> Self {
        Self {
            rules,
            held: HeldModifiers::default(),
            sequence: None,
            swallowed_keys: Vec::new(),
        }
    }

    pub fn set_allow_right_alt(&mut self, allow_right_alt: bool) {
        self.rules.allow_right_alt = allow_right_alt;
    }

    /// Abandons any sequence in progress and discards its digits.
    pub fn cancel(&mut self) {
        self.sequence = None;
    }

    pub fn handle(&mut self, event: KeyEvent, numlock_on: bool) -> AltCodeStep {
        match event.action {
            KeyAction::Repeat if self.swallowed_keys.contains(&event.key) => AltCodeStep::SWALLOW,
            KeyAction::Repeat => AltCodeStep::FORWARD,
            KeyAction::Press => self.handle_press(event, numlock_on),
            KeyAction::Release => self.handle_release(event),
        }
    }

    fn handle_press(&mut self, event: KeyEvent, numlock_on: bool) -> AltCodeStep {
        let key = event.key;
        self.held.update(event);

        let continued = match &mut self.sequence {
            Some(sequence) => match sequence.digit_for(key) {
                Some(digit) => {
                    if sequence.digits.len() < MAX_DIGITS {
                        sequence.digits.push(digit);
                    }

                    true
                }
                None => false,
            },
            None => self.try_start_sequence(key, numlock_on),
        };

        if !continued {
            self.cancel();

            return AltCodeStep::FORWARD;
        }

        self.swallowed_keys.push(key);

        AltCodeStep::SWALLOW
    }

    fn try_start_sequence(&mut self, key: KeyCode, numlock_on: bool) -> bool {
        let Some(alt_key) = self.eligible_alt_key(numlock_on) else {
            return false;
        };

        let (radix, digits) = if let Some(digit) = keypad_digit(key) {
            (AltCodeRadix::Decimal, digit.to_string())
        } else if key == KeyCode::KP_PLUS && self.rules.hex_numpad {
            (AltCodeRadix::Hexadecimal, String::new())
        } else {
            return false;
        };

        self.sequence = Some(Sequence {
            alt_key,
            radix,
            digits,
        });

        true
    }

    /// Returns the Alt key that may own a new sequence: it must be the only modifier
    /// down, so Ctrl+Alt and AltGr combinations stay untouched.
    fn eligible_alt_key(&self, numlock_on: bool) -> Option<KeyCode> {
        let numlock_satisfied = self.rules.numlock == NumLockPolicy::Ignored || numlock_on;

        if !self.rules.enabled || !numlock_satisfied {
            return None;
        }

        let active = self.held.active();

        if active == ModifierSet::EMPTY.with(Modifier::LeftAlt) {
            Some(KeyCode::LEFT_ALT)
        } else if self.rules.allow_right_alt
            && active == ModifierSet::EMPTY.with(Modifier::RightAlt)
        {
            Some(KeyCode::RIGHT_ALT)
        } else {
            None
        }
    }

    fn handle_release(&mut self, event: KeyEvent) -> AltCodeStep {
        self.held.update(event);

        if self.swallowed_keys.contains(&event.key) {
            self.swallowed_keys
                .retain(|swallowed| *swallowed != event.key);

            return AltCodeStep::SWALLOW;
        }

        let completes_sequence = self
            .sequence
            .as_ref()
            .is_some_and(|s| s.alt_key == event.key);

        if !completes_sequence {
            return AltCodeStep::FORWARD;
        }

        let entry = self
            .sequence
            .take()
            .and_then(|s| AltCodeEntry::new(s.radix, s.digits).ok());

        AltCodeStep {
            disposition: KeyDisposition::Forward,
            entry,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_pages::CodePageId;

    const DEFAULT_RULES: AltCodeRules = AltCodeRules {
        enabled: true,
        numlock: NumLockPolicy::Ignored,
        hex_numpad: false,
        allow_right_alt: false,
    };

    fn tap(machine: &mut AltCodeMachine, key: KeyCode) -> KeyDisposition {
        let press = machine.handle(KeyEvent::press(key), true);
        machine.handle(KeyEvent::release(key), true);

        press.disposition
    }

    fn entry(radix: AltCodeRadix, digits: &str) -> AltCodeEntry {
        AltCodeEntry::new(radix, digits.to_string()).unwrap()
    }

    fn latin_pages() -> CodePagePair {
        CodePagePair {
            oem: CodePageId::try_from(850).unwrap(),
            ansi: CodePageId::try_from(1252).unwrap(),
        }
    }

    fn decode(entry: &AltCodeEntry, unicode_above_255: bool) -> Option<char> {
        let catalog = CodePageCatalog::load().unwrap();

        entry.decode(latin_pages(), unicode_above_255, &catalog)
    }

    #[test]
    fn digits_are_swallowed_and_reported_when_alt_is_released() {
        let mut machine = AltCodeMachine::new(DEFAULT_RULES);
        let alt_press = machine.handle(KeyEvent::press(KeyCode::LEFT_ALT), true);

        assert_eq!(alt_press, AltCodeStep::FORWARD);
        assert_eq!(tap(&mut machine, KeyCode::KP1), KeyDisposition::Swallow);
        assert_eq!(tap(&mut machine, KeyCode::KP3), KeyDisposition::Swallow);
        assert_eq!(tap(&mut machine, KeyCode::KP0), KeyDisposition::Swallow);

        let alt_release = machine.handle(KeyEvent::release(KeyCode::LEFT_ALT), true);

        assert_eq!(alt_release.disposition, KeyDisposition::Forward);
        assert_eq!(alt_release.entry, Some(entry(AltCodeRadix::Decimal, "130")));
    }

    #[test]
    fn another_key_cancels_the_sequence_and_is_forwarded() {
        let mut machine = AltCodeMachine::new(DEFAULT_RULES);
        machine.handle(KeyEvent::press(KeyCode::LEFT_ALT), true);
        tap(&mut machine, KeyCode::KP1);

        assert_eq!(tap(&mut machine, KeyCode::A), KeyDisposition::Forward);
        assert_eq!(
            machine
                .handle(KeyEvent::release(KeyCode::LEFT_ALT), true)
                .entry,
            None
        );
    }

    #[test]
    fn other_modifiers_leave_the_keypad_alone() {
        for modifier in [KeyCode::LEFT_CTRL, KeyCode::LEFT_SHIFT, KeyCode::LEFT_META] {
            let mut machine = AltCodeMachine::new(DEFAULT_RULES);
            machine.handle(KeyEvent::press(KeyCode::LEFT_ALT), true);
            machine.handle(KeyEvent::press(modifier), true);

            assert_eq!(tap(&mut machine, KeyCode::KP1), KeyDisposition::Forward);
        }
    }

    #[test]
    fn right_alt_needs_the_layout_to_have_no_altgr() {
        let mut machine = AltCodeMachine::new(DEFAULT_RULES);
        machine.handle(KeyEvent::press(KeyCode::RIGHT_ALT), true);

        assert_eq!(tap(&mut machine, KeyCode::KP1), KeyDisposition::Forward);

        machine.set_allow_right_alt(true);

        assert_eq!(tap(&mut machine, KeyCode::KP1), KeyDisposition::Swallow);
        assert!(
            machine
                .handle(KeyEvent::release(KeyCode::RIGHT_ALT), true)
                .entry
                .is_some()
        );
    }

    #[test]
    fn numlock_policy_decides_whether_a_sequence_starts() {
        let rules = AltCodeRules {
            numlock: NumLockPolicy::Required,
            ..DEFAULT_RULES
        };
        let mut machine = AltCodeMachine::new(rules);
        machine.handle(KeyEvent::press(KeyCode::LEFT_ALT), false);

        let numlock_off = machine.handle(KeyEvent::press(KeyCode::KP1), false);
        machine.handle(KeyEvent::release(KeyCode::KP1), false);
        let numlock_on = machine.handle(KeyEvent::press(KeyCode::KP1), true);

        assert_eq!(numlock_off, AltCodeStep::FORWARD);
        assert_eq!(numlock_on, AltCodeStep::SWALLOW);
    }

    #[test]
    fn disabled_rules_forward_everything() {
        let rules = AltCodeRules {
            enabled: false,
            ..DEFAULT_RULES
        };
        let mut machine = AltCodeMachine::new(rules);
        machine.handle(KeyEvent::press(KeyCode::LEFT_ALT), true);

        assert_eq!(tap(&mut machine, KeyCode::KP1), KeyDisposition::Forward);
    }

    #[test]
    fn hex_input_needs_the_option_and_accepts_letters() {
        let mut without_option = AltCodeMachine::new(DEFAULT_RULES);
        without_option.handle(KeyEvent::press(KeyCode::LEFT_ALT), true);

        assert_eq!(
            tap(&mut without_option, KeyCode::KP_PLUS),
            KeyDisposition::Forward
        );

        let rules = AltCodeRules {
            hex_numpad: true,
            ..DEFAULT_RULES
        };
        let mut machine = AltCodeMachine::new(rules);
        machine.handle(KeyEvent::press(KeyCode::LEFT_ALT), true);

        for key in [
            KeyCode::KP_PLUS,
            KeyCode::KP2,
            KeyCode::KP0,
            KeyCode::A,
            KeyCode::C,
        ] {
            assert_eq!(tap(&mut machine, key), KeyDisposition::Swallow);
        }

        let release = machine.handle(KeyEvent::release(KeyCode::LEFT_ALT), true);

        assert_eq!(
            release.entry,
            Some(entry(AltCodeRadix::Hexadecimal, "20AC"))
        );
    }

    #[test]
    fn a_digit_released_after_alt_stays_swallowed() {
        let mut machine = AltCodeMachine::new(DEFAULT_RULES);
        machine.handle(KeyEvent::press(KeyCode::LEFT_ALT), true);
        machine.handle(KeyEvent::press(KeyCode::KP1), true);
        machine.handle(KeyEvent::release(KeyCode::LEFT_ALT), true);

        assert_eq!(
            machine.handle(KeyEvent::release(KeyCode::KP1), true),
            AltCodeStep::SWALLOW
        );
    }

    #[test]
    fn decoding_follows_the_windows_rules() {
        assert_eq!(
            decode(&entry(AltCodeRadix::Decimal, "130"), false),
            Some('é')
        );
        assert_eq!(
            decode(&entry(AltCodeRadix::Decimal, "0128"), false),
            Some('€')
        );
        assert_eq!(
            decode(&entry(AltCodeRadix::Decimal, "321"), false),
            Some('A')
        );
        assert_eq!(decode(&entry(AltCodeRadix::Decimal, "1"), false), Some('☺'));
        assert_eq!(
            decode(&entry(AltCodeRadix::Hexadecimal, "20AC"), false),
            Some('€')
        );
    }

    #[test]
    fn codes_without_a_printable_character_type_nothing() {
        assert_eq!(decode(&entry(AltCodeRadix::Decimal, "0"), false), None);
        assert_eq!(decode(&entry(AltCodeRadix::Decimal, "256"), false), None);
        assert_eq!(decode(&entry(AltCodeRadix::Decimal, "0129"), false), None);
        assert_eq!(
            decode(&entry(AltCodeRadix::Hexadecimal, "D800"), false),
            None
        );
    }

    #[test]
    fn the_unicode_option_stops_values_above_255_wrapping() {
        assert_eq!(
            decode(&entry(AltCodeRadix::Decimal, "8364"), true),
            Some('€')
        );
        assert_eq!(
            decode(&entry(AltCodeRadix::Decimal, "130"), true),
            Some('é')
        );
    }

    #[test]
    fn invalid_entries_cannot_be_built_or_received() {
        let invalid = serde_json::from_str::<AltCodeEntry>(r#"{"radix":"decimal","digits":"1A"}"#);

        assert!(invalid.is_err());
        assert!(AltCodeEntry::new(AltCodeRadix::Decimal, String::new()).is_err());
    }
}
