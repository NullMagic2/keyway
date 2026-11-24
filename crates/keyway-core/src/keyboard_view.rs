//! Module: keyway_core::keyboard_view
//! Purpose: The keys the on-screen keyboard draws: where each one is, how wide, and what it shows.
//! Created: 2026-11-24
//! Architecture: Pure geometry of a 105-key keyboard without its keypad, in rows of equal width.
//! keyway-desktop reads each character key's labels from the active layout, and the desktop
//! extensions draw the result and press the keys.

use crate::key_codes::{KeyCode, NamedKey};
use std::collections::BTreeMap;

/// A width in quarters of an ordinary key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyWidth(u8);

impl KeyWidth {
    const ORDINARY: Self = Self(4);
    const BACKSPACE: Self = Self(8);
    const TAB: Self = Self(6);
    const BACKSLASH: Self = Self(6);
    const CAPS_LOCK: Self = Self(7);
    const ENTER: Self = Self(9);
    const LEFT_SHIFT: Self = Self(9);
    const RIGHT_SHIFT: Self = Self(11);
    const BOTTOM_ROW_MODIFIER: Self = Self(5);
    const SPACE: Self = Self(25);

    /// Between Esc and F1, as wide as the key that a function row leaves out.
    const AFTER_ESCAPE: Self = Self(4);
    const BETWEEN_GROUPS: Self = Self(2);

    pub const fn quarters(self) -> u8 {
        self.0
    }

    /// What is left of a key that gives an ordinary key's width to the one beside it.
    const fn less_one_key(self) -> Self {
        Self(self.0 - Self::ORDINARY.0)
    }
}

/// What clicking a key does, beyond pressing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyRole {
    Plain,
    /// Stays down until the next key that is not a modifier, as Ctrl, Alt and Super do.
    Modifier,
    /// A modifier that also chooses the shifted labels.
    Shift,
    /// A modifier that also chooses the third level's labels: AltGr.
    Level3,
    /// Chooses the Caps Lock labels for as long as the desktop reports the lock on.
    CapsLock,
}

impl KeyRole {
    /// The name the desktop extensions know this role by.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Modifier => "modifier",
            Self::Shift => "shift",
            Self::Level3 => "level3",
            Self::CapsLock => "caps_lock",
        }
    }
}

/// The modifiers that decide which of a character key's labels is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LabelState {
    pub shift: bool,
    pub level3: bool,
    pub caps_lock: bool,
}

impl LabelState {
    pub const COUNT: usize = 8;
    const UNSHIFTED: Self = Self::from_index(0);
    const SHIFTED: Self = Self::from_index(1);

    /// Where this state's label is in a character key's labels. The extensions compute the same.
    pub const fn index(self) -> usize {
        self.shift as usize | (self.level3 as usize) << 1 | (self.caps_lock as usize) << 2
    }

    const fn from_index(index: usize) -> Self {
        Self {
            shift: index & 1 != 0,
            level3: index & 2 != 0,
            caps_lock: index & 4 != 0,
        }
    }

    /// Every state, in the order of its index.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT).map(Self::from_index)
    }
}

/// What a character key shows in each label state; empty where it types nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyLabels([String; LabelState::COUNT]);

impl KeyLabels {
    /// Asks for the label of each state.
    pub fn from_states(mut label: impl FnMut(LabelState) -> String) -> Self {
        Self(std::array::from_fn(|index| {
            label(LabelState::from_index(index))
        }))
    }

    pub fn get(&self, state: LabelState) -> &str {
        &self.0[state.index()]
    }

    fn into_vec(self) -> Vec<String> {
        self.0.into()
    }
}

/// One key as drawn. A character key has a label for each state, any other key one label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewKey {
    pub key: KeyCode,
    pub width: KeyWidth,
    pub role: KeyRole,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewCell {
    Gap(KeyWidth),
    Key(ViewKey),
}

impl ViewCell {
    pub const fn width(&self) -> KeyWidth {
        match self {
            Self::Gap(width) => *width,
            Self::Key(key) => key.width,
        }
    }
}

/// The whole keyboard, top row first. The default is empty, which draws nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyboardView {
    pub rows: Vec<Vec<ViewCell>>,
}

/// The character keys every keyboard has, by their names in key_codes, a row at a time.
const NUMBER_ROW: [&str; 13] = [
    "Grave", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "Minus", "Equal",
];
const TOP_ROW: [&str; 12] = [
    "Q",
    "W",
    "E",
    "R",
    "T",
    "Y",
    "U",
    "I",
    "O",
    "P",
    "LeftBracket",
    "RightBracket",
];
const HOME_ROW: [&str; 11] = [
    "A",
    "S",
    "D",
    "F",
    "G",
    "H",
    "J",
    "K",
    "L",
    "Semicolon",
    "Apostrophe",
];
const BOTTOM_ROW: [&str; 10] = ["Z", "X", "C", "V", "B", "N", "M", "Comma", "Dot", "Slash"];
const BACKSLASH: &str = "Backslash";

