//! Module: keyway_desktop::xkb
//! Purpose: Ask libxkbcommon what a layout's keys type, and whether Right Alt is its AltGr.
//! Created: 2026-11-08
//! Architecture: Decides whether Right Alt may start an Alt code, as on Windows, and labels the
//! on-screen keyboard's keys. The only module here allowed unsafe code; values match
//! xkbcommon.h, xkbcommon-names.h and xkbcommon-keysyms.h.

#![allow(unsafe_code)]

use keyway_core::key_codes::KeyCode;
use keyway_core::keyboard_view::{KeyLabels, LabelState};
use keyway_core::layout_id::LayoutId;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

const XKB_CONTEXT_NO_ENVIRONMENT_NAMES: c_int = 1 << 1;
const XKB_KEYMAP_COMPILE_NO_FLAGS: c_int = 0;
const XKB_KEY_ISO_LEVEL3_SHIFT: u32 = 0xFE03;
const XKB_KEY_MODE_SWITCH: u32 = 0xFF7E;
const XKB_MOD_INVALID: u32 = u32::MAX;
const EVDEV_TO_XKB_KEYCODE: u32 = 8;
const FIRST_LAYOUT: u32 = 0;
const BASE_LEVEL: u32 = 0;
const OPTION_SEPARATOR: &str = ",";

/// The modifiers that choose a key's level. The system's layouts all put AltGr on Mod5.
const SHIFT_MODIFIER: &CStr = c"Shift";
const CAPS_LOCK_MODIFIER: &CStr = c"Lock";
const LEVEL3_MODIFIER: &CStr = c"Mod5";

/// Dead keys type nothing by themselves, so each is shown as its accent standing alone. These
/// are the ones from dead_grave to dead_iota, which have consecutive symbols.
const XKB_KEY_DEAD_GRAVE: u32 = 0xFE50;
const DEAD_KEY_ACCENTS: [char; 14] = [
    '`', '\u{b4}', '^', '~', '\u{af}', '\u{2d8}', '\u{2d9}', '\u{a8}', '\u{2da}', '\u{2dd}',
    '\u{2c7}', '\u{b8}', '\u{2db}', '\u{37a}',
];

#[repr(C)]
struct XkbRuleNames {
    rules: *const c_char,
    model: *const c_char,
    layout: *const c_char,
    variant: *const c_char,
    options: *const c_char,
}

#[link(name = "xkbcommon")]
unsafe extern "C" {
    fn xkb_context_new(flags: c_int) -> *mut c_void;
    fn xkb_context_unref(context: *mut c_void);
    fn xkb_keymap_new_from_names(
        context: *mut c_void,
        names: *const XkbRuleNames,
        flags: c_int,
    ) -> *mut c_void;
    fn xkb_keymap_unref(keymap: *mut c_void);
    fn xkb_keymap_mod_get_index(keymap: *mut c_void, name: *const c_char) -> u32;
    fn xkb_state_new(keymap: *mut c_void) -> *mut c_void;
    fn xkb_state_unref(state: *mut c_void);
    fn xkb_state_update_mask(
        state: *mut c_void,
        depressed_modifiers: u32,
        latched_modifiers: u32,
        locked_modifiers: u32,
        depressed_layout: u32,
        latched_layout: u32,
        locked_layout: u32,
    ) -> c_int;
    fn xkb_state_key_get_one_sym(state: *mut c_void, key: u32) -> u32;
    fn xkb_keysym_to_utf32(symbol: u32) -> u32;
    fn xkb_keymap_key_get_syms_by_level(
        keymap: *mut c_void,
        key: u32,
        layout: u32,
        level: u32,
        syms_out: *mut *const u32,
    ) -> c_int;
}

struct Context(*mut c_void);

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: self.0 came from xkb_context_new, is not null, and is released only here.
        unsafe { xkb_context_unref(self.0) };
    }
}

struct Keymap(*mut c_void);

impl Drop for Keymap {
    fn drop(&mut self) {
        // SAFETY: self.0 came from xkb_keymap_new_from_names, is not null, and is released
        // only here.
        unsafe { xkb_keymap_unref(self.0) };
    }
}

struct State(*mut c_void);

impl Drop for State {
    fn drop(&mut self) {
        // SAFETY: self.0 came from xkb_state_new, is not null, and is released only here.
        unsafe { xkb_state_unref(self.0) };
    }
}

/// One layout as the desktop compiles it. Fields drop in this order, each before what it uses.
pub struct LayoutKeymap {
    state: State,
    keymap: Keymap,
    _context: Context,
}

