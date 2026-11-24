//! Module: keyway_core::key_interpreter
//! Purpose: Combine the shortcut and Alt code machines into one per-key decision.
//! Created: 2026-11-08
//! Architecture: The router's event loop calls this for every key and only performs
//! the input and output it asks for, keeping all key logic testable here.

use crate::alt_code::{AltCodeMachine, AltCodeStep};
use crate::hotkey::HotkeyMachine;
use crate::key_codes::{HeldModifiers, KeyAction, KeyDisposition, KeyEvent, Modifier};
use crate::protocol::{RouterConfig, RouterMessage};

/// What the router must do for one key event, in this order: tap the mask key if asked,
/// apply the disposition, then send the report. An Alt code is therefore only reported
/// after its Alt release has been forwarded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterpretedKey {
    pub tap_mask_key: bool,
    pub disposition: KeyDisposition,
    pub report: Option<RouterMessage>,
}

/// Decides what each key event means. The router uses it only while an agent is
/// connected; without one, every key passes through unchanged.
#[derive(Clone, Debug)]
pub struct KeyInterpreter {
    hotkeys: HotkeyMachine,
    alt_codes: AltCodeMachine,
    held: HeldModifiers,
    masked: bool,
}

impl KeyInterpreter {
    pub fn new(config: RouterConfig) -> Self {
        Self {
            hotkeys: HotkeyMachine::new(config.shortcuts),
            alt_codes: AltCodeMachine::new(config.alt_codes),
            held: HeldModifiers::default(),
            masked: false,
        }
    }

    pub fn set_allow_right_alt(&mut self, allow_right_alt: bool) {
        self.alt_codes.set_allow_right_alt(allow_right_alt);
    }

    pub fn handle(&mut self, event: KeyEvent, numlock_on: bool) -> InterpretedKey {
        self.held.update(event);

        let hotkey = self.hotkeys.handle(event);
        let is_press = event.action == KeyAction::Press;

        // A shortcut's key belongs to the shortcut, so it can never also be an Alt code.
        let alt_code = if hotkey.disposition == KeyDisposition::Swallow {
            if is_press {
                self.alt_codes.cancel();
            }

            AltCodeStep {
                disposition: KeyDisposition::Swallow,
                entry: None,
            }
        } else {
            self.alt_codes.handle(event, numlock_on)
        };

        let shortcut_report = hotkey.action.map(RouterMessage::Shortcut);
        let alt_code_report = alt_code.entry.map(RouterMessage::AltCode);
        let disposition = alt_code.disposition;

        InterpretedKey {
            tap_mask_key: self.update_mask(event, disposition),
            disposition,
            report: shortcut_report.or(alt_code_report),
        }
    }

    /// Swallowing every key pressed during an Alt or Super hold would leave applications
    /// seeing a bare tap, which opens menus or the overview. One mask tap per hold stops
    /// that; a forwarded key has the same effect, so it needs none.
    fn update_mask(&mut self, event: KeyEvent, disposition: KeyDisposition) -> bool {
        let is_key_press =
            event.action == KeyAction::Press && Modifier::from_key(event.key).is_none();
        let active = self.held.active();
        let tap_opens_something = active.contains(Modifier::LeftAlt)
            || active.contains(Modifier::RightAlt)
            || active.contains(Modifier::Super);

        let tap_mask_key = is_key_press
            && disposition == KeyDisposition::Swallow
            && tap_opens_something
            && !self.masked;

        if is_key_press {
            self.masked = true;
        }

        if self.held.is_empty() {
            self.masked = false;
        }

        tap_mask_key
    }
}
