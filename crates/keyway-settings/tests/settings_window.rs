//! Module: keyway_settings::tests::settings_window
//! Purpose: Operate the real window with real X11 input, and capture screenshots of it.
//! Created: 2026-11-08
//! Architecture: Run through tools/run_settings_tests.sh, on a private X server with keyfile
//! settings. It skips itself anywhere else, as it edits the input source list.

use gtk::prelude::*;
use gtk::{gdk, glib};
use keyway_core::hotkey::Chord;
use keyway_desktop::restart::looking_glass_line;
use keyway_settings::i18n::{Language, Message, text};
use keyway_settings::shortcuts_page::ShortcutSlot;
use keyway_settings::source_list::SourceEntry;
use keyway_settings::window::{PAGE_ALT_CODES, PAGE_SHORTCUTS, SettingsApp};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use zbus::zvariant::Value;

const GUARD_VARIABLE: &str = "KEYWAY_SETTINGS_TEST";
const PANEL_MARKER_VARIABLE: &str = "KEYWAY_PANEL_MARKER";
const SYSTEMCTL_DIRECTORY_VARIABLE: &str = "KEYWAY_SYSTEMCTL_DIR";
const SYSTEMCTL_LOG: &str = "log";
const SYSTEMCTL_HOLD: &str = "hold";
const SYSTEMCTL_REFUSE: &str = "refuse";
const RESTART_CALLS: [&str; 3] = [
    "--user daemon-reload",
    "--user restart keyway.service",
    "restart keyway-router.service",
];
const SHELL_NAME: &str = "org.gnome.Shell";
const SHELL_PATH: &str = "/org/gnome/Shell";
const LOADER_PATH: &str = "/org/keyway/Loader";

/// The Shell's state for an extension that is turned on.
const EXTENSION_ACTIVE: f64 = 1.0;
const CURRENT_DESKTOP_VARIABLE: &str = "XDG_CURRENT_DESKTOP";
const CINNAMON_DESKTOP: &str = "X-Cinnamon";
const CINNAMON_INPUT_SOURCE_MANAGER: &str = "/usr/share/cinnamon/js/ui/keyboardManager.js";
const SCREENSHOT_VARIABLE: &str = "KEYWAY_SCREENSHOT_DIR";
const WAIT_LIMIT: Duration = Duration::from_secs(10);
const SETTLE_TIME: Duration = Duration::from_millis(250);
const LOOP_SLICE: Duration = Duration::from_millis(5);

fn pump_for(duration: Duration) {
    let start = Instant::now();

    while start.elapsed() < duration {
        while gtk::events_pending() {
            gtk::main_iteration();
        }

        std::thread::sleep(LOOP_SLICE);
    }
}

fn settle() {
    pump_for(SETTLE_TIME);
}

fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let start = Instant::now();

    while !condition() {
        assert!(start.elapsed() < WAIT_LIMIT, "timed out waiting for {what}");
        pump_for(LOOP_SLICE);
    }

    println!("ok   {what}");
}

fn check(what: &str, passed: bool) {
    assert!(passed, "failed: {what}");
    println!("ok   {what}");
}

fn xdotool(arguments: &[&str]) {
    let status = Command::new("xdotool").args(arguments).status().unwrap();

    assert!(status.success(), "xdotool {arguments:?} failed");
}

/// Clicks a widget where it really is on screen, with a real pointer event.
fn click(widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    settle();

    let gdk_window = widget.window().expect("widget is not realized");
    let (_result, origin_x, origin_y) = gdk_window.origin();
    let allocation = widget.allocation();
    let x = origin_x + allocation.x() + allocation.width() / 2;
    let y = origin_y + allocation.y() + allocation.height() / 2;

    xdotool(&["mousemove", &x.to_string(), &y.to_string()]);
    settle();
    xdotool(&["click", "1"]);
    settle();
}

fn press_keys(keys: &str) {
    settle();
    xdotool(&["key", keys]);
    settle();
}

fn point_at(window: &impl IsA<gtk::Widget>) {
    let window = window.as_ref();
    settle();

    let gdk_window = window.window().expect("window is not realized");
    let (_result, origin_x, origin_y) = gdk_window.origin();
    let x = origin_x + window.allocated_width() / 2;
    let y = origin_y + window.allocated_height() / 2;

    xdotool(&["mousemove", &x.to_string(), &y.to_string()]);
    settle();
}

