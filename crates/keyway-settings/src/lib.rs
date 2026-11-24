//! Module: keyway_settings
//! Purpose: The settings window: layouts, shortcuts and Alt code options.
//! Created: 2026-11-08
//! Architecture: A library so the window test can drive real widgets; main.rs only opens
//! the window. Logic that needs no widgets lives in i18n, shortcut_capture and source_list.

#![deny(unsafe_code)]

pub mod add_layout_dialog;
pub mod alt_codes_page;
pub mod capture_dialog;
pub mod i18n;
pub mod input_sources;
pub mod layouts_page;
pub mod load_extension_dialog;
pub mod shortcut_capture;
pub mod shortcuts_page;
pub mod source_list;
pub mod system_shortcuts;
pub mod widgets;
pub mod window;
