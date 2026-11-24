//! Module: keyway_router::key_router
//! Purpose: Turn device and agent events into ordered output, with no I/O of its own.
//! Created: 2026-11-08
//! Architecture: Wraps the key interpreter from keyway-core and adds what depends on
//! several devices: which keys are held, and releasing them when anything goes away.

use keyway_core::key_codes::{KeyAction, KeyCode, KeyDisposition, KeyEvent};
use keyway_core::key_interpreter::KeyInterpreter;
use keyway_core::protocol::{RouterConfig, RouterMessage};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceId(pub u64);

/// One thing the router must do. Outputs must be performed in the order returned, so
/// that an Alt code reaches the agent only after its Alt release reached the desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouterOutput {
    Emit(KeyEvent),
    Send(RouterMessage),
}

#[derive(Clone, Debug, Default)]
pub struct KeyRouter {
    interpreter: Option<KeyInterpreter>,
    numlock_on: bool,
    device_keys: BTreeMap<DeviceId, BTreeSet<KeyCode>>,
    emitted_down: BTreeSet<KeyCode>,
}

impl KeyRouter {
    pub const fn is_configured(&self) -> bool {
        self.interpreter.is_some()
    }

    pub fn configure(&mut self, config: RouterConfig) {
        self.interpreter = Some(KeyInterpreter::new(config));
    }

    pub fn set_allow_right_alt(&mut self, allow_right_alt: bool) {
        if let Some(interpreter) = &mut self.interpreter {
            interpreter.set_allow_right_alt(allow_right_alt);
        }
    }

    pub fn set_numlock(&mut self, numlock_on: bool) {
        self.numlock_on = numlock_on;
    }

    /// Handles a key from a grabbed device. Without an interpreter every key passes
    /// through unchanged, so a keyboard keeps working whatever state the agent is in.
    pub fn key_event(&mut self, device: DeviceId, event: KeyEvent) -> Vec<RouterOutput> {
        let held_on_device = self.device_keys.entry(device).or_default();

        match event.action {
            KeyAction::Press => {
                held_on_device.insert(event.key);
            }
            KeyAction::Release => {
                held_on_device.remove(&event.key);
            }
            KeyAction::Repeat => {}
        }

        let Some(interpreter) = &mut self.interpreter else {
            return vec![self.forward(event)];
        };

        let decision = interpreter.handle(event, self.numlock_on);
        let mut outputs = Vec::new();

        if decision.tap_mask_key {
            outputs.push(RouterOutput::Emit(KeyEvent::press(KeyCode::MASK_KEY)));
            outputs.push(RouterOutput::Emit(KeyEvent::release(KeyCode::MASK_KEY)));
        }

        if decision.disposition == KeyDisposition::Forward {
            outputs.push(self.forward(event));
        }

        if let Some(report) = decision.report {
            outputs.push(RouterOutput::Send(report));
        }

        outputs
    }

    /// Releases what a removed device was holding. Without this, unplugging a keyboard
    /// with a key down would leave that key stuck on the desktop.
    pub fn device_removed(&mut self, device: DeviceId) -> Vec<RouterOutput> {
        let held = self.device_keys.remove(&device).unwrap_or_default();
        let mut outputs = Vec::new();

        for key in held {
            outputs.extend(self.key_event(device, KeyEvent::release(key)));
        }

        self.device_keys.remove(&device);

        outputs
    }

    /// Forgets the agent and releases every key the desktop still sees as held.
    pub fn agent_lost(&mut self) -> Vec<RouterOutput> {
        self.interpreter = None;
        self.device_keys.clear();

        self.release_all()
    }

    pub fn release_all(&mut self) -> Vec<RouterOutput> {
        let held = std::mem::take(&mut self.emitted_down);

        held.into_iter()
            .map(|key| RouterOutput::Emit(KeyEvent::release(key)))
            .collect()
    }