fn save_screenshot(window: &impl IsA<gtk::Widget>, name: &str) {
    let Some(directory) = env::var_os(SCREENSHOT_VARIABLE) else {
        return;
    };

    settle();

    let window = window.as_ref();
    let gdk_window = window.window().expect("window is not realized");
    let width = gdk_window.width();
    let height = gdk_window.height();
    let pixbuf = gdk_window
        .pixbuf(0, 0, width, height)
        .expect("could not capture the window");
    let path = PathBuf::from(directory).join(format!("{name}.png"));

    pixbuf.savev(&path, "png", &[]).unwrap();
}

/// Finds the dialog that a real click on a shortcut button opened.
fn visible_dialog() -> gtk::Dialog {
    settle();

    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|toplevel| toplevel.downcast::<gtk::Dialog>().ok())
        .find(|dialog| dialog.is_visible())
        .expect("no dialog is open")
}

fn source_ids(app: &SettingsApp) -> Vec<String> {
    app.sources().into_iter().map(|entry| entry.id).collect()
}

fn slot_button(app: &SettingsApp, slot: &ShortcutSlot) -> gtk::Button {
    app.shortcut_buttons()
        .into_iter()
        .find(|shortcut| shortcut.slot == *slot)
        .expect("no button for that shortcut")
        .button
}

fn test_layouts(app: &Rc<SettingsApp>, config_path: &Path) {
    check(
        "the desktop's layouts are listed",
        source_ids(app) == ["br", "gr+simple"],
    );

    app.set_active_source(Some("br".to_string()));
    settle();

    let rows = app.layout_rows();
    check(
        "the active layout is marked",
        rows[0].in_use && !rows[1].in_use,
    );
    check(
        "the first layout cannot move up",
        !rows[0].up.is_sensitive(),
    );
    check(
        "the last layout cannot move down",
        !rows[1].down.is_sensitive(),
    );

    save_screenshot(&app.window, "layouts");

    click(&app.layouts.per_window);
    wait_until("the per-window option reaches the desktop", || {
        env::var_os("XDG_CONFIG_HOME").is_some_and(|directory| {
            let keyfile = PathBuf::from(directory).join("glib-2.0/settings/keyfile");

            std::fs::read_to_string(keyfile).is_ok_and(|text| text.contains("per-window=true"))
        })
    });
    click(&app.layouts.per_window);

    click(&rows[0].down);
    wait_until("moving down reorders the desktop's list", || {
        source_ids(app) == ["gr+simple", "br"]
    });

    click(&app.layout_rows()[1].up);
    wait_until("moving up reorders it back", || {
        source_ids(app) == ["br", "gr+simple"]
    });

    let dialog = SettingsApp::open_add_layout(app).expect("dialog did not open");
    settle();
    check(
        "Add needs a selection first",
        !dialog.add_button.is_sensitive(),
    );

    dialog.search.set_text("english (us)");
    pump_for(Duration::from_millis(400));

    let visible_row = dialog
        .list
        .children()
        .into_iter()
        .find(|row| row.is_child_visible())
        .expect("the search found nothing");
    save_screenshot(&dialog.dialog, "add_layout");

    click(&visible_row);
    check(
        "selecting a layout enables Add",
        dialog.add_button.is_sensitive(),
    );
    click(&dialog.add_button);
    wait_until(
        "adding puts the layout at the end of the desktop's list",
        || app.sources().last() == Some(&SourceEntry::xkb("us")),
    );

    click(&app.layout_rows()[2].remove);
    wait_until("removing takes it off the desktop's list", || {
        source_ids(app) == ["br", "gr+simple"]
    });

    click(&app.layout_rows()[1].remove);
    wait_until("removing down to one layout works", || {
        source_ids(app) == ["br"]
    });
    check(
        "the only layout cannot be removed",
        !app.layout_rows()[0].remove.is_sensitive(),
    );

    let original_sources = [SourceEntry::xkb("br"), SourceEntry::xkb("gr+simple")];
    let dialog = SettingsApp::open_add_layout(app).expect("dialog did not open");
    settle();
    dialog.search.set_text("gr+simple");
    pump_for(Duration::from_millis(400));
    let greek_row = dialog
        .list
        .children()
        .into_iter()
        .find(|row| row.is_child_visible())
        .expect("the search found nothing");
    click(&greek_row);
    click(&dialog.add_button);
    wait_until("the layout can be added back", || {
        app.sources() == original_sources
    });

    check(
        "layout changes never touch the configuration file",
        !config_path.exists(),
    );

    check(
        "the on-screen keyboard is light unless chosen otherwise",
        app.layouts.keyboard_theme.active_id().as_deref() == Some("light"),
    );
    app.layouts.keyboard_theme.set_active_id(Some("dark"));
    wait_until("the keyboard's colours are saved", || {
        std::fs::read_to_string(config_path).is_ok_and(|text| text.contains("theme = \"dark\""))
    });
}

