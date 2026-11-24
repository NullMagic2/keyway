//! Module: keyway_desktop::xkb
//! Purpose: Ask libxkbcommon whether a layout uses Right Alt as AltGr.
//! Created: 2026-11-08
//! Architecture: Decides whether Right Alt may start an Alt code, as on Windows. The only
//! module here allowed unsafe code; values match xkbcommon.h and xkbcommon-keysyms.h.

#![allow(unsafe_code)]

use keyway_core::key_codes::KeyCode;
use keyway_core::layout_id::LayoutId;
use std::ffi::{CString, c_char, c_int, c_void};
use std::ptr;

const XKB_CONTEXT_NO_ENVIRONMENT_NAMES: c_int = 1 << 1;
const XKB_KEYMAP_COMPILE_NO_FLAGS: c_int = 0;
const XKB_KEY_ISO_LEVEL3_SHIFT: u32 = 0xFE03;
const XKB_KEY_MODE_SWITCH: u32 = 0xFF7E;
const EVDEV_TO_XKB_KEYCODE: u32 = 8;
const FIRST_LAYOUT: u32 = 0;
const BASE_LEVEL: u32 = 0;
const OPTION_SEPARATOR: &str = ",";

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

/// Reports whether Right Alt types a third level on this layout. When that cannot be
/// determined it answers true, because the key then keeps working as AltGr.
pub fn right_alt_is_altgr(layout: &LayoutId, xkb_options: &[String]) -> bool {
    right_alt_symbol(layout, xkb_options)
        .is_none_or(|symbol| symbol == XKB_KEY_ISO_LEVEL3_SHIFT || symbol == XKB_KEY_MODE_SWITCH)
}

fn right_alt_symbol(layout: &LayoutId, xkb_options: &[String]) -> Option<u32> {
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

    let key = u32::from(KeyCode::RIGHT_ALT.raw()) + EVDEV_TO_XKB_KEYCODE;
    let mut symbols: *const u32 = ptr::null();

    // SAFETY: keymap is live and symbols is a valid place for the result pointer.
    let symbol_count = unsafe {
        xkb_keymap_key_get_syms_by_level(keymap.0, key, FIRST_LAYOUT, BASE_LEVEL, &raw mut symbols)
    };

    if symbol_count < 1 || symbols.is_null() {
        return None;
    }

    // SAFETY: the call reported at least one symbol at this pointer, which stays valid while
    // keymap lives, and keymap is still alive here.
    Some(unsafe { *symbols })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_altgr(layout: &str, options: &[&str]) -> bool {
        let layout = LayoutId::try_from(layout.to_string()).unwrap();
        let options: Vec<String> = options.iter().map(ToString::to_string).collect();

        right_alt_is_altgr(&layout, &options)
    }

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
}