/// The function keys, in the groups a keyboard spaces them in.
const FUNCTION_GROUPS: [[&str; 4]; 3] = [
    ["F1", "F2", "F3", "F4"],
    ["F5", "F6", "F7", "F8"],
    ["F9", "F10", "F11", "F12"],
];

fn key_code(name: &str) -> Option<KeyCode> {
    NamedKey::from_name(name).map(|key| key.code)
}

fn gap(width: KeyWidth) -> Option<ViewCell> {
    Some(ViewCell::Gap(width))
}

fn key(key: KeyCode, label: &str, width: KeyWidth, role: KeyRole) -> Option<ViewCell> {
    Some(ViewCell::Key(ViewKey {
        key,
        width,
        role,
        labels: vec![label.to_string()],
    }))
}

/// A key that types nothing a layout decides, found by its name in key_codes.
fn named(name: &str, label: &str, width: KeyWidth) -> Option<ViewCell> {
    key(key_code(name)?, label, width, KeyRole::Plain)
}

/// Builds one keyboard from the labels of its character keys.
struct Builder {
    labels: BTreeMap<KeyCode, KeyLabels>,
    right_alt_is_altgr: bool,
}

impl Builder {
    fn character(&self, code: KeyCode) -> Option<ViewCell> {
        Some(ViewCell::Key(ViewKey {
            key: code,
            width: KeyWidth::ORDINARY,
            role: KeyRole::Plain,
            labels: self.labels.get(&code)?.clone().into_vec(),
        }))
    }

    fn characters(&self, names: &[&str]) -> Vec<Option<ViewCell>> {
        let character = |name: &&str| self.character(key_code(name)?);

        names.iter().map(character).collect()
    }

    /// Whether a key that only some keyboards have types, plain or shifted, something that no
    /// key of every keyboard does. Only then is it worth a place.
    fn extra_key_is_needed(&self, extra: KeyCode) -> bool {
        let typed_by = |code: &KeyCode| {
            let labels = self.labels.get(code);
            let states = [LabelState::UNSHIFTED, LabelState::SHIFTED];

            states.map(|state| labels.map_or("", |labels| labels.get(state)))
        };
        let elsewhere: Vec<&str> = self
            .labels
            .keys()
            .filter(|code| !EXTRA_KEYS.contains(code))
            .flat_map(typed_by)
            .collect();

        typed_by(&extra)
            .iter()
            .any(|label| !label.is_empty() && !elsewhere.contains(label))
    }

    fn function_row(&self) -> Vec<Option<ViewCell>> {
        let mut row = vec![
            named("Esc", "Esc", KeyWidth::ORDINARY),
            gap(KeyWidth::AFTER_ESCAPE),
        ];

        for (position, group) in FUNCTION_GROUPS.iter().enumerate() {
            if position > 0 {
                row.push(gap(KeyWidth::BETWEEN_GROUPS));
            }

            row.extend(group.map(|name| named(name, name, KeyWidth::ORDINARY)));
        }

        row.extend([
            gap(KeyWidth::BETWEEN_GROUPS),
            key(
                KeyCode::PRINT_SCREEN,
                "PrtSc",
                KeyWidth::ORDINARY,
                KeyRole::Plain,
            ),
            named("ScrollLock", "ScrLk", KeyWidth::ORDINARY),
            named("Pause", "Pause", KeyWidth::ORDINARY),
        ]);

        row
    }

    fn number_row(&self) -> Vec<Option<ViewCell>> {
        let mut row = self.characters(&NUMBER_ROW);

        row.extend([
            named("Backspace", "Backspace", KeyWidth::BACKSPACE),
            gap(KeyWidth::BETWEEN_GROUPS),
            named("Insert", "Ins", KeyWidth::ORDINARY),
            named("Home", "Home", KeyWidth::ORDINARY),
            named("PageUp", "PgUp", KeyWidth::ORDINARY),
        ]);

        row
    }

    fn top_row(&self) -> Vec<Option<ViewCell>> {
        let mut row = vec![named("Tab", "Tab", KeyWidth::TAB)];

        row.extend(self.characters(&TOP_ROW));
        row.extend([
            key_code(BACKSLASH).and_then(|code| {
                let mut cell = self.character(code)?;

                if let ViewCell::Key(key) = &mut cell {
                    key.width = KeyWidth::BACKSLASH;
                }

                Some(cell)
            }),
            gap(KeyWidth::BETWEEN_GROUPS),
            named("Delete", "Del", KeyWidth::ORDINARY),
            named("End", "End", KeyWidth::ORDINARY),
            named("PageDown", "PgDn", KeyWidth::ORDINARY),
        ]);

        row
    }