fn test_shortcuts(app: &Rc<SettingsApp>, config_path: &Path) {
    app.stack.set_visible_child_name(PAGE_SHORTCUTS);
    settle();
    save_screenshot(&app.window, "shortcuts");

    click(&slot_button(app, &ShortcutSlot::Next));
    let dialog = visible_dialog();
    point_at(&dialog);
    save_screenshot(&dialog, "capture");

    press_keys("ctrl+shift");
    wait_until("real keys set a modifier-only shortcut", || {
        app.config().switching.next == Chord::parse("Ctrl+Shift").unwrap()
    });
    wait_until("the capture dialog closes", || !dialog.is_visible());
    wait_until("the shortcut is saved for the agent", || {
        std::fs::read_to_string(config_path).is_ok_and(|text| text.contains("Ctrl+Shift"))
    });

    let previous_dialog = SettingsApp::open_capture(app, ShortcutSlot::Previous);
    point_at(&previous_dialog.dialog);
    press_keys("x");
    check(
        "a typing key alone is refused, and says why",
        previous_dialog.dialog.is_visible()
            && previous_dialog.message_text() == text(app.language, Message::NeedsMoreKeys),
    );
    save_screenshot(&previous_dialog.dialog, "capture_refused");

    press_keys("ctrl+shift");
    check(
        "a shortcut in use elsewhere is refused",
        previous_dialog.dialog.is_visible()
            && previous_dialog.message_text() == text(app.language, Message::ShortcutInUse),
    );

    press_keys("super+space");
    wait_until("a keyed shortcut is set", || {
        app.config().switching.previous == Some(Chord::parse("Super+Space").unwrap())
    });

    let clear_dialog = SettingsApp::open_capture(app, ShortcutSlot::Previous);
    let clear_button = clear_dialog.clear_button.clone().expect("no clear button");
    click(&clear_button);
    wait_until("an optional shortcut can be cleared", || {
        app.config().switching.previous.is_none()
    });

    let cancelled = SettingsApp::open_capture(app, ShortcutSlot::Previous);
    point_at(&cancelled.dialog);
    press_keys("Escape");
    wait_until("Esc cancels without changing anything", || {
        !cancelled.dialog.is_visible() && app.config().switching.previous.is_none()
    });

    let next_only = SettingsApp::open_capture(app, ShortcutSlot::Next);
    check(
        "the next-layout shortcut cannot be cleared",
        next_only.clear_button.is_none(),
    );
    point_at(&next_only.dialog);
    press_keys("alt+shift");
    wait_until("the default shortcut can be set again", || {
        app.config().switching.next == Chord::ALT_SHIFT
    });

    let layout = "gr+simple".to_string().try_into().unwrap();
    let direct = SettingsApp::open_capture(app, ShortcutSlot::Direct(layout));
    point_at(&direct.dialog);
    press_keys("ctrl+shift+exclam");
    wait_until("a direct layout shortcut is set", || {
        app.config().switching.direct.values().next()
            == Some(&Chord::parse("Ctrl+Shift+1").unwrap())
    });
    save_screenshot(&app.window, "shortcuts_set");

    app.shortcuts.order.set_active_id(Some("recent"));
    wait_until("the switching order is saved", || {
        std::fs::read_to_string(config_path).is_ok_and(|text| text.contains("recent"))
    });
}

fn test_alt_codes(app: &Rc<SettingsApp>, config_path: &Path) {
    app.stack.set_visible_child_name(PAGE_ALT_CODES);
    settle();
    save_screenshot(&app.window, "alt_codes");

    click(&app.alt_codes.hex_numpad);
    wait_until("the hex option is saved", || {
        std::fs::read_to_string(config_path).is_ok_and(|text| text.contains("hex_numpad = true"))
    });

    click(&app.alt_codes.enabled);
    wait_until("Alt codes can be turned off", || {
        !app.config().alt_codes.enabled
    });

    app.alt_codes.numlock.set_active_id(Some("required"));
    wait_until("the NumLock option is saved", || {
        std::fs::read_to_string(config_path).is_ok_and(|text| text.contains("\"required\""))
    });

    let saved = keyway_desktop::config_file::load_config(config_path).unwrap();
    check(
        "the saved file loads back to the same settings",
        saved == app.config(),
    );
}

