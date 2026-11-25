<!--
Module: keyway.documentation.style_guide
Purpose: Keep the keyboard switcher readable, reusable and safe around live input.
Created: 2026-11-08
Architecture: ARCHITECTURE.md owns subsystem boundaries and implementation status.
Adapted from: Sonic-SNES/STYLE_GUIDE.md and Clearwave/STYLE_GUIDE.md.
-->

# Keyway style guide

Write code that another person can follow without knowing the development
session. Prefer clear contracts, descriptive names and ordinary control flow.
These rules apply to authored source, tools, tests and documentation.

Do not keep "legacy / historical" code under any circumstances, because this
encourages clutter. Keep structure identical to how a human would structure code,
and group files under similar semantic units.

Subagents, always work under the current source version. Do not fork it; instead,
coordinate between yourselves which files you are working on using an agents.txt
file. This file must be deleted after the subagent closes.
DO NOT duplicate work. Before doing anything, check if anything close to what you
want to do already exists in the source and reuse it. Remove any duplicate code if
it exists.
DO NOT do A/B testing. Test only what is necessary for the code to run, and avoid
very improbable scenarios unless expressly requested.

## Language and toolchain

All crates use Rust 2024 (`edition = "2024"`) through one Cargo workspace. Run
`cargo fmt --all`, `cargo clippy --all-targets -- -D warnings` and
`cargo test --offline --locked` before delivery. Pin every dependency to an exact
version and commit Cargo.lock. Add a dependency only when the standard library or
existing workspace code cannot reasonably do the job.

The only runtime code outside Rust is the desktop extensions, because the GNOME Shell
and Cinnamon APIs are reachable only from JavaScript inside each desktop; scripts under
tools/ run only during development. Keep each extension a thin adapter with no policy,
offering the same D-Bus interface: it activates a source, commits text, reports changes,
adds a settings entry and Keyway's languages to the taskbar menu, passing a chosen language on
for the agent to save, and draws the on-screen keyboard as the agent lays it out, pressing the
keys the user clicks.

## Reusable modules and clear boundaries

Give each module one responsibility and a small public interface. keyway-core is
pure logic with no device, socket, D-Bus or file access, so every shortcut and
Alt code rule is testable without a desktop. keyway-desktop holds the desktop access
that the agent and settings window share. Code used by more than one binary belongs
in one of these two crates.

Pass state and configuration explicitly. Avoid mutable global state. Share behavior
when the contract is the same, but avoid generic frameworks for one caller: add a
trait only when a second implementation exists. Remove unused private code rather
than suppressing dead-code warnings. Keep CLI dispatch and platform callbacks thin.

Desktops own their layout list and active layout. Never keep a second authoritative
copy; read them from the backend and react to its change signal.

## Reusable, human-readable data types

Define shared types in their owning module. Use named fields when tuple positions
would be unclear. Distinguish evdev key codes, XKB keysyms, Unicode scalars, code
page bytes and layout indices with separate types, not bare integers.

Document ownership and constructor invariants. Validate untrusted values before
constructing internal types. Report unsupported values explicitly; never replace bad
configuration with a plausible-looking success. Reject unknown configuration keys.

Avoid magic numbers. Name limits, timeouts, key codes and policy constants in the
module that owns them, and prefer existing platform constants for operating-system
values. Zero/one identities and ordinary loop initialization are exempt. Give
deliberate test fixtures names.

## Human-readable files and formatting

Use descriptive snake_case functions and variables, PascalCase types and
SCREAMING_SNAKE_CASE constants. Avoid minified functions, multiple unrelated
statements on one line, clever iterator chains and single-letter names outside
familiar math. Prefer early errors to deeply nested success paths.

Leave a blank line between a declaration block and the adjacent if, match, for,
while or loop. Keep related declarations together, and separate declaration blocks
that serve different purposes. Place explanatory comments beside the decision.
Split functions when separate responsibilities emerge, not to satisfy line counts.

Model key handling as explicit state machines, with time passed in as a value. Do
not read the wall clock inside keyway-core.

## Module comments and file headers

Start handwritten files with module name, purpose, creation day and architectural
relationship. Rust uses a leading //! block; scripts and JavaScript use a leading
comment block. Module headers are at most 10 lines. Short explanatory comment
blocks are at most 3 lines. Preserve a known creation date. Generated files
identify their generator.

## Function comments and API documentation

