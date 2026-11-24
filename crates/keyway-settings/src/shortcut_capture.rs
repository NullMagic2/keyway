//! Module: keyway_settings::shortcut_capture
//! Purpose: Turn the keys someone presses into the shortcut they meant.
//! Created: 2026-11-08
//! Architecture: Pure logic behind capture_dialog.rs, following the same chord rules the
//! router uses: modifier-only chords finish on release, keyed chords on press.

use keyway_core::hotkey::{Chord, ChordError};
use keyway_core::key_codes::{
    HeldModifiers, KeyAction, KeyCode, KeyEvent, Modifier, ModifierSet, NamedKey,
};

/// GDK reports X11 keycodes, which are evdev codes plus this offset.
const X11_KEYCODE_OFFSET: u16 = 8;

pub fn key_code_from_hardware(hardware_keycode: u16) -> KeyCode {
    KeyCode::new(hardware_keycode.saturating_sub(X11_KEYCODE_OFFSET))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureStep {
    /// Still waiting for the shortcut to be finished.
    Waiting,
    Finished(Chord),
    Cancelled,
    /// That combination cannot be a shortcut; the person can try another.
    Rejected(ChordError),
    /// Keyway has no name for that key.
    UnknownKey,
}

#[derive(Clone, Debug, Default)]
pub struct ShortcutCapture {
    held: HeldModifiers,
    peak: ModifierSet,
    key_pressed: bool,
}

impl ShortcutCapture {
    pub fn handle(&mut self, event: KeyEvent) -> CaptureStep {
        let is_modifier = Modifier::from_key(event.key).is_some();

        match event.action {
            KeyAction::Repeat => CaptureStep::Waiting,
            KeyAction::Press if is_modifier => {
                self.held.update(event);
                self.peak = self.peak.union(self.held.active());

                CaptureStep::Waiting
            }
            KeyAction::Press => self.handle_key_press(event.key),
            KeyAction::Release if is_modifier => self.handle_modifier_release(event),
            KeyAction::Release => CaptureStep::Waiting,
        }
    }

    fn handle_key_press(&mut self, key: KeyCode) -> CaptureStep {
        let active = self.held.active();
        self.key_pressed = !active.is_empty();

        if key == KeyCode::ESCAPE && active.is_empty() {
            return CaptureStep::Cancelled;
        }

        let Some(named_key) = NamedKey::from_code(key) else {
            return CaptureStep::UnknownKey;
        };

        Self::finish(Chord::from_parts(active, Some(named_key)))
    }

    /// A key pressed during the chord means the person meant a keyed shortcut, as on the
    /// router, so letting go of its modifiers afterwards must not become a shortcut itself.
    fn handle_modifier_release(&mut self, event: KeyEvent) -> CaptureStep {
        let peak = self.peak;
        let key_was_pressed = self.key_pressed;

        self.held.update(event);
        self.peak = self.held.active();

        if self.held.is_empty() {
            self.key_pressed = false;
        }

        if key_was_pressed || peak.len() < 2 {
            return CaptureStep::Waiting;
        }

        Self::finish(Chord::from_parts(peak, None))
    }

    fn finish(result: Result<Chord, ChordError>) -> CaptureStep {
        match result {
            Ok(chord) => CaptureStep::Finished(chord),
            Err(error) => CaptureStep::Rejected(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const X_KEY: KeyCode = KeyCode::new(45);
    const CAPS_LOCK_KEY: KeyCode = KeyCode::new(58);
    const UNNAMED_KEY: KeyCode = KeyCode::new(240);

    fn run(events: &[KeyEvent]) -> CaptureStep {
        let mut capture = ShortcutCapture::default();
        let mut last = CaptureStep::Waiting;

        for event in events {
            last = capture.handle(*event);

            if last != CaptureStep::Waiting {
                break;
            }
        }

        last
    }

    fn chord(text: &str) -> CaptureStep {
        CaptureStep::Finished(Chord::parse(text).unwrap())
    }

    #[test]
    fn modifier_only_chords_finish_on_the_first_release() {
        let steps = [
            KeyEvent::press(KeyCode::LEFT_ALT),
            KeyEvent::press(KeyCode::RIGHT_SHIFT),
            KeyEvent::release(KeyCode::RIGHT_SHIFT),
        ];

        assert_eq!(run(&steps), chord("Alt+Shift"));
    }

    #[test]
    fn one_modifier_alone_keeps_waiting() {
        let tapped = [
            KeyEvent::press(KeyCode::LEFT_ALT),
            KeyEvent::release(KeyCode::LEFT_ALT),
        ];
        let then_chord = [
            KeyEvent::press(KeyCode::LEFT_ALT),
            KeyEvent::release(KeyCode::LEFT_ALT),
            KeyEvent::press(KeyCode::LEFT_CTRL),
            KeyEvent::press(KeyCode::LEFT_SHIFT),
            KeyEvent::release(KeyCode::LEFT_CTRL),
        ];

        assert_eq!(run(&tapped), CaptureStep::Waiting);
        assert_eq!(run(&then_chord), chord("Ctrl+Shift"));
    }

    #[test]
    fn keyed_chords_finish_on_press() {
        let steps = [
            KeyEvent::press(KeyCode::LEFT_CTRL),
            KeyEvent::press(KeyCode::LEFT_SHIFT),
            KeyEvent::press(X_KEY),
        ];

        assert_eq!(run(&steps), chord("Ctrl+Shift+X"));
    }

    #[test]
    fn releasing_a_refused_keyed_chord_does_not_become_a_shortcut() {
        let mut capture = ShortcutCapture::default();
        capture.handle(KeyEvent::press(KeyCode::LEFT_CTRL));
        capture.handle(KeyEvent::press(KeyCode::LEFT_SHIFT));
        capture.handle(KeyEvent::press(UNNAMED_KEY));

        let shift_up = capture.handle(KeyEvent::release(KeyCode::LEFT_SHIFT));
        let ctrl_up = capture.handle(KeyEvent::release(KeyCode::LEFT_CTRL));

        assert_eq!(shift_up, CaptureStep::Waiting);
        assert_eq!(ctrl_up, CaptureStep::Waiting);

        capture.handle(KeyEvent::press(KeyCode::LEFT_ALT));
        capture.handle(KeyEvent::press(KeyCode::LEFT_SHIFT));

        assert_eq!(
            capture.handle(KeyEvent::release(KeyCode::LEFT_SHIFT)),
            chord("Alt+Shift")
        );
    }

    #[test]
    fn a_refused_key_alone_does_not_stop_the_next_chord() {
        let mut capture = ShortcutCapture::default();
        capture.handle(KeyEvent::press(X_KEY));
        capture.handle(KeyEvent::release(X_KEY));
        capture.handle(KeyEvent::press(KeyCode::LEFT_CTRL));
        capture.handle(KeyEvent::press(KeyCode::LEFT_SHIFT));

        assert_eq!(
            capture.handle(KeyEvent::release(KeyCode::LEFT_SHIFT)),
            chord("Ctrl+Shift")
        );
    }

    #[test]
    fn a_key_that_types_nothing_can_stand_alone() {
        assert_eq!(run(&[KeyEvent::press(CAPS_LOCK_KEY)]), chord("CapsLock"));
    }

    #[test]
    fn a_typing_key_alone_is_rejected() {
        assert!(matches!(
            run(&[KeyEvent::press(X_KEY)]),
            CaptureStep::Rejected(ChordError::TooFewModifiers(_))
        ));
    }

    #[test]
    fn escape_cancels_but_a_modified_escape_does_not() {
        let modified = [
            KeyEvent::press(KeyCode::LEFT_CTRL),
            KeyEvent::press(KeyCode::ESCAPE),
        ];

        assert_eq!(
            run(&[KeyEvent::press(KeyCode::ESCAPE)]),
            CaptureStep::Cancelled
        );
        assert_eq!(run(&modified), chord("Ctrl+Esc"));
    }

    #[test]
    fn keys_without_a_name_are_reported() {
        assert_eq!(
            run(&[KeyEvent::press(UNNAMED_KEY)]),
            CaptureStep::UnknownKey
        );
    }

    #[test]
    fn hardware_keycodes_convert_to_evdev_codes() {
        assert_eq!(key_code_from_hardware(64), KeyCode::LEFT_ALT);
        assert_eq!(key_code_from_hardware(0), KeyCode::new(0));
    }
}