fn gsettings_set(schema: &str, key: &str, value: &str) {
    let status = Command::new("gsettings")
        .args(["set", schema, key, value])
        .status()
        .unwrap();

    assert!(status.success(), "gsettings set {schema} {key} failed");
}

/// The same check on either desktop: what its settings look like differs, the behavior not.
struct SystemOptions {
    schema: &'static str,
    key: &'static str,
    led_only: &'static str,
    with_switching: &'static str,
}

const GNOME_OPTIONS: SystemOptions = SystemOptions {
    schema: "org.gnome.desktop.input-sources",
    key: "xkb-options",
    led_only: "['grp_led:scroll']",
    with_switching: "['grp_led:scroll', 'grp:alt_shift_toggle']",
};

const CINNAMON_INPUT_SOURCE_OPTIONS: SystemOptions = SystemOptions {
    schema: "org.cinnamon.desktop.input-sources",
    ..GNOME_OPTIONS
};

const CINNAMON_XKB_GROUP_OPTIONS: SystemOptions = SystemOptions {
    schema: "org.gnome.libgnomekbd.keyboard",
    key: "options",
    led_only: "['grp_led\\tgrp_led:scroll']",
    with_switching: "['grp_led\\tgrp_led:scroll', 'grp\\tgrp:alt_shift_toggle']",
};

fn option_list(options: &SystemOptions) -> String {
    let output = Command::new("gsettings")
        .args(["get", options.schema, options.key])
        .output()
        .unwrap();

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn test_system_integration(app: &Rc<SettingsApp>) {
    let is_cinnamon = env::var(CURRENT_DESKTOP_VARIABLE).as_deref() == Ok(CINNAMON_DESKTOP);
    let has_input_sources = !is_cinnamon || Path::new(CINNAMON_INPUT_SOURCE_MANAGER).exists();
    let (options, switch_bindings) = match (is_cinnamon, has_input_sources) {
        (false, _) => (GNOME_OPTIONS, Some("org.gnome.desktop.wm.keybindings")),
        (true, true) => (
            CINNAMON_INPUT_SOURCE_OPTIONS,
            Some("org.cinnamon.desktop.keybindings.wm"),
        ),
        (true, false) => (CINNAMON_XKB_GROUP_OPTIONS, None),
    };

    gsettings_set(options.schema, options.key, options.led_only);
    settle();
    check(
        "an LED option is not a switching shortcut",
        !app.conflict_banner.is_visible(),
    );

    gsettings_set(options.schema, options.key, options.with_switching);
    wait_until("the system's Alt+Shift option is noticed", || {
        app.conflict_banner.is_visible()
    });
    save_screenshot(&app.window, "system_conflict");

    click(&app.use_only_keyway);
    wait_until("Use only Keyway turns it off", || {
        !app.conflict_banner.is_visible()
    });
    check(
        "every other option stays",
        option_list(&options).contains("grp_led") && !option_list(&options).contains("toggle"),
    );

    if let Some(schema) = switch_bindings {
        let key = "switch-input-source";

        gsettings_set(schema, key, "['<Super>space', '<Alt>Shift_L']");
        wait_until("a modifier-only system binding is noticed", || {
            app.conflict_banner.is_visible()
        });
        click(&app.use_only_keyway);
        wait_until("that binding is removed too", || {
            !app.conflict_banner.is_visible()
        });

        let output = Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .unwrap();

        check(
            "the system's Super+Space stays",
            String::from_utf8_lossy(&output.stdout).trim() == "['<Super>space']",
        );
    }

    let panel_button = app
        .layouts
        .system_keyboard
        .clone()
        .expect("the desktop's keyboard panel is offered");
    let marker = PathBuf::from(env::var_os(PANEL_MARKER_VARIABLE).unwrap());

    click(&panel_button);
    wait_until("the button opens the desktop's keyboard panel", || {
        marker.exists()
    });
    check(
        "the panel opens on its layout list",
        std::fs::read_to_string(&marker).unwrap().trim()
            == if is_cinnamon { "-t layouts" } else { "" },
    );

    if is_cinnamon && has_input_sources {
        gsettings_set(
            "org.cinnamon.desktop.input-sources",
            "sources",
            "[('xkb', 'br'), ('xkb', 'gr+simple'), ('xkb', 'us'), ('xkb', 'de'), ('xkb', 'fr')]",
        );
        wait_until("input sources have no four-layout limit", || {
            source_ids(app).len() == 5 && app.layouts.add_button.is_sensitive()
        });
        gsettings_set(
            "org.cinnamon.desktop.input-sources",
            "sources",
            "[('xkb', 'br'), ('xkb', 'gr+simple')]",
        );
        wait_until("the layouts are back", || {
            source_ids(app) == ["br", "gr+simple"]
        });
    }

    if is_cinnamon && !has_input_sources {
        gsettings_set(
            "org.gnome.libgnomekbd.keyboard",
            "layouts",
            "['br', 'gr\\tsimple', 'us', 'de']",
        );
        wait_until("four layouts are the most an X server holds", || {
            !app.layouts.add_button.is_sensitive() && app.layouts.add_button.has_tooltip()
        });
        save_screenshot(&app.window, "layout_limit");

        gsettings_set("org.gnome.libgnomekbd.keyboard", "layouts", "@as []");
        wait_until("an empty list shows the X server's layouts", || {
            !source_ids(app).is_empty() && app.layouts.add_button.is_sensitive()
        });

        gsettings_set(
            "org.gnome.libgnomekbd.keyboard",
            "layouts",
            "['br', 'gr\\tsimple']",
        );
        wait_until("the layouts are back", || {
            source_ids(app) == ["br", "gr+simple"]
        });
    }
}

/// Stands in for the loader that Keyway's extension exports in GNOME Shell.
struct FakeLoader {
    reloads: Arc<AtomicUsize>,
}

#[zbus::interface(name = "org.keyway.Loader")]
impl FakeLoader {
    fn reload(&self) {
        self.reloads.fetch_add(1, Ordering::SeqCst);
    }
}

/// Stands in for GNOME Shell's own word on an extension: here, that it is turned on.
struct FakeExtensions;

#[zbus::interface(name = "org.gnome.Shell.Extensions")]
impl FakeExtensions {
    fn get_extension_info(&self, _uuid: &str) -> HashMap<String, Value<'static>> {
        HashMap::from([("state".to_string(), Value::from(EXTENSION_ACTIVE))])
    }
}