    fn home_row(&self) -> Vec<Option<ViewCell>> {
        let mut row = vec![key(
            KeyCode::CAPS_LOCK,
            "Caps",
            KeyWidth::CAPS_LOCK,
            KeyRole::CapsLock,
        )];

        row.extend(self.characters(&HOME_ROW));
        row.extend([
            named("Enter", "Enter", KeyWidth::ENTER),
            gap(KeyWidth::BETWEEN_GROUPS),
            gap(KeyWidth::ORDINARY),
            gap(KeyWidth::ORDINARY),
            gap(KeyWidth::ORDINARY),
        ]);

        row
    }

    /// Each extra key takes its place from the Shift key beside it, as on a real keyboard.
    fn bottom_row(&self) -> Vec<Option<ViewCell>> {
        let iso_key = self.extra_key_is_needed(KeyCode::ISO_EXTRA);
        let abnt_key = self.extra_key_is_needed(KeyCode::ABNT_EXTRA);
        let shift = |code, width: KeyWidth, shortened: bool| {
            let width = if shortened {
                width.less_one_key()
            } else {
                width
            };

            key(code, "Shift", width, KeyRole::Shift)
        };
        let mut row = vec![shift(KeyCode::LEFT_SHIFT, KeyWidth::LEFT_SHIFT, iso_key)];

        if iso_key {
            row.push(self.character(KeyCode::ISO_EXTRA));
        }

        row.extend(self.characters(&BOTTOM_ROW));

        if abnt_key {
            row.push(self.character(KeyCode::ABNT_EXTRA));
        }

        row.extend([
            shift(KeyCode::RIGHT_SHIFT, KeyWidth::RIGHT_SHIFT, abnt_key),
            gap(KeyWidth::BETWEEN_GROUPS),
            gap(KeyWidth::ORDINARY),
            named("Up", "\u{2191}", KeyWidth::ORDINARY),
            gap(KeyWidth::ORDINARY),
        ]);

        row
    }

    /// Right Alt is AltGr only on a layout with a third level, as it is for Alt codes.
    fn space_row(&self) -> Vec<Option<ViewCell>> {
        let width = KeyWidth::BOTTOM_ROW_MODIFIER;
        let held = |code, label| key(code, label, width, KeyRole::Modifier);
        let right_alt = if self.right_alt_is_altgr {
            key(KeyCode::RIGHT_ALT, "AltGr", width, KeyRole::Level3)
        } else {
            held(KeyCode::RIGHT_ALT, "Alt")
        };

        vec![
            held(KeyCode::LEFT_CTRL, "Ctrl"),
            held(KeyCode::LEFT_META, "Super"),
            held(KeyCode::LEFT_ALT, "Alt"),
            named("Space", "", KeyWidth::SPACE),
            right_alt,
            held(KeyCode::RIGHT_META, "Super"),
            named("Menu", "Menu", width),
            held(KeyCode::RIGHT_CTRL, "Ctrl"),
            gap(KeyWidth::BETWEEN_GROUPS),
            named("Left", "\u{2190}", KeyWidth::ORDINARY),
            named("Down", "\u{2193}", KeyWidth::ORDINARY),
            named("Right", "\u{2192}", KeyWidth::ORDINARY),
        ]
    }
}

/// Keys only some keyboards have: beside the left Shift of ISO and ABNT2 ones, and beside the
/// right Shift of ABNT2 ones.
const EXTRA_KEYS: [KeyCode; 2] = [KeyCode::ISO_EXTRA, KeyCode::ABNT_EXTRA];

/// The keys whose labels come from the layout.
pub fn character_keys() -> Vec<KeyCode> {
    let rows = [&NUMBER_ROW[..], &TOP_ROW, &HOME_ROW, &BOTTOM_ROW];
    let names = rows.into_iter().flatten().chain([&BACKSLASH]);

    names
        .filter_map(|name| key_code(name))
        .chain(EXTRA_KEYS)
        .collect()
}

