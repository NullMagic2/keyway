//! Module: keyway_core::tests::windows_behavior
//! Purpose: Drive whole key sequences through the interpreter and decoder.
//! Created: 2026-11-08
//! Architecture: Expected values come from the design rules and published code pages.
//! They have not yet been confirmed on a real Windows machine.

use keyway_core::alt_code::AltCodeEntry;
use keyway_core::code_pages::{CodePageCatalog, LayoutCodePages};
use keyway_core::config::Config;
use keyway_core::hotkey::ShortcutAction;
use keyway_core::key_codes::{KeyCode, KeyDisposition, KeyEvent};
use keyway_core::key_interpreter::{InterpretedKey, KeyInterpreter};
use keyway_core::layout_id::LayoutId;
use keyway_core::protocol::RouterMessage;
use std::collections::BTreeMap;

const NUMLOCK_ON: bool = true;
const TAB_KEY: KeyCode = KeyCode::new(15);
const Z_KEY: KeyCode = KeyCode::new(44);
const DIGIT_ONE_KEY: KeyCode = KeyCode::new(2);

fn interpreter(config_text: &str) -> KeyInterpreter {
    let config = Config::parse(config_text).unwrap();

    KeyInterpreter::new(config.router_config(false))
}

fn press(machine: &mut KeyInterpreter, key: KeyCode) -> InterpretedKey {
    machine.handle(KeyEvent::press(key), NUMLOCK_ON)
}

fn release(machine: &mut KeyInterpreter, key: KeyCode) -> InterpretedKey {
    machine.handle(KeyEvent::release(key), NUMLOCK_ON)
}

/// Holds Left Alt, taps the keypad digits and returns the entry reported on release.
fn type_alt_code(digits: &[KeyCode]) -> AltCodeEntry {
    let mut machine = interpreter("");
    press(&mut machine, KeyCode::LEFT_ALT);

    for digit in digits {
        press(&mut machine, *digit);
        release(&mut machine, *digit);
    }

    match release(&mut machine, KeyCode::LEFT_ALT).report {
        Some(RouterMessage::AltCode(entry)) => entry,
        other => panic!("expected an Alt code report, got {other:?}"),
    }
}

fn decode_for_layout(entry: &AltCodeEntry, layout: &str) -> Option<char> {
    let catalog = CodePageCatalog::load().unwrap();
    let layouts = LayoutCodePages::load().unwrap();
    let layout = LayoutId::try_from(layout.to_string()).unwrap();
    let pages = layouts.pages_for(&layout, &BTreeMap::new());

    entry.decode(pages, false, &catalog)
}

#[test]
fn alt_codes_follow_the_active_layout() {
    use KeyCode as K;

    let oem_130 = type_alt_code(&[K::KP1, K::KP3, K::KP0]);
    let ansi_128 = type_alt_code(&[K::KP0, K::KP1, K::KP2, K::KP8]);
    let oem_152 = type_alt_code(&[K::KP1, K::KP5, K::KP2]);
    let ansi_225 = type_alt_code(&[K::KP0, K::KP2, K::KP2, K::KP5]);

    assert_eq!(decode_for_layout(&oem_130, "br"), Some('é'));
    assert_eq!(decode_for_layout(&ansi_128, "br"), Some('€'));
    assert_eq!(decode_for_layout(&oem_152, "gr+simple"), Some('α'));
    assert_eq!(decode_for_layout(&ansi_225, "gr+simple"), Some('α'));
}

#[test]
fn an_alt_code_masks_alt_once_and_reports_only_at_release() {
    let mut machine = interpreter("");
    let alt_down = press(&mut machine, KeyCode::LEFT_ALT);

    assert_eq!(alt_down.disposition, KeyDisposition::Forward);
    assert!(!alt_down.tap_mask_key);

    let first_digit = press(&mut machine, KeyCode::KP6);
    release(&mut machine, KeyCode::KP6);
    let second_digit = press(&mut machine, KeyCode::KP5);
    release(&mut machine, KeyCode::KP5);

    assert_eq!(first_digit.disposition, KeyDisposition::Swallow);
    assert!(first_digit.tap_mask_key);
    assert_eq!(first_digit.report, None);
    assert!(!second_digit.tap_mask_key);

    let alt_up = release(&mut machine, KeyCode::LEFT_ALT);

    assert_eq!(alt_up.disposition, KeyDisposition::Forward);
    assert!(matches!(alt_up.report, Some(RouterMessage::AltCode(_))));

    press(&mut machine, KeyCode::LEFT_ALT);

    assert!(press(&mut machine, KeyCode::KP6).tap_mask_key);
}

