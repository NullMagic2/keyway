//! Module: keyway_router::native
//! Purpose: The evdev and uinput ioctls, the kernel's input event byte layout, and sd-login.
//! Created: 2026-11-08
//! Architecture: The only module allowed unsafe code; everything else uses these safe
//! wrappers. Numbers match linux/input.h and linux/uinput.h.

#![allow(unsafe_code)]

use keyway_core::key_codes::{KeyAction, KeyCode, KeyEvent};
use nix::errno::Errno;
use std::ffi::{CStr, c_char, c_int};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::ptr;

const SYSTEMD_LIBRARY: &CStr = c"libsystemd.so.0";
const SEAT_GET_ACTIVE: &CStr = c"sd_seat_get_active";

type SeatGetActive =
    unsafe extern "C" fn(*const c_char, *mut *mut c_char, *mut libc::uid_t) -> c_int;

/// The user of a seat's active session, or None if it has none or the library is missing.
/// The library loads when needed, so building needs no systemd headers, and the static build
/// the device tests use still links; those tests trust a user instead of calling this.
pub fn seat_active_uid(seat: &CStr) -> Option<u32> {
    // SAFETY: both names are valid strings for these calls. The library stays loaded until the
    // dlclose below, past the last use of the function found in it, whose type matches the one
    // sd-login.h declares. It accepts a null session pointer, so it allocates nothing to free,
    // and it writes only to uid, which outlives the call.
    unsafe {
        let library = libc::dlopen(SYSTEMD_LIBRARY.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);

        if library.is_null() {
            return None;
        }

        let symbol = libc::dlsym(library, SEAT_GET_ACTIVE.as_ptr());
        let mut uid = 0;
        let found = !symbol.is_null() && {
            let get_active = std::mem::transmute::<*mut libc::c_void, SeatGetActive>(symbol);

            get_active(seat.as_ptr(), ptr::null_mut(), &mut uid) >= 0
        };

        libc::dlclose(library);

        found.then_some(uid)
    }
}

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_LED: u16 = 0x11;
pub const SYN_REPORT: u16 = 0;
pub const LED_NUML: u16 = 0;

const KEY_VALUE_RELEASE: i32 = 0;
const KEY_VALUE_PRESS: i32 = 1;
const KEY_VALUE_REPEAT: i32 = 2;

const BITS_PER_BYTE: usize = 8;
pub const EVENT_TYPE_BYTES: usize = libc::EV_MAX as usize / BITS_PER_BYTE + 1;
pub const KEY_BYTES: usize = libc::KEY_CNT.div_ceil(BITS_PER_BYTE);
const DEVICE_NAME_BYTES: usize = 256;
const READ_BATCH_EVENTS: usize = 64;

/// The size of the kernel's input_event, which begins with a timestamp whose width
/// depends on the platform. The type, code and value are always its last eight bytes.
pub const EVENT_SIZE: usize = size_of::<libc::input_event>();
const TYPE_OFFSET: usize = EVENT_SIZE - 8;
const CODE_OFFSET: usize = EVENT_SIZE - 6;
const VALUE_OFFSET: usize = EVENT_SIZE - 4;

nix::ioctl_read_buf!(eviocgname, b'E', 0x06, u8);
nix::ioctl_read_buf!(eviocgkey, b'E', 0x18, u8);
nix::ioctl_read_buf!(eviocgbit_types, b'E', 0x20, u8);
nix::ioctl_read_buf!(eviocgbit_keys, b'E', 0x21, u8);
nix::ioctl_write_int!(eviocgrab, b'E', 0x90);
nix::ioctl_none!(ui_dev_create, b'U', 1);
nix::ioctl_none!(ui_dev_destroy, b'U', 2);
nix::ioctl_write_ptr!(ui_dev_setup, b'U', 3, libc::uinput_setup);
nix::ioctl_write_int!(ui_set_evbit, b'U', 100);
nix::ioctl_write_int!(ui_set_keybit, b'U', 101);
nix::ioctl_write_int!(ui_set_ledbit, b'U', 105);

/// One input event as the kernel reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawEvent {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

impl RawEvent {
    pub const SYNC: Self = Self {
        kind: EV_SYN,
        code: SYN_REPORT,
        value: 0,
    };

    pub const fn from_key_event(event: KeyEvent) -> Self {
        let value = match event.action {
            KeyAction::Release => KEY_VALUE_RELEASE,
            KeyAction::Press => KEY_VALUE_PRESS,
            KeyAction::Repeat => KEY_VALUE_REPEAT,
        };

        Self {
            kind: EV_KEY,
            code: event.key.raw(),
            value,
        }
    }