impl LayoutKeymap {
    /// Compiles a layout with the desktop's XKB options. None means it could not be compiled,
    /// as for a name the system does not have.
    pub fn compile(layout: &LayoutId, xkb_options: &[String]) -> Option<Self> {
        let layout_name = CString::new(layout.base()).ok()?;
        let variant = CString::new(layout.variant().unwrap_or_default()).ok()?;
        let options = CString::new(xkb_options.join(OPTION_SEPARATOR)).ok()?;

        let names = XkbRuleNames {
            rules: ptr::null(),
            model: ptr::null(),
            layout: layout_name.as_ptr(),
            variant: variant.as_ptr(),
            options: options.as_ptr(),
        };

        // SAFETY: the flag is a valid context flag, and the call takes nothing else.
        let context = Context(unsafe { xkb_context_new(XKB_CONTEXT_NO_ENVIRONMENT_NAMES) });

        if context.0.is_null() {
            return None;
        }

        // SAFETY: context is live, and names and its strings outlive the call, which copies
        // what it needs.
        let keymap = Keymap(unsafe {
            xkb_keymap_new_from_names(context.0, &raw const names, XKB_KEYMAP_COMPILE_NO_FLAGS)
        });

        if keymap.0.is_null() {
            return None;
        }

        // SAFETY: keymap is live, and the state takes its own reference to it.
        let state = State(unsafe { xkb_state_new(keymap.0) });

        if state.0.is_null() {
            return None;
        }

        Some(Self {
            state,
            keymap,
            _context: context,
        })
    }

    /// Reports whether Right Alt types a third level on this layout. A key with no symbol
    /// counts as one, like a layout that could not be compiled.
    pub fn right_alt_is_altgr(&self) -> bool {
        let key = u32::from(KeyCode::RIGHT_ALT.raw()) + EVDEV_TO_XKB_KEYCODE;
        let mut symbols: *const u32 = ptr::null();

        // SAFETY: the keymap is live and symbols is a valid place for the result pointer.
        let symbol_count = unsafe {
            xkb_keymap_key_get_syms_by_level(
                self.keymap.0,
                key,
                FIRST_LAYOUT,
                BASE_LEVEL,
                &raw mut symbols,
            )
        };

        if symbol_count < 1 || symbols.is_null() {
            return true;
        }

        // SAFETY: the call reported at least one symbol at this pointer, which stays valid
        // while the keymap lives, and self keeps it alive.
        let symbol = unsafe { *symbols };

        symbol == XKB_KEY_ISO_LEVEL3_SHIFT || symbol == XKB_KEY_MODE_SWITCH
    }

    /// Returns the bit of a modifier, or no bit for one the keymap does not have.
    fn modifier_mask(&self, name: &CStr) -> u32 {
        // SAFETY: the keymap is live and name is a terminated string that outlives the call.
        let index = unsafe { xkb_keymap_mod_get_index(self.keymap.0, name.as_ptr()) };

        if index == XKB_MOD_INVALID {
            return 0;
        }

        1_u32.checked_shl(index).unwrap_or_default()
    }

    /// Returns the symbol a key types in a state of Shift, AltGr and Caps Lock.
    fn symbol(&self, key: u32, state: LabelState) -> u32 {
        let mask = |held: bool, name| if held { self.modifier_mask(name) } else { 0 };
        let held = mask(state.shift, SHIFT_MODIFIER) | mask(state.level3, LEVEL3_MODIFIER);
        let locked = mask(state.caps_lock, CAPS_LOCK_MODIFIER);

        // SAFETY: the state is live, and only this object uses it, from one thread at a
        // time. Any key code is accepted; one the keymap lacks has no symbol.
        unsafe {
            xkb_state_update_mask(self.state.0, held, 0, locked, 0, 0, 0);
            xkb_state_key_get_one_sym(self.state.0, key)
        }
    }

    /// Returns what a key types in each state, and which of that are accents.
    pub fn key_labels(&self, key: KeyCode) -> KeyLabels {
        let key = u32::from(key.raw()) + EVDEV_TO_XKB_KEYCODE;

        KeyLabels::from_states(|state| symbol_label(self.symbol(key, state)))
            .with_accents(|state| dead_key_accent(self.symbol(key, state)).is_some())
    }
}

/// Returns the accent a dead key is shown as, or nothing for any other symbol.
fn dead_key_accent(symbol: u32) -> Option<char> {
    let offset = symbol.checked_sub(XKB_KEY_DEAD_GRAVE)?;

    DEAD_KEY_ACCENTS.get(usize::try_from(offset).ok()?).copied()
}

