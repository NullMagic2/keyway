//! Module: keyway_settings::main
//! Purpose: Open the settings window and run until it closes.
//! Created: 2026-11-08
//! Architecture: Thin entry point; window.rs builds everything.

use gtk::prelude::*;
use keyway_settings::i18n::Language;
use keyway_settings::window::SettingsApp;
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(error) = gtk::init() {
        eprintln!("keyway-settings: {error}");

        return ExitCode::FAILURE;
    }

    let app = match SettingsApp::new(Language::from_environment()) {
        Ok(app) => app,
        Err(error) => {
            eprintln!("keyway-settings: {error}");

            return ExitCode::FAILURE;
        }
    };

    app.window().connect_destroy(|_window| gtk::main_quit());
    app.start_watching_desktop();
    app.window().show_all();
    gtk::main();

    ExitCode::SUCCESS
}
