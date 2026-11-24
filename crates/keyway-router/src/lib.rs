//! Module: keyway_router
//! Purpose: Grab physical keyboards and route their keys through one virtual keyboard.
//! Created: 2026-11-08
//! Architecture: A library so the device tests can reuse the uinput and evdev code;
//! main.rs only parses the command line. Unsafe code is confined to the native module.

#![deny(unsafe_code)]

pub mod devices;
pub mod key_router;
pub mod native;
pub mod router;
pub mod session;
pub mod socket;
pub mod virtual_keyboard;
