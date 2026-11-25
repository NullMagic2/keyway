<!--
Module: keyway.documentation.architecture
Purpose: Describe process boundaries, implementation status and build ownership.
Created: 2026-11-08
Adapted from: Sonic-SNES/ARCHITECTURE.md.
Please do not add other sections to this file unless otherwise requested, and only modify Overview if approved.
All source code files must be listed in the tree.
-->
# Architecture

## Overview

Keyway is a Linux keyboard language switcher that behaves like the Windows one: shortcuts
switch between layouts, Alt plus keypad digits type characters by code, and all of it is
customizable. It is a Rust 2024 workspace. keyway-router is a small sandboxed system
service that grabs physical keyboards with evdev and re-emits every key through one uinput
device, recognizing only shortcut chords and Alt code digits. keyway is the per-user agent
and command line tool; it receives those high-level events over a peer-checked Unix socket,
decodes Alt codes with the active layout's code pages and acts through a desktop backend.
keyway-settings is the GTK3 window that adds, removes and reorders layouts and edits the
configuration, opened from Keyway's indicator in the taskbar or the application list.
keyway-core holds shared logic as pure, testable code; keyway-desktop holds the desktop access
that the agent and settings window share.

The desktop stays in charge of layouts: all XKB layouts and variants from the system's
evdev.xml, plus IBus engines, remain ordinary desktop input sources, and Keyway never keeps
a second list. Adding or removing a layout in settings edits the desktop's list. Keyway replaces
the desktop's switcher: the extension puts its own indicator where the desktop's was, showing the
active layout with a menu of layouts and the settings window, and keeps the desktop's hidden for as
long as it is enabled, so that when it is not, as on GNOME's lock screen or after removal, the
original is back with nothing to restore. A package does this by itself: its session entry
(/etc/xdg/autostart/keyway.desktop) runs keyway session-start at each login, which enables the
extension the first time and starts the agent every time. A session already running keeps its
switcher until then, as a desktop only loads a new extension at login. An upgrade does not wait for
one, since the session already runs Keyway: keyway restart has the extension load its new files,
then restarts the agent and, once the system has asked for a password, the router. A GNOME Shell
imports an extension's code once per login, so extension.js there is only a small loader, which
imports a copy of adapter.js named by its checksum and does so again when asked; a change to the
loader itself still waits for a login. Cinnamon reloads an extension by itself. keyway remove keeps
one user on the desktop's switcher, with the standard Hidden entry of the same name in
~/.config/autostart.
The layout list is the same as the desktop's keyboard
settings panel, which the settings window links to, and it offers to turn off a system shortcut
on modifier keys that would switch layouts twice alongside Keyway's. On GNOME the extension also
makes the desktop's own switching shortcut, Super+Space unless changed, switch at once instead of
opening the Shell's popup; Cinnamon has no such popup. IBus engines' property menus and changing labels were part of the
desktop's switcher and Keyway's indicator does not offer them yet. GSettings can edit that list
but does not select the active source, so a thin extension for each desktop exposes activation,
text commit and change signals through one D-Bus interface, and one client serves them all. On
GNOME a decoded character is committed through the input method when the focused client accepts
text; other clients get key events, which can only type what the active layout has on a single
key. Cinnamon has no such input method on X11, but its virtual keyboard can borrow a key for any
character. Cinnamon 6.6 keeps layouts as input sources, and 6.2 and 6.4 as XKB groups in
libgnomekbd's settings, with at most four. KDE Plasma and wlroots compositors come later, with a
Keyway tray icon only where a desktop has no indicator of its own.

