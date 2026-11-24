//! Module: keyway_router::devices
//! Purpose: Find physical keyboards and grab them only when that is safe.
//! Created: 2026-11-08
//! Architecture: The router holds these only while an agent is configured; closing a
//! device, by Drop or by the process dying, makes the kernel release its grab.

use crate::key_router::DeviceId;
use crate::native::{self, EV_ABS, EV_KEY, EV_LED, EV_REL, RawEvent};
use crate::virtual_keyboard::{LedChange, ROUTER_DEVICE_NAME};
use keyway_core::key_codes::KeyEvent;
use nix::errno::Errno;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub const INPUT_DIRECTORY: &str = "/dev/input";
const EVENT_NODE_PREFIX: &str = "event";

/// Keys every real keyboard has; power buttons and media remotes lack them. These are
/// KEY_A, KEY_Z, KEY_SPACE, KEY_ENTER and KEY_LEFTSHIFT.
const REQUIRED_KEYS: [usize; 5] = [30, 44, 57, 28, 42];

/// Which devices the router may grab, beyond them being keyboards.
#[derive(Clone, Debug, Default)]
pub struct DeviceFilter {
    pub excluded: Vec<String>,
    pub only: Vec<String>,
}

impl DeviceFilter {
    pub fn allows(&self, name: &str) -> bool {
        let is_listed = |names: &[String]| names.iter().any(|listed| listed == name);

        name != ROUTER_DEVICE_NAME
            && !is_listed(&self.excluded)
            && (self.only.is_empty() || is_listed(&self.only))
    }
}

pub fn is_event_node(name: &str) -> bool {
    name.starts_with(EVENT_NODE_PREFIX)
}

/// True for plain keyboards. A device that also reports motion is left alone, because
/// grabbing it would take its pointer away from the desktop as well.
fn is_plain_keyboard(types: &[u8], keys: &[u8]) -> bool {
    let is_pointer = native::bit_is_set(types, usize::from(EV_REL))
        || native::bit_is_set(types, usize::from(EV_ABS));

    native::bit_is_set(types, usize::from(EV_KEY))
        && !is_pointer
        && REQUIRED_KEYS
            .iter()
            .all(|key| native::bit_is_set(keys, *key))
}

/// An open keyboard, which is either grabbed or waiting for all of its keys to come up.
#[derive(Debug)]
pub struct Keyboard {
    pub id: DeviceId,
    pub path: PathBuf,
    pub name: String,
    file: File,
    grabbed: bool,
}

impl Keyboard {
    /// Opens a node if it is a keyboard the filter allows, and returns None otherwise.
    /// A node udev has not yet given us access to fails here and is retried when its
    /// permissions change.
    pub fn open(id: DeviceId, path: &Path, filter: &DeviceFilter) -> io::Result<Option<Self>> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;

        let name = native::device_name(file.as_fd())?;
        let types = native::supported_event_types(file.as_fd())?;
        let keys = native::supported_keys(file.as_fd())?;

        if !filter.allows(&name) || !is_plain_keyboard(&types, &keys) {
            return Ok(None);
        }

        Ok(Some(Self {
            id,
            path: path.to_path_buf(),
            name,
            file,
            grabbed: false,
        }))
    }

    pub const fn is_grabbed(&self) -> bool {
        self.grabbed
    }

    /// Reads queued key events. Until the grab is held, the desktop is receiving these
    /// keys itself, so they are discarded here.
    pub fn read_keys(&mut self) -> io::Result<Vec<KeyEvent>> {
        let events = native::read_available_events(&self.file)?;

        if !self.grabbed {
            return Ok(Vec::new());
        }

        Ok(events
            .into_iter()
            .filter_map(RawEvent::to_key_event)
            .collect())
    }

    /// Grabs the keyboard if no key is down, and reports whether it is now grabbed.
    ///
    /// A key held across the grab would have its release hidden from the desktop and stay
    /// stuck, so the state is checked again after grabbing to close that race.
    pub fn try_grab(&mut self) -> io::Result<bool> {
        if self.grabbed {
            return Ok(true);
        }

        if native::any_key_down(self.file.as_fd())? {
            return Ok(false);
        }

        match native::set_grabbed(self.file.as_fd(), true) {
            Ok(()) => {}
            Err(Errno::EBUSY) => return Ok(false),
            Err(error) => return Err(error.into()),
        }

        if native::any_key_down(self.file.as_fd())? {
            native::set_grabbed(self.file.as_fd(), false)?;

            return Ok(false);
        }

        self.grabbed = true;

        Ok(true)
    }

    /// Sets LEDs on the physical keyboard, which the grab cuts off from the desktop.
    pub fn apply_leds(&mut self, changes: &[LedChange]) -> io::Result<()> {
        for change in changes {
            let event = RawEvent {
                kind: EV_LED,
                code: change.led,
                value: i32::from(change.on),
            };

            self.file
                .write_all(&[event.encode(), RawEvent::SYNC.encode()].concat())?;
        }

        Ok(())
    }
}