    fn forward(&mut self, event: KeyEvent) -> RouterOutput {
        match event.action {
            KeyAction::Press => {
                self.emitted_down.insert(event.key);
            }
            KeyAction::Release => {
                self.emitted_down.remove(&event.key);
            }
            KeyAction::Repeat => {}
        }

        RouterOutput::Emit(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyway_core::alt_code::{AltCodeEntry, AltCodeRadix, AltCodeRules, NumLockPolicy};
    use keyway_core::hotkey::{Chord, ShortcutAction, ShortcutBinding};

    const KEYBOARD: DeviceId = DeviceId(1);
    const OTHER_KEYBOARD: DeviceId = DeviceId(2);

    fn configured_router() -> KeyRouter {
        let mut router = KeyRouter::default();
        router.configure(RouterConfig {
            shortcuts: vec![ShortcutBinding {
                chord: Chord::ALT_SHIFT,
                action: ShortcutAction::NextLayout,
            }],
            alt_codes: AltCodeRules {
                enabled: true,
                numlock: NumLockPolicy::Ignored,
                hex_numpad: false,
                allow_right_alt: false,
            },
            excluded_devices: Vec::new(),
        });

        router
    }

    fn emit(event: KeyEvent) -> RouterOutput {
        RouterOutput::Emit(event)
    }

    #[test]
    fn an_alt_code_masks_alt_and_reports_after_the_alt_release() {
        let mut router = configured_router();
        let expected_entry = AltCodeEntry::new(AltCodeRadix::Decimal, "1".to_string()).unwrap();

        assert_eq!(
            router.key_event(KEYBOARD, KeyEvent::press(KeyCode::LEFT_ALT)),
            [emit(KeyEvent::press(KeyCode::LEFT_ALT))]
        );
        assert_eq!(
            router.key_event(KEYBOARD, KeyEvent::press(KeyCode::KP1)),
            [
                emit(KeyEvent::press(KeyCode::MASK_KEY)),
                emit(KeyEvent::release(KeyCode::MASK_KEY))
            ]
        );
        assert_eq!(
            router.key_event(KEYBOARD, KeyEvent::release(KeyCode::KP1)),
            []
        );
        assert_eq!(
            router.key_event(KEYBOARD, KeyEvent::release(KeyCode::LEFT_ALT)),
            [
                emit(KeyEvent::release(KeyCode::LEFT_ALT)),
                RouterOutput::Send(RouterMessage::AltCode(expected_entry))
            ]
        );
    }

    #[test]
    fn a_shortcut_is_reported_after_its_modifier_is_forwarded() {
        let mut router = configured_router();
        router.key_event(KEYBOARD, KeyEvent::press(KeyCode::LEFT_ALT));
        router.key_event(KEYBOARD, KeyEvent::press(KeyCode::LEFT_SHIFT));

        assert_eq!(
            router.key_event(KEYBOARD, KeyEvent::release(KeyCode::LEFT_SHIFT)),
            [
                emit(KeyEvent::release(KeyCode::LEFT_SHIFT)),
                RouterOutput::Send(RouterMessage::Shortcut(ShortcutAction::NextLayout))
            ]
        );
    }

    #[test]
    fn without_a_configuration_every_key_passes_through() {
        let mut router = KeyRouter::default();
        let digit = KeyEvent::press(KeyCode::KP1);

        assert!(!router.is_configured());
        assert_eq!(router.key_event(KEYBOARD, digit), [emit(digit)]);
    }

    #[test]
    fn removing_a_device_releases_only_its_keys() {
        let mut router = configured_router();
        router.key_event(KEYBOARD, KeyEvent::press(KeyCode::A));
        router.key_event(OTHER_KEYBOARD, KeyEvent::press(KeyCode::B));

        assert_eq!(
            router.device_removed(KEYBOARD),
            [emit(KeyEvent::release(KeyCode::A))]
        );
        assert_eq!(router.device_removed(KEYBOARD), []);
    }

    #[test]
    fn removing_a_device_holding_alt_resets_the_interpreter() {
        let mut router = configured_router();
        router.key_event(KEYBOARD, KeyEvent::press(KeyCode::LEFT_ALT));
        router.device_removed(KEYBOARD);

        let digit = KeyEvent::press(KeyCode::KP1);

        assert_eq!(router.key_event(OTHER_KEYBOARD, digit), [emit(digit)]);
    }

    #[test]
    fn losing_the_agent_releases_forwarded_keys() {
        let mut router = configured_router();
        router.key_event(KEYBOARD, KeyEvent::press(KeyCode::A));
        router.key_event(OTHER_KEYBOARD, KeyEvent::press(KeyCode::B));

        let outputs = router.agent_lost();

        assert!(!router.is_configured());
        assert_eq!(
            outputs,
            [
                emit(KeyEvent::release(KeyCode::A)),
                emit(KeyEvent::release(KeyCode::B))
            ]
        );
        assert_eq!(router.release_all(), []);
    }

    #[test]
    fn released_keys_are_no_longer_tracked() {
        let mut router = configured_router();
        router.key_event(KEYBOARD, KeyEvent::press(KeyCode::A));
        router.key_event(KEYBOARD, KeyEvent::release(KeyCode::A));

        assert_eq!(router.release_all(), []);
    }

    #[test]
    fn repeats_of_forwarded_keys_are_forwarded() {
        let mut router = configured_router();
        let repeat = KeyEvent::repeat(KeyCode::A);
        router.key_event(KEYBOARD, KeyEvent::press(KeyCode::A));

        assert_eq!(router.key_event(KEYBOARD, repeat), [emit(repeat)]);
    }
}
