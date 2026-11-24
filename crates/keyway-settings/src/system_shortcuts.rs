//! Module: keyway_settings::system_shortcuts
//! Purpose: Recognize the desktop's own layout shortcuts that work on modifier keys alone.
//! Created: 2026-11-08
//! Architecture: Pure rules; input_sources.rs reads and writes the real settings. Such a
//! shortcut sees the same keys as Keyway's, so one press can switch twice. A shortcut with a
//! final key, such as Super+Space, stays: the router swallows that key if Keyway also uses it.

const XKB_SWITCH_PREFIX: &str = "grp:";
const GROUP_OPTION_SEPARATOR: char = '\t';
const ACCELERATOR_MODIFIER_END: char = '>';
const MODIFIER_KEYSYMS: [&str; 10] = [
    "Shift_L",
    "Shift_R",
    "Control_L",
    "Control_R",
    "Alt_L",
    "Alt_R",
    "Meta_L",
    "Meta_R",
    "Super_L",
    "Super_R",
];

/// True for an XKB option that switches layouts, such as grp:alt_shift_toggle. Options that
/// only share the prefix's first letters, such as grp_led:scroll, do not switch anything.
/// libgnomekbd stores options with their group first, as in "grp\tgrp:alt_shift_toggle".
fn is_xkb_switch_option(option: &str) -> bool {
    option
        .rsplit(GROUP_OPTION_SEPARATOR)
        .next()
        .unwrap_or(option)
        .starts_with(XKB_SWITCH_PREFIX)
}

/// True for an accelerator with no final key, such as Alt plus Shift_L in GTK notation.
fn is_modifier_only_accelerator(accelerator: &str) -> bool {
    let key = accelerator
        .rsplit(ACCELERATOR_MODIFIER_END)
        .next()
        .unwrap_or(accelerator);

    MODIFIER_KEYSYMS.contains(&key)
}

/// Returns the XKB options without the ones that switch layouts, keeping the rest in order.
pub fn without_xkb_switching(options: &[String]) -> Vec<String> {
    options
        .iter()
        .filter(|option| !is_xkb_switch_option(option))
        .cloned()
        .collect()
}

/// Returns the accelerators without the modifier-only ones, keeping the rest in order.
pub fn without_modifier_only(accelerators: &[String]) -> Vec<String> {
    accelerators
        .iter()
        .filter(|accelerator| !is_modifier_only_accelerator(accelerator))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn only_switching_options_are_removed() {
        let options = strings(&["grp_led:scroll", "grp:alt_shift_toggle", "compose:ralt"]);

        assert_eq!(
            without_xkb_switching(&options),
            strings(&["grp_led:scroll", "compose:ralt"])
        );
    }

    #[test]
    fn libgnomekbd_options_are_recognized_too() {
        let options = strings(&["grp\tgrp:alt_shift_toggle", "grp_led\tgrp_led:scroll"]);

        assert_eq!(
            without_xkb_switching(&options),
            strings(&["grp_led\tgrp_led:scroll"])
        );
    }

    #[test]
    fn a_led_option_alone_is_not_a_switching_shortcut() {
        let options = strings(&["grp_led:scroll"]);

        assert_eq!(without_xkb_switching(&options), options);
    }

    #[test]
    fn only_modifier_only_accelerators_are_removed() {
        let accelerators = strings(&[
            "<Super>space",
            "<Alt>Shift_L",
            "XF86Keyboard",
            "<Shift>Alt_L",
        ]);

        assert_eq!(
            without_modifier_only(&accelerators),
            strings(&["<Super>space", "XF86Keyboard"])
        );
    }
}