Status: every planned piece for GNOME and Cinnamon is written. The core's Windows expectations come from
published code pages and have not been confirmed on a Windows machine. The router passes on a
real kernel in a throwaway VM (tools/run_vm_tests.sh), the agent and extension in an isolated
headless Shell (tools/run_headless_tests.sh) and a sandboxed Cinnamon 6.4 and 6.6 on a private X
server (tools/run_cinnamon_tests.sh), all with a fake router, and the settings window on a private
X server with real input (tools/run_settings_tests.sh). For 6.6 those scripts ran inside a complete
Linux Mint 22.3 system, with the Mint package installed there by apt; a manual run there also saw
real logins through cinnamon-session, with systemctl stubbed: the package's session entry replaced
the applet with no setup, keyway remove kept the applet at the next login and keyway setup undid
that, and the settings window opened the real keyboard panel. On GNOME the session entry itself has
only run as a command, as no test runs gnome-session. Cinnamon 6.2 has not run; it was written from
its source code. tools/run_package_vm_tests.sh boots a complete system under real systemd and logind
and installs the packages, as a broken first revision then its upgrade: the sandboxed router starts
although /dev/uinput has an access list that denies its group, refuses a user without the active
session, and grabs and releases the VM's keyboard for one with it. A stand-in speaks for the agent
there, on systemd 255. The first install on a real Ubuntu 26.04 found both faults this covers. The
second revision then ran there under systemd 259 on GNOME Shell 50, with the extension loaded into
the running session through Looking Glass: Alt+Shift switched layouts, an Alt code typed its
character, and typing and the Print key worked as before. The indicator's country flags and codes
such as PT-BR, from data/layout_labels.json, have run only in the isolated GNOME Shell 50 and
sandboxed Cinnamon 6.4 and 6.6. So has keyway restart, with systemctl stubbed: it made each of them
run an extension changed on disk, and on GNOME also in a replay of a session that began with the
second revision's extension, after one Looking Glass line gave that Shell the loader. No package
has run on Debian, and no real login has yet started Keyway from the session entry. build.sh
compiles the workspace into bin/ with intermediates in .build/. tools/build_packages.sh builds
.deb packages into packages/ for releases with glibc 2.39 or newer and GNOME Shell 46 or
Cinnamon 6.2 or newer; only Shell 50 and Cinnamon 6.4 and 6.6 have been tested.
clean_source.sh asks for confirmation, then removes everything except bin/, packages/ (and .git unless
told otherwise), which is why runtime files are embedded in the binaries: keyway setup
installs them. Device access needs the planned udev rule and system service.

