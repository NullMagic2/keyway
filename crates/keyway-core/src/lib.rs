//! Module: keyway_core
//! Purpose: Shortcut, Alt code, code page, configuration and protocol logic.
//! Created: 2026-11-08
//! Architecture: Pure library shared by the router, agent and settings binaries;
//! it performs no device, socket, D-Bus or file access and never reads the clock.

#![deny(unsafe_code)]

pub mod alt_code;
pub mod code_pages;
pub mod config;
pub mod hotkey;
pub mod key_codes;
pub mod key_interpreter;
pub mod layout_id;
pub mod layout_labels;
pub mod protocol;