    /// Returns the key event this carries, or None for other event types.
    pub const fn to_key_event(self) -> Option<KeyEvent> {
        if self.kind != EV_KEY {
            return None;
        }

        let action = match self.value {
            KEY_VALUE_RELEASE => KeyAction::Release,
            KEY_VALUE_PRESS => KeyAction::Press,
            KEY_VALUE_REPEAT => KeyAction::Repeat,
            _ => return None,
        };

        Some(KeyEvent {
            key: KeyCode::new(self.code),
            action,
        })
    }

    pub fn decode(bytes: &[u8; EVENT_SIZE]) -> Self {
        Self {
            kind: u16::from_ne_bytes([bytes[TYPE_OFFSET], bytes[TYPE_OFFSET + 1]]),
            code: u16::from_ne_bytes([bytes[CODE_OFFSET], bytes[CODE_OFFSET + 1]]),
            value: i32::from_ne_bytes([
                bytes[VALUE_OFFSET],
                bytes[VALUE_OFFSET + 1],
                bytes[VALUE_OFFSET + 2],
                bytes[VALUE_OFFSET + 3],
            ]),
        }
    }

    /// Encodes with a zero timestamp, which the kernel fills in when it delivers the event.
    pub fn encode(self) -> [u8; EVENT_SIZE] {
        let mut bytes = [0; EVENT_SIZE];
        bytes[TYPE_OFFSET..CODE_OFFSET].copy_from_slice(&self.kind.to_ne_bytes());
        bytes[CODE_OFFSET..VALUE_OFFSET].copy_from_slice(&self.code.to_ne_bytes());
        bytes[VALUE_OFFSET..].copy_from_slice(&self.value.to_ne_bytes());

        bytes
    }
}

/// Reads every event queued on a non-blocking device. An empty queue is not an error.
pub fn read_available_events(mut device: impl Read) -> io::Result<Vec<RawEvent>> {
    let mut events = Vec::new();
    let mut buffer = [0; EVENT_SIZE * READ_BATCH_EVENTS];

    loop {
        let bytes_read = match device.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(events),
            Err(error) => return Err(error),
        };

        if bytes_read == 0 {
            return Ok(events);
        }

        let (whole_events, _partial) = buffer[..bytes_read].as_chunks::<EVENT_SIZE>();
        events.extend(whole_events.iter().map(RawEvent::decode));
    }
}

pub const fn bit_is_set(bits: &[u8], bit: usize) -> bool {
    let index = bit / BITS_PER_BYTE;

    index < bits.len() && bits[index] & (1 << (bit % BITS_PER_BYTE)) != 0
}

pub fn device_name(device: BorrowedFd<'_>) -> io::Result<String> {
    let mut name = [0; DEVICE_NAME_BYTES];

    // SAFETY: device is an open descriptor for its whole borrow, and the ioctl writes at
    // most the slice's length into name, which outlives the call.
    unsafe { eviocgname(device.as_raw_fd(), &mut name) }.map_err(io::Error::from)?;

    let name_end = name.iter().position(|byte| *byte == 0);
    let name_bytes = &name[..name_end.unwrap_or(DEVICE_NAME_BYTES)];

    Ok(String::from_utf8_lossy(name_bytes).into_owned())
}

pub fn supported_event_types(device: BorrowedFd<'_>) -> io::Result<[u8; EVENT_TYPE_BYTES]> {
    let mut types = [0; EVENT_TYPE_BYTES];

    // SAFETY: device is open, and the ioctl writes at most the length of types.
    unsafe { eviocgbit_types(device.as_raw_fd(), &mut types) }.map_err(io::Error::from)?;

    Ok(types)
}

pub fn supported_keys(device: BorrowedFd<'_>) -> io::Result<[u8; KEY_BYTES]> {
    let mut keys = [0; KEY_BYTES];

    // SAFETY: device is open, and the ioctl writes at most the length of keys.
    unsafe { eviocgbit_keys(device.as_raw_fd(), &mut keys) }.map_err(io::Error::from)?;

    Ok(keys)
}

pub fn any_key_down(device: BorrowedFd<'_>) -> io::Result<bool> {
    let mut keys = [0_u8; KEY_BYTES];

    // SAFETY: device is open, and the ioctl writes at most the length of keys.
    unsafe { eviocgkey(device.as_raw_fd(), &mut keys) }.map_err(io::Error::from)?;

    Ok(keys.iter().any(|byte| *byte != 0))
}

