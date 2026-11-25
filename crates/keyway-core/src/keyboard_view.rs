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

    /// What a key has beyond an ordinary key's width, which it can give to keys beside it.
    const fn spare(self) -> u8 {
        self.0.saturating_sub(Self::ORDINARY.0)
    }

    const fn less(self, quarters: u8) -> Self {
        Self(self.0 - quarters)
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
    /// Types what its key does with Shift, which clicking it holds for that long.
    Shifted,
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
            Self::Shifted => "shifted",
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

/// What a character key shows in each label state; empty where it types nothing. An accent is
/// a label that waits for the next key's letter instead of being typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyLabels {
    texts: [String; LabelState::COUNT],
    accents: [bool; LabelState::COUNT],
}

impl KeyLabels {
    /// Asks for the label of each state. None of them is an accent until with_accents says so.
    pub fn from_states(mut label: impl FnMut(LabelState) -> String) -> Self {
        Self {
            texts: std::array::from_fn(|index| label(LabelState::from_index(index))),
            accents: [false; LabelState::COUNT],
        }
    }

    /// Asks for each state whether its label is an accent.
    pub fn with_accents(mut self, mut is_accent: impl FnMut(LabelState) -> bool) -> Self {
        self.accents = std::array::from_fn(|index| is_accent(LabelState::from_index(index)));

        self
    }

    pub fn get(&self, state: LabelState) -> &str {
        &self.texts[state.index()]
    }

    pub fn is_accent(&self, state: LabelState) -> bool {
        self.accents[state.index()]
    }

    /// The same labels for a keyboard with no AltGr: the third level's states show the others'.
    fn without_level3(&self) -> Self {
        let reached = |state| LabelState {
            level3: false,
            ..state
        };

        Self::from_states(|state| self.get(reached(state)).to_string())
            .with_accents(|state| self.is_accent(reached(state)))
    }

    fn into_vec(self) -> Vec<String> {
        self.texts.into()
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

/// Brackets that belong together. One typed only with Shift gets a key of its own where the
/// other has one, as accents typed only with Shift do.
const BRACKET_PAIRS: [(&str, &str); 6] = [
    ("(", ")"),
    ("[", "]"),
    ("{", "}"),
    ("<", ">"),
    ("\u{ab}", "\u{bb}"),
    ("\u{2039}", "\u{203a}"),
];

/// Names too long for a key that gave up width to the keys beside it, and what it shows then.
const SHORT_NAMES: [(&str, &str); 1] = [("Backspace", "Bksp")];

/// The width of every row in quarters: fifteen keys of the main block, a gap, and the three
/// navigation keys.
const ROW_QUARTERS: u32 = 74;

fn other_bracket(label: &str) -> Option<&'static str> {
    BRACKET_PAIRS.iter().find_map(|&(opening, closing)| {
        if label == opening {
            Some(closing)
        } else if label == closing {
            Some(opening)
        } else {
            None
        }
    })
}

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
    /// The keys followed by a key for their shifted character.
    with_own_key: Vec<KeyCode>,
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

    /// The key for a character key's shifted character, where that has a key of its own.
    fn own_key(&self, code: KeyCode) -> Option<ViewCell> {
        let shifted = self.labels.get(&code)?.get(LabelState::SHIFTED);

        if !self.with_own_key.contains(&code) {
            return None;
        }

        key(code, shifted, KeyWidth::ORDINARY, KeyRole::Shifted)
    }

    /// A character key, and beside it the key for its shifted character if it has one.
    fn character_and_own_key(&self, code: KeyCode) -> [Option<ViewCell>; 2] {
        [self.character(code), self.own_key(code)]
    }