impl AsFd for Keyboard {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }
}

/// The LED state the desktop last requested, replayed onto newly grabbed keyboards.
#[derive(Clone, Debug, Default)]
pub struct LedState {
    leds: BTreeMap<u16, bool>,
}

impl LedState {
    pub fn record(&mut self, change: LedChange) {
        self.leds.insert(change.led, change.on);
    }

    pub fn as_changes(&self) -> Vec<LedChange> {
        self.leds
            .iter()
            .map(|(led, on)| LedChange { led: *led, on: *on })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{EVENT_TYPE_BYTES, KEY_BYTES};

    fn with_bits<const SIZE: usize>(bits: &[usize]) -> [u8; SIZE] {
        let mut bytes = [0; SIZE];

        for bit in bits {
            bytes[bit / 8] |= 1 << (bit % 8);
        }

        bytes
    }

    #[test]
    fn only_devices_with_typing_keys_and_no_pointer_count_as_keyboards() {
        let key_type = usize::from(EV_KEY);
        let rel_type = usize::from(EV_REL);
        let keyboard_keys: [u8; KEY_BYTES] = with_bits(&REQUIRED_KEYS);
        let power_button_keys: [u8; KEY_BYTES] = with_bits(&[116]);
        let keyboard_types: [u8; EVENT_TYPE_BYTES] = with_bits(&[key_type]);
        let combo_types: [u8; EVENT_TYPE_BYTES] = with_bits(&[key_type, rel_type]);

        assert!(is_plain_keyboard(&keyboard_types, &keyboard_keys));
        assert!(!is_plain_keyboard(&keyboard_types, &power_button_keys));
        assert!(!is_plain_keyboard(&combo_types, &keyboard_keys));
    }

    #[test]
    fn the_filter_never_allows_the_routers_own_device() {
        let everything = DeviceFilter::default();
        let only_own_device = DeviceFilter {
            excluded: Vec::new(),
            only: vec![ROUTER_DEVICE_NAME.to_string()],
        };

        assert!(everything.allows("AT Translated Set 2 keyboard"));
        assert!(!everything.allows(ROUTER_DEVICE_NAME));
        assert!(!only_own_device.allows(ROUTER_DEVICE_NAME));
    }

    #[test]
    fn the_filter_applies_exclusions_and_test_allow_lists() {
        let excluding = DeviceFilter {
            excluded: vec!["Macro pad".to_string()],
            only: Vec::new(),
        };
        let test_only = DeviceFilter {
            excluded: Vec::new(),
            only: vec!["Test keyboard".to_string()],
        };

        assert!(!excluding.allows("Macro pad"));
        assert!(excluding.allows("Laptop keyboard"));
        assert!(test_only.allows("Test keyboard"));
        assert!(!test_only.allows("Laptop keyboard"));
    }

    #[test]
    fn led_state_replays_the_latest_values() {
        let mut state = LedState::default();
        state.record(LedChange { led: 1, on: true });
        state.record(LedChange { led: 1, on: false });
        state.record(LedChange { led: 0, on: true });

        assert_eq!(
            state.as_changes(),
            [
                LedChange { led: 0, on: true },
                LedChange { led: 1, on: false }
            ]
        );
    }
}