/// Returns the character a symbol types, or nothing for one that types none.
fn symbol_label(symbol: u32) -> String {
    // SAFETY: the call takes any value and reads nothing else.
    let typed = || char::from_u32(unsafe { xkb_keysym_to_utf32(symbol) });

    dead_key_accent(symbol)
        .or_else(typed)
        .filter(|character| !character.is_control())
        .map(String::from)
        .unwrap_or_default()
}

/// Reports whether Right Alt types a third level on this layout. When that cannot be
/// determined it answers true, because the key then keeps working as AltGr.
pub fn right_alt_is_altgr(keymap: Option<&LayoutKeymap>) -> bool {
    keymap.is_none_or(LayoutKeymap::right_alt_is_altgr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(layout: &str, options: &[&str]) -> Option<LayoutKeymap> {
        let layout = LayoutId::try_from(layout.to_string()).unwrap();
        let options: Vec<String> = options.iter().map(ToString::to_string).collect();

        LayoutKeymap::compile(&layout, &options)
    }

    fn is_altgr(layout: &str, options: &[&str]) -> bool {
        right_alt_is_altgr(compile(layout, options).as_ref())
    }

    fn labels(layout: &str, key: KeyCode) -> KeyLabels {
        compile(layout, &[]).unwrap().key_labels(key)
    }

    const PLAIN: LabelState = LabelState {
        shift: false,
        level3: false,
        caps_lock: false,
    };
    const SHIFT: LabelState = LabelState {
        shift: true,
        ..PLAIN
    };
    const ALTGR: LabelState = LabelState {
        level3: true,
        ..PLAIN
    };
    const CAPS_LOCK: LabelState = LabelState {
        caps_lock: true,
        ..PLAIN
    };

    /// The key that types 1 on most layouts, which no letter rule applies to.
    const DIGIT_ONE: KeyCode = KeyCode::new(2);

    /// The key right of P, where the Brazilian layout has its acute and grave dead keys.
    const RIGHT_OF_P: KeyCode = KeyCode::new(26);

    #[test]
    fn layouts_with_a_third_level_on_right_alt_are_detected() {
        assert!(is_altgr("br", &[]));
        assert!(is_altgr("de", &[]));
        assert!(is_altgr("us+intl", &[]));
    }

    #[test]
    fn plain_us_leaves_right_alt_free() {
        assert!(!is_altgr("us", &[]));
    }

    #[test]
    fn xkb_options_are_applied() {
        assert!(is_altgr("us", &["lv3:ralt_switch"]));
    }

    #[test]
    fn an_unknown_layout_keeps_right_alt_as_altgr() {
        assert!(is_altgr("no-such-layout", &[]));
    }

    #[test]
    fn a_key_is_labelled_with_what_the_layout_types() {
        let latin = labels("br", KeyCode::A);
        let greek = labels("gr+simple", KeyCode::A);

        assert_eq!(latin.get(PLAIN), "a");
        assert_eq!(latin.get(SHIFT), "A");
        assert_eq!(greek.get(PLAIN), "\u{3b1}");
        assert_eq!(greek.get(SHIFT), "\u{391}");
    }

    #[test]
    fn caps_lock_changes_letters_and_leaves_digits() {
        assert_eq!(labels("br", KeyCode::A).get(CAPS_LOCK), "A");
        assert_eq!(labels("br", DIGIT_ONE).get(CAPS_LOCK), "1");
        assert_eq!(labels("br", DIGIT_ONE).get(SHIFT), "!");
    }

    #[test]
    fn altgr_reaches_the_third_level() {
        assert_eq!(labels("br", DIGIT_ONE).get(ALTGR), "\u{b9}");
        assert_eq!(labels("us", DIGIT_ONE).get(ALTGR), "1");
    }

    #[test]
    fn a_dead_key_is_labelled_with_its_accent() {
        let accents = labels("br", RIGHT_OF_P);

        assert_eq!(accents.get(PLAIN), "\u{b4}");
        assert_eq!(accents.get(SHIFT), "`");
    }

    #[test]
    fn only_a_dead_key_is_an_accent() {
        let accents = labels("br", RIGHT_OF_P);
        let greek_accents = labels("gr+simple", KeyCode::new(39));

        assert!(accents.is_accent(PLAIN) && accents.is_accent(SHIFT));
        assert!(greek_accents.is_accent(SHIFT));
        assert_eq!(greek_accents.get(SHIFT), "\u{a8}");
        assert!(!labels("us", KeyCode::new(41)).is_accent(SHIFT));
    }

    #[test]
    fn a_key_the_layout_leaves_empty_has_no_label() {
        assert_eq!(labels("us", KeyCode::ABNT_EXTRA).get(PLAIN), "");
        assert_eq!(labels("br", KeyCode::ABNT_EXTRA).get(PLAIN), "/");
    }
}