```text
keyway/
|-- README.md                       What Keyway is and how to install it
|-- ARCHITECTURE.md                 This overview
|-- STYLE_GUIDE.md                  Code, input safety and documentation rules
|-- LICENSE                         MIT license
|-- .gitignore                      Keep generated files out of version control
|-- build.sh                        Build the workspace and publish binaries to bin/
|-- clean_source.sh                 After confirmation, leave only bin/
|-- Cargo.toml / Cargo.lock         Workspace and pinned dependencies
|-- bin/                            (generated) keyway, keyway-router, keyway-settings
|-- packages/                       (generated) .deb packages from tools/build_packages.sh
|-- .build/                         (generated) Disposable Cargo intermediates
|-- crates/
|   |-- keyway-core/                Pure logic shared by every binary
|   |   |-- Cargo.toml
|   |   |-- build.rs                Embeds every flag in data/flags/
|   |   |-- src/
|   |   |   |-- lib.rs
|   |   |   |-- alt_code.rs         Alt plus keypad sequence state machine and decoding
|   |   |   |-- code_pages.rs       Embedded code page tables and layout to page mapping
|   |   |   |-- config.rs           Typed, validated configuration
|   |   |   |-- hotkey.rs           Shortcut parsing and chord state machine
|   |   |   |-- key_codes.rs        Key code, key event and modifier types
|   |   |   |-- key_interpreter.rs  One decision per key from both state machines
|   |   |   |-- keyboard_view.rs    Rows, widths and roles of the on-screen keyboard's keys
|   |   |   |-- language.rs         The languages of Keyway's own texts and their tags
|   |   |   |-- layout_id.rs        Validated input source identifier
|   |   |   |-- layout_labels.rs    Taskbar code and flag for each layout
|   |   |   `-- protocol.rs         Router and agent socket messages
|   |   `-- tests/
|   |       `-- windows_behavior.rs Whole key sequences through interpreter and decoder
|   |-- keyway-router/              Privileged key router service
|   |   |-- Cargo.toml
|   |   |-- src/
|   |   |   |-- lib.rs
|   |   |   |-- main.rs             Command line entry point
|   |   |   |-- devices.rs          Keyboard detection, safe grab and LED writes
|   |   |   |-- key_router.rs       Ordered output from device and agent events, with no I/O
|   |   |   |-- native.rs           The only unsafe code: evdev, uinput and sd-login
|   |   |   |-- router.rs           Event loop over devices, agent, signals and hotplug
|   |   |   |-- session.rs          Non-blocking query for the active session's user
|   |   |   |-- socket.rs           Peer-checked agent connection and message framing
|   |   |   `-- virtual_keyboard.rs uinput device, key output and LED input
|   |   `-- tests/
|   |       `-- device_e2e.rs       Real router on real kernel devices, in the VM only
|   |-- keyway-desktop/             Desktop access for the agent and settings
|   |   |-- Cargo.toml
|   |   `-- src/
|   |       |-- lib.rs
|   |       |-- command.rs          Run a system tool and report its failure
|   |       |-- config_file.rs      Find, load and atomically save the configuration
|   |       |-- desktop_shell.rs    D-Bus client for the desktop extensions
|   |       |-- layout_catalog.rs   Read layouts and variants from evdev.xml
|   |       |-- layout_names.rs     Layout names in a chosen language, from the system's catalogs
|   |       |-- restart.rs          Restart the extension, the agent and the router
|   |       `-- xkb.rs              Only unsafe code here: a layout's key labels and its AltGr
|   |-- keyway/                     User agent and command line tool
|   |   |-- Cargo.toml
|   |   `-- src/
|   |       |-- main.rs             run, status, next, previous, select, setup, session-start, restart, remove
|   |       |-- agent.rs            Router reports to layout changes and text, with no I/O
|   |       |-- run.rs              Event loop over router, desktop, config and signals
|   |       `-- setup.rs            Install and remove embedded runtime files
|   `-- keyway-settings/            GTK3 settings window
|       |-- Cargo.toml
|       |-- src/
|       |   |-- lib.rs
|       |   |-- main.rs             Opens the window
|       |   |-- window.rs           Header bar, state and what each control does
|       |   |-- layouts_page.rs     Reorder and remove layouts; options; restart Keyway
|       |   |-- add_layout_dialog.rs Searchable layout and variant picker
|       |   |-- load_extension_dialog.rs Steps and line for a Shell that restart cannot ask
|       |   |-- shortcuts_page.rs   Switching and direct layout shortcuts
|       |   |-- capture_dialog.rs   Press-to-set shortcut dialog
|       |   |-- shortcut_capture.rs Turns pressed keys into a shortcut, with no widgets
|       |   |-- alt_codes_page.rs   Alt code options and live code page examples
|       |   |-- input_sources.rs    Each desktop's layout list and shortcuts in GSettings
|       |   |-- source_list.rs      Rules for editing that list, with no I/O
|       |   |-- system_shortcuts.rs Recognizes the system's modifier-only shortcuts, with no I/O
|       |   |-- i18n.rs             English, Spanish, Portuguese and Greek interface text
|       |   |-- widgets.rs          Shared building blocks for the pages
|       |   `-- style.css           Card layout over the system theme's colors
|       `-- tests/
|           `-- settings_window.rs  Real window with real input, on a private X server
|-- extension/                      Desktop extensions, embedded by keyway setup
|   |-- keyboard.css                The on-screen keyboard's light and dark look, for both
|   |-- gnome/keyway@local/
|   |   |-- adapter.js              GNOME Shell: D-Bus adapter, indicator and on-screen keyboard
|   |   |-- extension.js            Loads adapter.js, and again when keyway restart asks
|   |   `-- metadata.json
|   `-- cinnamon/keyway@local/
|       |-- extension.js            Cinnamon: the same, for both layout systems
|       `-- metadata.json
|-- packaging/                      System integration files, embedded by keyway setup
|   |-- debian/                     Install and removal scripts for the .deb packages
|   |   |-- postinst
|   |   |-- postrm
|   |   `-- prerm
|   |-- 70-keyway.rules             udev: router access to uinput
|   |-- keyway-router.service       Sandboxed system service
|   |-- keyway.service              User service for the agent
|   |-- keyway-autostart.desktop    Starts that service in Cinnamon sessions, without a package
|   |-- keyway-session.desktop      A package's session entry, which turns Keyway on at login
|   |-- keyway-settings.desktop     Application list and taskbar menu entry point
|   `-- 30-keyway.quirks            libinput: keep touchpad disable-while-typing working
|-- data/
|   |-- config.example.toml         Every option with its default
|   |-- code_pages/                 Generated cp437.txt to cp1258.txt, 27 single-byte pages
|   |-- flags/                      Generated: flag-icons SVGs the layouts use, and their LICENSE
|   |-- layout_code_pages.toml      XKB layout to OEM and ANSI code page
|   `-- layout_labels.json          Generated: each XKB layout's code and flag
`-- tools/
    |-- build_packages.sh           Builds the Ubuntu, Debian and Mint .deb packages
    |-- generate_code_pages.py      Regenerates data/code_pages/ during development
    |-- generate_layout_labels.py   Regenerates data/layout_labels.json and data/flags/
    |-- headless_agent_test.py      Drives the agent and extension on either desktop
    |-- headless_test_window.py     Focused GTK client that reports what it receives
    |-- package_vm_scenario.py      Inside that VM: installs packages, tests the router
    |-- run_cinnamon_tests.sh       Runs that test on a sandboxed Cinnamon
    |-- run_headless_tests.sh       Runs that test in an isolated headless GNOME Shell
    |-- run_package_vm_tests.sh     Boots a whole system in a VM for the packages' test
    |-- run_settings_tests.sh       Tests and screenshots the window in each language
    |-- run_vm_tests.sh             Runs the router's device tests in a throwaway VM
    `-- script_common.sh            Project root, binary list and lock for both scripts
```
