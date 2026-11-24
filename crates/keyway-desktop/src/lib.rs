//! Module: keyway_desktop
//! Purpose: Reach the running desktop: its input sources, text commit and keymaps.
//! Created: 2026-11-08
//! Architecture: Each desktop has an extension with the same D-Bus interface, so one client
//! serves them all with no backend trait.
//! Unsafe code is confined to the xkb module.

#![deny(unsafe_code)]

pub mod command;
pub mod config_file;
pub mod desktop_shell;
pub mod layout_catalog;
pub mod restart;
pub mod xkb;