/// Takes or releases exclusive access. Fails with EBUSY if another client holds the grab.
pub fn set_grabbed(device: BorrowedFd<'_>, grabbed: bool) -> Result<(), Errno> {
    let grab_value = nix::sys::ioctl::ioctl_param_type::from(grabbed);

    // SAFETY: device is open, and this ioctl passes its argument by value.
    unsafe { eviocgrab(device.as_raw_fd(), grab_value) }.map(|_| ())
}

pub fn enable_event_type(uinput: BorrowedFd<'_>, event_type: u16) -> io::Result<()> {
    let value = nix::sys::ioctl::ioctl_param_type::from(event_type);

    // SAFETY: uinput is open, and this ioctl passes its argument by value.
    unsafe { ui_set_evbit(uinput.as_raw_fd(), value) }.map_err(io::Error::from)?;

    Ok(())
}

pub fn enable_key(uinput: BorrowedFd<'_>, key: u16) -> io::Result<()> {
    let value = nix::sys::ioctl::ioctl_param_type::from(key);

    // SAFETY: uinput is open, and this ioctl passes its argument by value.
    unsafe { ui_set_keybit(uinput.as_raw_fd(), value) }.map_err(io::Error::from)?;

    Ok(())
}

pub fn enable_led(uinput: BorrowedFd<'_>, led: u16) -> io::Result<()> {
    let value = nix::sys::ioctl::ioctl_param_type::from(led);

    // SAFETY: uinput is open, and this ioctl passes its argument by value.
    unsafe { ui_set_ledbit(uinput.as_raw_fd(), value) }.map_err(io::Error::from)?;

    Ok(())
}

pub fn set_up_device(uinput: BorrowedFd<'_>, setup: &libc::uinput_setup) -> io::Result<()> {
    // SAFETY: uinput is open, and setup is a valid reference the kernel only reads from
    // during the call.
    unsafe { ui_dev_setup(uinput.as_raw_fd(), setup) }.map_err(io::Error::from)?;

    Ok(())
}

pub fn create_device(uinput: BorrowedFd<'_>) -> io::Result<()> {
    // SAFETY: uinput is open and this ioctl takes no argument.
    unsafe { ui_dev_create(uinput.as_raw_fd()) }.map_err(io::Error::from)?;

    Ok(())
}

pub fn destroy_device(uinput: BorrowedFd<'_>) -> io::Result<()> {
    // SAFETY: uinput is open and this ioctl takes no argument.
    unsafe { ui_dev_destroy(uinput.as_raw_fd()) }.map_err(io::Error::from)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_survive_encoding_and_keep_the_kernel_layout() {
        let event = RawEvent {
            kind: EV_KEY,
            code: 30,
            value: KEY_VALUE_PRESS,
        };
        let bytes = event.encode();

        assert_eq!(RawEvent::decode(&bytes), event);
        assert_eq!(&bytes[..TYPE_OFFSET], &[0; TYPE_OFFSET]);
    }

    #[test]
    fn key_events_convert_in_both_directions() {
        for event in [
            KeyEvent::press(KeyCode::A),
            KeyEvent::repeat(KeyCode::A),
            KeyEvent::release(KeyCode::A),
        ] {
            assert_eq!(RawEvent::from_key_event(event).to_key_event(), Some(event));
        }

        assert_eq!(RawEvent::SYNC.to_key_event(), None);
    }

    #[test]
    fn whole_events_are_read_until_the_stream_ends() {
        let first = RawEvent::from_key_event(KeyEvent::press(KeyCode::A));
        let second = RawEvent::SYNC;
        let stream: Vec<u8> = [first.encode(), second.encode()].concat();

        let events = read_available_events(stream.as_slice()).unwrap();

        assert_eq!(events, [first, second]);
    }

    #[test]
    fn a_seat_that_does_not_exist_has_no_active_user() {
        assert_eq!(seat_active_uid(c"keyway-no-such-seat"), None);
    }

    #[test]
    fn bits_are_read_from_the_right_byte() {
        let bits = [0b0000_0010, 0b0000_0001];

        assert!(bit_is_set(&bits, 1));
        assert!(bit_is_set(&bits, 8));
        assert!(!bit_is_set(&bits, 0));
        assert!(!bit_is_set(&bits, 200));
    }
}