fn test_restart(app: &Rc<SettingsApp>) {
    let directory = PathBuf::from(
        env::var_os(SYSTEMCTL_DIRECTORY_VARIABLE).expect("no stand-in for systemctl"),
    );
    let hold = directory.join(SYSTEMCTL_HOLD);
    let refuse = directory.join(SYSTEMCTL_REFUSE);
    let calls = || {
        let log = std::fs::read_to_string(directory.join(SYSTEMCTL_LOG)).unwrap_or_default();

        log.lines().map(str::to_string).collect::<Vec<String>>()
    };
    let restart = &app.layouts.restart;
    let usual_note = restart.note.text();
    let names = |message: Message| restart.note.text().contains(text(app.language, message));

    // The real systemctl would restart the services of whoever runs this test.
    check(
        "systemctl is only a stand-in",
        env::var("PATH").is_ok_and(|path| path.split(':').next() == directory.to_str()),
    );

    click(&restart.button);
    wait_until("a desktop that cannot be reached is reported", || {
        restart.problem.is_visible() && names(Message::PartExtension)
    });
    check(
        "the agent and the router are restarted all the same",
        calls() == RESTART_CALLS && !names(Message::PartAgent) && !names(Message::PartRouter),
    );
    check(
        "nothing is confirmed, and no warning covers the page",
        !restart.done.is_visible() && !app.banner.is_visible(),
    );
    save_screenshot(&app.window, "restart_unreachable");

    let reloads = Arc::new(AtomicUsize::new(0));
    let loader = FakeLoader {
        reloads: Arc::clone(&reloads),
    };
    let shell = zbus::blocking::connection::Builder::session()
        .and_then(|builder| builder.name(SHELL_NAME))
        .and_then(|builder| builder.serve_at(LOADER_PATH, loader))
        .and_then(|builder| builder.serve_at(SHELL_PATH, FakeExtensions))
        .and_then(zbus::blocking::connection::Builder::build)
        .unwrap();

    std::fs::write(&hold, "").unwrap();
    click(&restart.button);
    wait_until("a restart shows that it is under way", || {
        restart.spinner.is_visible() && !restart.button.is_sensitive()
    });
    check(
        "and takes the earlier report away",
        !restart.problem.is_visible() && restart.note.text() == usual_note,
    );
    save_screenshot(&app.window, "restart_running");

    std::fs::remove_file(&hold).unwrap();
    wait_until("a finished restart is confirmed", || {
        restart.done.is_visible() && !restart.spinner.is_visible()
    });
    check(
        "the extension was asked to load again",
        reloads.load(Ordering::SeqCst) == 1,
    );
    check(
        "the agent and the router were restarted",
        calls().ends_with(&RESTART_CALLS.map(str::to_string)),
    );
    check(
        "nothing else is reported",
        !restart.problem.is_visible() && app.load_extension_dialog().is_none(),
    );
    save_screenshot(&app.window, "restart_done");

    std::fs::write(&refuse, "").unwrap();
    click(&restart.button);
    wait_until("a refused password is reported", || {
        restart.problem.is_visible() && names(Message::PartRouter)
    });
    check(
        "only the router is named, and nothing is confirmed",
        !names(Message::PartExtension) && !names(Message::PartAgent) && !restart.done.is_visible(),
    );
    check("the button can try again", restart.button.is_sensitive());
    save_screenshot(&app.window, "restart_refused");
    std::fs::remove_file(&refuse).unwrap();

    shell
        .object_server()
        .remove::<FakeLoader, _>(LOADER_PATH)
        .unwrap();
    click(&restart.button);
    wait_until("a Shell without the loader gets the steps instead", || {
        app.load_extension_dialog()
            .is_some_and(|steps| steps.dialog.is_visible())
    });

    let steps = app.load_extension_dialog().unwrap();
    let line = looking_glass_line();

    check(
        "with nothing else to report",
        !restart.problem.is_visible() && !restart.done.is_visible(),
    );
    check("the steps show the line", steps.line.text() == line);
    click(&steps.copy_button);
    wait_until("the line can be copied", || {
        gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD)
            .wait_for_text()
            .is_some_and(|copied| copied == line)
    });
    check(
        "and the button says so",
        steps.copy_button.label().as_deref() == Some(text(app.language, Message::LineCopied)),
    );
    save_screenshot(&steps.dialog, "load_extension");
    steps.dialog.close();
    settle();
}

