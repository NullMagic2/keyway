//! Module: keyway_router::virtual_keyboard
//! Purpose: The uinput keyboard that the desktop receives every routed key from.
//! Created: 2026-11-08
//! Architecture: Written to by the router; LED changes from the desktop are read back
//! so they can reach the physical keyboards.

use crate::native::{self, EV_KEY, EV_LED, RawEvent};
use keyway_core::key_codes::KeyEvent;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::ops::RangeInclusive;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

const UINPUT_PATH: &str = "/dev/uinput";
const BUS_VIRTUAL: u16 = 0x06;
const VENDOR_ID: u16 = 0x4B57;
const PRODUCT_ID: u16 = 0x0001;
const VERSION: u16 = 1;

/// Exact name of the router's own device, which it must never grab. The libinput quirk
/// in packaging/30-keyway.quirks matches this name.
pub const ROUTER_DEVICE_NAME: &str = "Keyway virtual keyboard";

/// Keyboard keys only: the button codes between and after these ranges would make the
/// desktop classify the device as a mouse or a gamepad.
const KEY_RANGES: [RangeInclusive<u16>; 3] = [1..=0xFF, 0x160..=0x21F, 0x230..=0x2BF];

/// A change to one keyboard LED, requested by the desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedChange {
    pub led: u16,
    pub on: bool,
}

/// A virtual keyboard. The kernel releases its held keys when it is destroyed, whether by
/// this type's Drop or by the process dying.
#[derive(Debug)]
pub struct VirtualKeyboard {
    uinput: File,
}

impl VirtualKeyboard {
    pub fn create(name: &str) -> io::Result<Self> {
        let uinput = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(Path::new(UINPUT_PATH))?;

        native::enable_event_type(uinput.as_fd(), EV_KEY)?;
        native::enable_event_type(uinput.as_fd(), EV_LED)?;

        for key in KEY_RANGES.into_iter().flatten() {
            native::enable_key(uinput.as_fd(), key)?;
        }

        for led in 0..=libc::LED_MAX {
            native::enable_led(uinput.as_fd(), led)?;
        }

        native::set_up_device(uinput.as_fd(), &device_setup(name))?;
        native::create_device(uinput.as_fd())?;

        Ok(Self { uinput })
    }

    /// Sends key events to the desktop, each followed by a sync so it takes effect at once.
    pub fn emit(&mut self, events: &[KeyEvent]) -> io::Result<()> {
        for event in events {
            let key = RawEvent::from_key_event(*event).encode();
            let sync = RawEvent::SYNC.encode();

            self.uinput.write_all(&[key, sync].concat())?;
        }

        Ok(())
    }

    /// Returns the LED changes the desktop has requested since the last call.
    pub fn read_led_changes(&mut self) -> io::Result<Vec<LedChange>> {
        let events = native::read_available_events(&self.uinput)?;
        let led_events = events.into_iter().filter(|event| event.kind == EV_LED);

        Ok(led_events
            .map(|event| LedChange {
                led: event.code,
                on: event.value != 0,
            })
            .collect())
    }
}

impl AsFd for VirtualKeyboard {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.uinput.as_fd()
    }
}

impl Drop for VirtualKeyboard {
    fn drop(&mut self) {
        // Closing the file destroys the device anyway, so a failure here changes nothing.
        let _ = native::destroy_device(self.uinput.as_fd());
    }
}

fn device_setup(name: &str) -> libc::uinput_setup {
    let mut name_field = [0; libc::UINPUT_MAX_NAME_SIZE];
    let last_usable_index = libc::UINPUT_MAX_NAME_SIZE - 1;

    for (slot, byte) in name_field[..last_usable_index].iter_mut().zip(name.bytes()) {
        *slot = byte as libc::c_char;
    }

    libc::uinput_setup {
        id: libc::input_id {
            bustype: BUS_VIRTUAL,
            vendor: VENDOR_ID,
            product: PRODUCT_ID,
            version: VERSION,
        },
        name: name_field,
        ff_effects_max: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{KEY_BYTES, bit_is_set};

    const BTN_LEFT: usize = 0x110;
    const KEY_A: usize = 30;

    #[test]
    fn long_names_stay_terminated() {
        let setup = device_setup(&"x".repeat(libc::UINPUT_MAX_NAME_SIZE * 2));

        assert_eq!(setup.name[libc::UINPUT_MAX_NAME_SIZE - 1], 0);
        assert_eq!(setup.name[0], b'x' as libc::c_char);
    }

    #[test]
    fn key_ranges_cover_keys_but_not_mouse_buttons() {
        let mut bits = [0_u8; KEY_BYTES];

        for key in KEY_RANGES.into_iter().flatten() {
            bits[usize::from(key) / 8] |= 1 << (key % 8);
        }

        assert!(bit_is_set(&bits, KEY_A));
        assert!(!bit_is_set(&bits, BTN_LEFT));
    }
}