    fn characters(&self, names: &[&str]) -> Vec<Option<ViewCell>> {
        let codes = names.iter().filter_map(|name| key_code(name));

        codes
            .flat_map(|code| self.character_and_own_key(code))
            .collect()
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

    /// The character keys this keyboard draws, in the order of character_keys.
    fn drawn_keys(&self) -> Vec<KeyCode> {
        let is_drawn =
            |code: &KeyCode| !EXTRA_KEYS.contains(code) || self.extra_key_is_needed(*code);

        character_keys().into_iter().filter(is_drawn).collect()
    }

    /// The keys whose shifted character is hard to find there and gets a key of its own: an
    /// accent, or a bracket whose other half has a key. One key for each such character.
    fn keys_with_own_key(&self) -> Vec<KeyCode> {
        let drawn = self.drawn_keys();
        let label =
            |code: &KeyCode, state| self.labels.get(code).map_or("", |labels| labels.get(state));
        let unshifted: Vec<&str> = drawn
            .iter()
            .map(|code| label(code, LabelState::UNSHIFTED))
            .collect();
        let deserves_a_key = |code: &KeyCode| {
            let shifted = label(code, LabelState::SHIFTED);
            let is_accent = self
                .labels
                .get(code)
                .is_some_and(|labels| labels.is_accent(LabelState::SHIFTED));
            let completes_a_pair =
                other_bracket(shifted).is_some_and(|other| unshifted.contains(&other));

            !shifted.is_empty() && !unshifted.contains(&shifted) && (is_accent || completes_a_pair)
        };
        let mut shown: Vec<&str> = Vec::new();
        let mut codes = Vec::new();

        for code in drawn.iter().filter(|code| deserves_a_key(code)) {
            let shifted = label(code, LabelState::SHIFTED);

            if !shown.contains(&shifted) {
                shown.push(shifted);
                codes.push(*code);
            }
        }

        codes
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
            key_code(BACKSLASH).and_then(|code| self.own_key(code)),
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
            row.extend(self.character_and_own_key(KeyCode::ISO_EXTRA));
        }

        row.extend(self.characters(&BOTTOM_ROW));

        if abnt_key {
            row.extend(self.character_and_own_key(KeyCode::ABNT_EXTRA));
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

fn shorten_name(key: &mut ViewKey) {
    for label in &mut key.labels {
        if let Some((_long, short)) = SHORT_NAMES.iter().find(|(long, _short)| label == long) {
            *label = short.to_string();
        }
    }
}

/// Brings a row back to the width of every row after own keys were added to it. Wide keys
/// give up what they have beyond an ordinary key, the last one first; where that is not
/// enough, the last own key is left out.
fn fit(mut row: Vec<ViewCell>) -> Vec<ViewCell> {
    let width = |row: &[ViewCell]| -> u32 {
        row.iter()
            .map(|cell| u32::from(cell.width().quarters()))
            .sum()
    };
    let spare_of = |cell: &ViewCell| match cell {
        ViewCell::Key(key) => key.width.spare(),
        ViewCell::Gap(_) => 0,
    };
    let spare =
        |row: &[ViewCell]| -> u32 { row.iter().map(|cell| u32::from(spare_of(cell))).sum() };
    let is_own_key =
        |cell: &ViewCell| matches!(cell, ViewCell::Key(key) if key.role == KeyRole::Shifted);

    while width(&row) > ROW_QUARTERS + spare(&row) {
        let Some(last_own_key) = row.iter().rposition(is_own_key) else {
            break;
        };

        row.remove(last_own_key);
    }

    let mut excess = width(&row).saturating_sub(ROW_QUARTERS);

    for cell in row.iter_mut().rev() {
        if let ViewCell::Key(key) = cell {
            let given = key
                .width
                .spare()
                .min(u8::try_from(excess).unwrap_or(u8::MAX));

            key.width = key.width.less(given);
            excess -= u32::from(given);

            if given > 0 {
                shorten_name(key);
            }
        }
    }

    row
}

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
        // A third level that no key of this keyboard reaches is not offered.
        let reachable = |labels: KeyLabels| match right_alt_is_altgr {
            true => labels,
            false => labels.without_level3(),
        };
        let labels = character_keys()
            .into_iter()
            .map(|code| (code, reachable(labels_of(code))))
            .collect();
        let mut builder = Builder {
            labels,
            right_alt_is_altgr,
            with_own_key: Vec::new(),
        };

        builder.with_own_key = builder.keys_with_own_key();

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
                .map(|row| fit(row.into_iter().flatten().collect()))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let expected = vec![ROW_QUARTERS; 6];

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

    /// A layout like the Greek one where it matters here: an accent key with another accent
    /// on Shift, and a bracket key with the closing bracket on Shift.
    fn accent_and_bracket_labels(code: KeyCode) -> KeyLabels {
        let pair = |plain: &'static str, shifted: &'static str| {
            KeyLabels::from_states(move |state| {
                if state.shift { shifted } else { plain }.to_string()
            })
        };

        let is = |name| key_code(name) == Some(code);

        if is("Semicolon") {
            pair("\u{b4}", "\u{a8}").with_accents(|_state| true)
        } else if code == KeyCode::ISO_EXTRA {
            pair("\u{ab}", "\u{bb}")
        } else if is("Comma") {
            pair(",", "<")
        } else if is("Dot") {
            pair(".", ">")
        } else {
            code_labels(code)
        }
    }

    fn own_keys(view: &KeyboardView) -> Vec<(KeyCode, Vec<String>)> {
        keys(view)
            .into_iter()
            .filter(|key| key.role == KeyRole::Shifted)
            .map(|key| (key.key, key.labels.clone()))
            .collect()
    }

    #[test]
    fn a_shifted_accent_and_a_lone_bracket_get_keys_beside_their_own() {
        let view = KeyboardView::new(accent_and_bracket_labels, false);
        let semicolon = key_code("Semicolon").unwrap();
        let drawn: Vec<(KeyCode, KeyRole)> =
            keys(&view).iter().map(|key| (key.key, key.role)).collect();
        let follows_its_key = |code| {
            let own_key = drawn
                .iter()
                .position(|key| *key == (code, KeyRole::Shifted));

            own_key.is_some_and(|place| drawn[place - 1] == (code, KeyRole::Plain))
        };

        assert_eq!(
            own_keys(&view),
            vec![
                (semicolon, vec!["\u{a8}".to_string()]),
                (KeyCode::ISO_EXTRA, vec!["\u{bb}".to_string()]),
            ]
        );
        assert!(follows_its_key(semicolon) && follows_its_key(KeyCode::ISO_EXTRA));
        assert_eq!(row_widths(&view), vec![ROW_QUARTERS; 6]);
    }

    #[test]
    fn a_row_takes_only_the_own_keys_it_has_room_for() {
        let accents_on_every_key = |code: KeyCode| {
            KeyLabels::from_states(|state| format!("{}{}", code.raw(), state.shift))
                .with_accents(|state| state.shift)
        };
        let view = KeyboardView::new(accents_on_every_key, false);
        let own_keys_in = |row: &Vec<ViewCell>| {
            let is_own_key = |cell: &&ViewCell| matches!(cell, ViewCell::Key(key) if key.role == KeyRole::Shifted);

            row.iter().filter(is_own_key).count()
        };
        let narrowest = keys(&view).iter().map(|key| key.width.quarters()).min();

        assert_eq!(row_widths(&view), vec![ROW_QUARTERS; 6]);
        assert_eq!(own_keys_in(&view.rows[1]), 1);
        assert!(keys(&view).iter().any(|key| key.labels == ["Bksp"]));
        assert_eq!(narrowest, Some(KeyWidth::ORDINARY.quarters()));
    }

    #[test]
    fn other_shifted_characters_stay_on_their_keys() {
        assert_eq!(own_keys(&KeyboardView::new(code_labels, true)), vec![]);
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
    fn the_third_level_is_shown_only_where_right_alt_reaches_it() {
        let level_labels = |_code| {
            KeyLabels::from_states(|state| if state.level3 { "3" } else { "1" }.to_string())
        };
        let third_level = LabelState {
            level3: true,
            ..LabelState::default()
        };
        let label = |is_altgr| {
            let view = KeyboardView::new(level_labels, is_altgr);

            find(&view, KeyCode::A).unwrap().labels[third_level.index()].clone()
        };

        assert_eq!(label(true), "3");
        assert_eq!(label(false), "1");
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