impl KeyboardView {
    /// Lays out the keyboard for one layout. labels_of answers for each of character_keys, and
    /// right_alt_is_altgr says whether Right Alt chooses the third level there.
    pub fn new(mut labels_of: impl FnMut(KeyCode) -> KeyLabels, right_alt_is_altgr: bool) -> Self {
        let labels = character_keys()
            .into_iter()
            .map(|code| (code, labels_of(code)))
            .collect();
        let builder = Builder {
            labels,
            right_alt_is_altgr,
        };
        let rows = [
            builder.function_row(),
            builder.number_row(),
            builder.top_row(),
            builder.home_row(),
            builder.bottom_row(),
            builder.space_row(),
        ];

        Self {
            rows: rows
                .into_iter()
                .map(|row| row.into_iter().flatten().collect())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN_BLOCK_QUARTERS: u32 = 60;
    const NAVIGATION_QUARTERS: u32 = 14;

    /// A layout that types each key's own code, in upper case with Shift.
    fn code_labels(code: KeyCode) -> KeyLabels {
        KeyLabels::from_states(|state| {
            let shift = if state.shift { "S" } else { "" };

            format!("{shift}{}", code.raw())
        })
    }

    /// The same, except that the extra keys type what other keys already do.
    fn labels_without_extra_keys(code: KeyCode) -> KeyLabels {
        if EXTRA_KEYS.contains(&code) {
            return code_labels(KeyCode::A);
        }

        code_labels(code)
    }

    fn keys(view: &KeyboardView) -> Vec<&ViewKey> {
        let cells = view.rows.iter().flatten();

        cells
            .filter_map(|cell| match cell {
                ViewCell::Key(key) => Some(key),
                ViewCell::Gap(_) => None,
            })
            .collect()
    }

    fn find(view: &KeyboardView, code: KeyCode) -> Option<&ViewKey> {
        keys(view).into_iter().find(|key| key.key == code)
    }

    fn row_widths(view: &KeyboardView) -> Vec<u32> {
        let width = |row: &Vec<ViewCell>| {
            row.iter()
                .map(|cell| u32::from(cell.width().quarters()))
                .sum()
        };

        view.rows.iter().map(width).collect()
    }

    #[test]
    fn every_row_is_as_wide_as_the_others() {
        let expected = vec![MAIN_BLOCK_QUARTERS + NAVIGATION_QUARTERS; 6];

        for labels in [
            code_labels as fn(KeyCode) -> KeyLabels,
            labels_without_extra_keys,
        ] {
            for right_alt_is_altgr in [false, true] {
                let view = KeyboardView::new(labels, right_alt_is_altgr);

                assert_eq!(row_widths(&view), expected);
            }
        }
    }

    #[test]
    fn no_key_is_drawn_twice() {
        let view = KeyboardView::new(code_labels, true);
        let mut codes: Vec<KeyCode> = keys(&view).iter().map(|key| key.key).collect();
        let drawn = codes.len();

        codes.sort();
        codes.dedup();

        assert_eq!(codes.len(), drawn);
    }

    #[test]
    fn character_keys_carry_a_label_for_each_state() {
        let view = KeyboardView::new(code_labels, true);
        let key = find(&view, KeyCode::A).unwrap();
        let shifted = LabelState {
            shift: true,
            ..LabelState::default()
        };

        assert_eq!(key.labels.len(), LabelState::COUNT);
        assert_eq!(key.labels[LabelState::default().index()], "30");
        assert_eq!(key.labels[shifted.index()], "S30");
        assert_eq!(key.role, KeyRole::Plain);
    }

    #[test]
    fn label_states_are_listed_in_the_order_of_their_index() {
        let indices: Vec<usize> = LabelState::all().map(LabelState::index).collect();

        assert_eq!(indices, (0..LabelState::COUNT).collect::<Vec<_>>());
    }

    #[test]
    fn an_extra_key_is_drawn_only_when_it_types_something_of_its_own() {
        let with_extra = KeyboardView::new(code_labels, true);
        let without = KeyboardView::new(labels_without_extra_keys, true);

        for extra in EXTRA_KEYS {
            assert!(find(&with_extra, extra).is_some());
            assert!(find(&without, extra).is_none());
        }

        let left_shift = |view| find(view, KeyCode::LEFT_SHIFT).unwrap().width;

        assert_eq!(left_shift(&without), KeyWidth::LEFT_SHIFT);
        assert_eq!(left_shift(&with_extra), KeyWidth::LEFT_SHIFT.less_one_key());
    }

    #[test]
    fn right_alt_is_altgr_only_where_the_layout_says_so() {
        let right_alt = |is_altgr| {
            let view = KeyboardView::new(code_labels, is_altgr);
            let key = find(&view, KeyCode::RIGHT_ALT).unwrap();

            (key.role, key.labels.clone())
        };

        assert_eq!(
            right_alt(true),
            (KeyRole::Level3, vec!["AltGr".to_string()])
        );
        assert_eq!(
            right_alt(false),
            (KeyRole::Modifier, vec!["Alt".to_string()])
        );
    }
}