fn test_invalid_config(config_path: &Path) {
    std::fs::write(config_path, "this is not valid\n").unwrap();

    let app = SettingsApp::new(Language::from_environment()).unwrap();
    app.window.show_all();
    settle();
    check("an invalid file is reported", app.banner.is_visible());
    save_screenshot(&app.window, "invalid_config");

    app.stack.set_visible_child_name(PAGE_ALT_CODES);
    settle();
    click(&app.alt_codes.hex_numpad);
    wait_until("changing a setting writes a valid file", || {
        keyway_desktop::config_file::load_config(config_path).is_ok()
    });
    check(
        "the invalid file is kept as a copy",
        std::fs::read_to_string(config_path.with_extension("toml.invalid"))
            .is_ok_and(|text| text.contains("not valid")),
    );

    app.window.close();
    settle();
}

fn main() {
    let is_isolated = env::var_os(GUARD_VARIABLE).is_some()
        && env::var("GSETTINGS_BACKEND").as_deref() == Ok("keyfile");

    // A plain cargo test ends up here too, and this test edits the input source list.
    if !is_isolated {
        println!("skipped: run tools/run_settings_tests.sh instead");

        return;
    }

    gtk::init().unwrap();
    glib::set_prgname(Some("keyway-settings-test"));

    let config_directory = keyway_desktop::config_file::config_directory().unwrap();
    let config_path = config_directory.join(keyway_desktop::config_file::CONFIG_FILE_NAME);
    let app = SettingsApp::new(Language::from_environment()).unwrap();

    app.window.show_all();
    check(
        "no warning shows without a reason",
        !app.banner.is_visible(),
    );
    settle();

    test_layouts(&app, &config_path);
    test_system_integration(&app);
    test_restart(&app);
    test_shortcuts(&app, &config_path);
    test_alt_codes(&app, &config_path);

    app.window.close();
    settle();

    test_invalid_config(&config_path);

    println!("ALL SETTINGS TESTS PASSED");
}