Document public inputs, outputs, failures and invariants. Comments explain why,
rather than narrating the syntax. Comments must not claim planned behavior is
implemented.

Default to `deny(unsafe_code)`. Allow unsafe only at a named native boundary, and
give every unsafe block a safety explanation covering pointer validity, bounds,
ownership and lifetime.

## Plain comment text

Do not use backticks in code comments or docstrings. Identifiers and paths remain
plain text there. Normal Markdown prose may use code formatting.

## Creating documentation files

AI assistants must not add extra sections to STYLE_GUIDE.md or ARCHITECTURE.md
without the user's approval. ARCHITECTURE.md must contain only an Overview section
with two or three paragraphs and an ASCII tree summarizing every folder and file.
Update existing sections instead of adding sections unless requested. Ask before
creating additional Markdown documents; a named-file request authorizes that file.
Keep status, build instructions and limitations in the Overview. The development
style guide stays out of release archives and binary packages.

## Live input safety

The keyboard must never be lost. Grab a device only after the virtual device exists
and every key on that device is up. Never grab our own virtual device. Rely on the
kernel releasing a grab when the process dies, and keep no recovery step that needs
the router to be healthy. With no agent configured, keep keyboards closed, so a login
screen is never touched. Release all forwarded and injected keys on shutdown and agent
loss, and release a device's keys when it is removed. Never run device tests on a live
desktop, because virtual keyboards reach the focused window; use tools/run_vm_tests.sh.
Test against GNOME only through tools/run_headless_tests.sh, and Cinnamon only through
tools/run_cinnamon_tests.sh. The isolated environment must be set before dbus-run-session, or
bus-activated services write to the real settings. Cinnamon also needs the system bus hidden,
because it tries to take over the login session.

Treat keystrokes as private. Never log, persist or send key contents, except the
digits of an Alt code once that sequence is complete. Router diagnostics record
states and counts only. Accept socket clients only from the active session's user,
checked through peer credentials. Keep the router's privileges and sandbox minimal.

Add no measurable typing latency: forward each event as soon as it is read. Stay
event driven with no polling loops, and do not redraw the indicator or settings
window unless something changed.

## Windows compatibility

Windows behavior is the specification, and any deliberate difference is a named
configuration option. Check it against a real Windows machine and record the results
as test fixtures. Do not derive expected values from this project's own output.

Alt codes hold Left Alt without Ctrl, read keypad keys by key code and act on Alt
release, emitting the character only after Alt is up. Digits with a leading zero use
the layout's ANSI code page and all others use its OEM code page, both modulo 256.
OEM values use the Microsoft glyph mapping, such as 1 for U+263A. Any other key
cancels the sequence. Right Alt counts only when the layout has no AltGr level. Hex
input through keypad plus is opt-in, as it is on Windows.

Modifier-only shortcuts fire on release and only when no other key was pressed
during the chord. Shortcuts with a final key fire on press and ignore auto-repeat.
Swallowing a key while Alt is forwarded leaves applications seeing a bare Alt tap,
so mask it with a neutral key to keep menu bars from taking focus.

## Build and cleanup behavior

bin/ holds executable deliverables only; packages/ holds .deb installers; .build/ holds
disposable intermediates.
Runtime files such as the extension, udev rule and systemd units are embedded in the
binaries, so bin/ works alone. Preserve source and published binaries on a failed
build. Do not run broad extension-based deletion or git clean. Cleanup shares the
build lock, rejects symlink or unowned roots, and refuses to run while binaries are
missing or older than source.

## Desktop interface

Use native GTK3 widgets for the settings window and follow the system theme, fonts,
scaling and accessibility: a header bar, framed list boxes as cards, dimmed aligned
labels and suggested-action buttons. Render and screenshot every interface change
before reporting it, and judge it as a designer would. Keep widget access on the main
thread. Save settings automatically and apply them without a restart. Keep application
text in one i18n module, covering English, Brazilian Portuguese, European Portuguese
and Greek. Show layouts by the system's translated names, which is what the taskbar shows.
Test the window only through tools/run_settings_tests.sh, with real input on a private X
server, and review its screenshots in each language in light and dark themes.

The layout switcher stays in the taskbar. Where the desktop already has one, as GNOME,
Cinnamon and KDE do, use it and draw no second indicator or pop-up. Adding, removing or
reordering layouts in settings writes to the desktop's own list, and the list must
always keep at least one layout.