#[test]
fn the_default_shortcut_is_alt_shift_on_release() {
    let mut machine = interpreter("");
    press(&mut machine, KeyCode::LEFT_ALT);

    let shift_down = press(&mut machine, KeyCode::LEFT_SHIFT);
    let shift_up = release(&mut machine, KeyCode::LEFT_SHIFT);
    let alt_up = release(&mut machine, KeyCode::LEFT_ALT);

    assert_eq!(shift_down.report, None);
    assert_eq!(
        shift_up.report,
        Some(RouterMessage::Shortcut(ShortcutAction::NextLayout))
    );
    assert_eq!(shift_up.disposition, KeyDisposition::Forward);
    assert_eq!(alt_up.report, None);
}

#[test]
fn alt_tab_and_alt_shift_letter_reach_the_desktop_untouched() {
    let mut machine = interpreter("");
    press(&mut machine, KeyCode::LEFT_ALT);

    assert_eq!(
        press(&mut machine, TAB_KEY).disposition,
        KeyDisposition::Forward
    );
    assert_eq!(
        release(&mut machine, TAB_KEY).disposition,
        KeyDisposition::Forward
    );

    press(&mut machine, KeyCode::LEFT_SHIFT);
    press(&mut machine, KeyCode::F);

    assert_eq!(release(&mut machine, KeyCode::F).report, None);
    assert_eq!(release(&mut machine, KeyCode::LEFT_SHIFT).report, None);
}

#[test]
fn a_keyed_shortcut_masks_alt() {
    let mut machine = interpreter("[switching.direct]\nbr = \"Alt+Z\"");
    press(&mut machine, KeyCode::LEFT_ALT);

    let shortcut = press(&mut machine, Z_KEY);
    let release_z = release(&mut machine, Z_KEY);

    assert_eq!(shortcut.disposition, KeyDisposition::Swallow);
    assert_eq!(release_z.disposition, KeyDisposition::Swallow);
    assert!(shortcut.tap_mask_key);
    assert!(matches!(
        shortcut.report,
        Some(RouterMessage::Shortcut(ShortcutAction::SelectLayout(_)))
    ));
}

#[test]
fn a_shortcut_during_an_alt_code_discards_the_digits() {
    let mut machine = interpreter("[switching.direct]\nbr = \"Alt+Z\"");
    press(&mut machine, KeyCode::LEFT_ALT);
    press(&mut machine, KeyCode::KP6);
    release(&mut machine, KeyCode::KP6);

    let shortcut = press(&mut machine, Z_KEY);
    release(&mut machine, Z_KEY);
    let alt_up = release(&mut machine, KeyCode::LEFT_ALT);

    assert!(!shortcut.tap_mask_key);
    assert!(matches!(
        shortcut.report,
        Some(RouterMessage::Shortcut(ShortcutAction::SelectLayout(_)))
    ));
    assert_eq!(alt_up.report, None);
}

#[test]
fn ctrl_and_super_shortcuts_mask_only_when_a_tap_would_do_something() {
    let mut machine = interpreter("[switching.direct]\nbr = \"Ctrl+Shift+1\"\ngr = \"Super+1\"");

    press(&mut machine, KeyCode::LEFT_CTRL);
    press(&mut machine, KeyCode::LEFT_SHIFT);

    let ctrl_shortcut = press(&mut machine, DIGIT_ONE_KEY);

    assert_eq!(ctrl_shortcut.disposition, KeyDisposition::Swallow);
    assert!(!ctrl_shortcut.tap_mask_key);

    release(&mut machine, DIGIT_ONE_KEY);
    release(&mut machine, KeyCode::LEFT_SHIFT);
    release(&mut machine, KeyCode::LEFT_CTRL);
    press(&mut machine, KeyCode::LEFT_META);

    let super_shortcut = press(&mut machine, DIGIT_ONE_KEY);

    assert_eq!(super_shortcut.disposition, KeyDisposition::Swallow);
    assert!(super_shortcut.tap_mask_key);
}
