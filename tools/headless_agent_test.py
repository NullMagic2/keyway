"""
Module: keyway.tools.headless_agent_test
Purpose: Drive the real agent and extension on a headless desktop, with a fake router.
Created: 2026-11-08
Architecture: Started by run_headless_tests.sh or run_cinnamon_tests.sh, which provide the
isolated environment. The same scenarios run on each desktop through a small Desktop class.
The fake router stands in for key input, which a headless desktop has no devices for.
"""

import json
import os
import re
import signal
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path

WAYLAND_DISPLAY = "keyway-test"
SOCKET_NAME = "router.sock"
EXTENSION_UUID = "keyway@local"
TWO_LAYOUTS = ["br", "gr+simple"]
THREE_LAYOUTS = ["br", "gr+simple", "us"]
FIVE_LAYOUTS = ["br", "gr+simple", "us", "de", "fr"]
INPUT_SOURCE_MANAGER = "/usr/share/cinnamon/js/ui/keyboardManager.js"
KEYBOARD_APPLET = "imports.ui.appletManager.getRunningInstancesForUuid('keyboard@cinnamon.org')[0]"
CINNAMON_INDICATOR = ("Main.panel._rightBox.get_children()"
                      ".find(actor => actor.name === 'keyway-indicator')?._delegate")
PACKAGED_MARKER = "keyway/packaged"
RELOAD_MARKER = "keywayReloaded"
RESTART_CALLS = ["--user daemon-reload", "--user restart keyway.service",
                 "restart keyway-router.service"]
SCREENSHOT_SECONDS = 1.0
SHELL_INPUT_SOURCES = "Main.panel.statusArea.keyboard._inputSourceManager"

# Real key events for the Shell to act on, from a keyboard kept alive between calls.
SHELL_KEYS = """(() => {
    const {Clutter, GLib} = imports.gi;
    globalThis.keywayTestKeyboard ??= Clutter.get_default_backend().get_default_seat()
        .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    for (const [key, state] of [%s])
        keywayTestKeyboard.notify_keyval(GLib.get_monotonic_time(), Clutter[key],
            state ? Clutter.KeyState.PRESSED : Clutter.KeyState.RELEASED);
})()"""
# A real pointer for the Shell, kept alive between calls: a move, then a button change if any.
SHELL_POINTER = """(() => {
    const {Clutter, GLib} = imports.gi;
    globalThis.keywayTestPointer ??= Clutter.get_default_backend().get_default_seat()
        .create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    keywayTestPointer.notify_absolute_motion(GLib.get_monotonic_time(), %d, %d);
    %s
})()"""
SHELL_BUTTON = ("keywayTestPointer.notify_button(GLib.get_monotonic_time(),"
                " Clutter.BUTTON_PRIMARY, Clutter.ButtonState.%s);")
# A real touch screen for the Shell, kept alive between calls, with one finger on it.
SHELL_TOUCH = """(() => {
    const {Clutter, GLib} = imports.gi;
    globalThis.keywayTestTouch ??= Clutter.get_default_backend().get_default_seat()
        .create_virtual_device(Clutter.InputDeviceType.TOUCHSCREEN_DEVICE);
    keywayTestTouch.%s;
})()"""
TOUCH_DOWN = "notify_touch_down(GLib.get_monotonic_time(), 0, %d, %d)"
TOUCH_MOTION = "notify_touch_motion(GLib.get_monotonic_time(), 0, %d, %d)"
TOUCH_UP = "notify_touch_up(GLib.get_monotonic_time(), 0)"
# Puts a window above every other, as a screenshot tool's extension does for its picture of the
# screen, since a Wayland window cannot ask for that itself.
SHELL_LIFT_WINDOW = """(() => {
    const actor = global.get_window_actors()
        .find(actor => actor.get_meta_window().get_title() === '%s');
    actor.hide();
    actor.get_parent().remove_child(actor);
    global.top_window_group.add_child(actor);
    actor.show();
})()"""
SHELL_ACTOR_AT = ("global.stage.get_actor_at_pos(imports.gi.Clutter.PickMode.REACTIVE, %d, %d)")
HOLD_SUPER_TAP_SPACE = "['KEY_Super_L', 1], ['KEY_space', 1], ['KEY_space', 0]"
HOLD_SHIFT_SUPER_TAP_SPACE = "['KEY_Shift_L', 1], " + HOLD_SUPER_TAP_SPACE
RELEASE_SUPER = "['KEY_Super_L', 0]"
RELEASE_SHIFT_SUPER = "['KEY_Super_L', 0], ['KEY_Shift_L', 0]"
X_DISPLAY = ":95"
X_SCREEN = "1280x800x24"
WAIT_LIMIT_SECONDS = 15.0
RETRY_SECONDS = 0.05
QUIET_SECONDS = 1.0
PROTOCOL_VERSION = 1

NEXT_LAYOUT = {"shortcut": "next_layout"}
PREVIOUS_LAYOUT = {"shortcut": "previous_layout"}


def select_layout(layout):
    return {"shortcut": {"select_layout": layout}}


def decimal_code(digits):
    return {"alt_code": {"radix": "decimal", "digits": digits}}


def check(description, passed, detail=""):
    print(f"{'ok  ' if passed else 'FAIL'} {description}{f': {detail}' if detail else ''}", flush=True)

    if not passed:
        raise SystemExit(1)


def wait_until(description, condition):
    deadline = time.monotonic() + WAIT_LIMIT_SECONDS

    while time.monotonic() < deadline:
        if condition():
            check(description, True)
            return

        time.sleep(RETRY_SECONDS)

    check(description, False, "timed out")


def wait_quietly(condition):
    """Waits for a step that only makes the next one safe, and reports it only when it fails."""
    deadline = time.monotonic() + WAIT_LIMIT_SECONDS

    while not condition():
        if time.monotonic() > deadline:
            check("a step the test waits for", False, "timed out")

        time.sleep(RETRY_SECONDS)


def run_quietly(*arguments):
    return subprocess.run(arguments, capture_output=True, text=True, check=False)


def extension_call(method, *arguments):
    return run_quietly("gdbus", "call", "--session", "--dest", "org.keyway.Shell",
                       "--object-path", "/org/keyway/Shell",
                       "--method", f"org.keyway.Shell.{method}", *arguments)


def active_source():
    result = extension_call("ActiveSource")

    return result.stdout.strip().strip("(),'") if result.returncode == 0 else None


def gsettings_set(schema, key, value):
    subprocess.run(["gsettings", "set", schema, key, value], check=True)


class GnomeDesktop:
    """GNOME Shell, headless on Wayland."""

    eval_name = "org.gnome.Shell"
    eval_path = "/org/gnome/Shell"
    indicator = "Main.panel.statusArea.keyway"
    desktop_switcher = "Main.panel.statusArea.keyboard.container"
    reloadable_file = "gnome-shell/extensions/keyway@local/adapter.js"

    def set_layouts(self, layouts):
        sources = ", ".join(f"('xkb', '{layout}')" for layout in layouts)
        gsettings_set("org.gnome.desktop.input-sources", "sources", f"[{sources}]")

    def start(self, log):
        os.environ["WAYLAND_DISPLAY"] = WAYLAND_DISPLAY

        return [subprocess.Popen(
            ["gnome-shell", "--headless", "--wayland", "--no-x11", "--unsafe-mode",
             "--virtual-monitor", "1280x720", f"--wayland-display={WAYLAND_DISPLAY}"],
            stdout=log, stderr=subprocess.STDOUT)]

    def wait_for_daemon(self):
        pass

    def enable_extension(self):
        desktop_eval(f"Main.extensionManager.enableExtension('{EXTENSION_UUID}')")

    def disable_extension(self):
        desktop_eval(f"Main.extensionManager.disableExtension('{EXTENSION_UUID}')")

    def enabled_extensions(self):
        return run_quietly("gsettings", "get", "org.gnome.shell", "enabled-extensions").stdout

    def text_input_matches(self, expect_text_input):
        has_input = '"true"' in desktop_eval("String(!!Main.inputMethod.currentFocus)")

        return has_input == expect_text_input

    def save_screenshot(self, path):
        run_quietly("gdbus", "call", "--session", "--dest", "org.gnome.Shell.Screenshot",
                    "--object-path", "/org/gnome/Shell/Screenshot",
                    "--method", "org.gnome.Shell.Screenshot.Screenshot",
                    "false", "false", str(path))

    def pointer(self, x, y, pressed=None):
        """Moves the pointer, then presses or releases its button unless pressed is None."""
        button = "" if pressed is None else SHELL_BUTTON % ("PRESSED" if pressed else "RELEASED")

        desktop_eval(SHELL_POINTER % (x, y, button))

    def keep_above_others(self, title):
        desktop_eval(SHELL_LIFT_WINDOW % title)

    def hold_key(self, name, held):
        """Presses or releases a key on a keyboard other than the on-screen one's."""
        desktop_eval(SHELL_KEYS % f"['KEY_{name}', {int(held)}]")


class CinnamonDesktop:
    """Cinnamon 6.4 and older on a private X server, whose daemon applies the layouts."""

    eval_name = "org.Cinnamon"
    eval_path = "/org/Cinnamon"
    indicator = CINNAMON_INDICATOR
    desktop_switcher = f"{KEYBOARD_APPLET}.actor"
    reloadable_file = "cinnamon/extensions/keyway@local/extension.js"
    has_layout_limit = True

    def set_layouts(self, layouts):
        entries = ", ".join("'" + layout.replace("+", "\\t") + "'" for layout in layouts)
        gsettings_set("org.gnome.libgnomekbd.keyboard", "layouts", f"[{entries}]")

    def applet_shows_second_layout(self):
        return '"1"' in desktop_eval(
            f"String({KEYBOARD_APPLET}._controller.get_current_layout_idx())")

    def wait_for_daemon(self):
        wait_until("the daemon applies the layouts",
                   lambda: "br,gr" in run_quietly("setxkbmap", "-query").stdout)

    def start(self, log):
        os.environ["DISPLAY"] = X_DISPLAY
        server = subprocess.Popen(["Xvfb", X_DISPLAY, "-screen", "0", X_SCREEN],
                                  stdout=log, stderr=subprocess.STDOUT)

        wait_until("X server is up", lambda: run_quietly("xdpyinfo").returncode == 0)

        daemon = subprocess.Popen(["csd-keyboard"], stdout=log, stderr=subprocess.STDOUT)

        self.wait_for_daemon()

        return [subprocess.Popen(["cinnamon", "--x11"], stdout=log, stderr=subprocess.STDOUT),
                daemon, server]

    def enable_extension(self):
        gsettings_set("org.cinnamon", "enabled-extensions", f"['{EXTENSION_UUID}']")

    def disable_extension(self):
        gsettings_set("org.cinnamon", "enabled-extensions", "@as []")

    def enabled_extensions(self):
        return run_quietly("gsettings", "get", "org.cinnamon", "enabled-extensions").stdout

    def text_input_matches(self, _expect_text_input):
        """Cinnamon has no text input protocol here, so every client gets key events."""
        return True

    def save_screenshot(self, path):
        result = run_quietly("gdbus", "call", "--session", "--dest", self.eval_name,
                             "--object-path", self.eval_path, "--method",
                             f"{self.eval_name}.Screenshot", "false", "false", str(path))

        if result.returncode != 0:
            print(f"screenshot failed: {result.stderr.strip()}", flush=True)

    def pointer(self, x, y, pressed=None):
        """Moves the pointer, then presses or releases its button unless pressed is None."""
        button = [] if pressed is None else ["mousedown" if pressed else "mouseup", "1"]

        run_quietly("xdotool", "mousemove", str(x), str(y), *button)

    def hold_key(self, name, held):
        """Presses or releases a key as a real keyboard does."""
        run_quietly("xdotool", "keydown" if held else "keyup", name)

    def keep_above_others(self, _title):
        """An X window asks for this itself, and the window manager grants it."""


class InputSourceCinnamonDesktop(CinnamonDesktop):
    """Cinnamon 6.6 and later, which applies its input sources itself, with no layout limit."""

    has_layout_limit = False

    def set_layouts(self, layouts):
        sources = ", ".join(f"('xkb', '{layout}')" for layout in layouts)
        gsettings_set("org.cinnamon.desktop.input-sources", "sources", f"[{sources}]")

    def applet_shows_second_layout(self):
        return '"true"' in desktop_eval(
            f"String({KEYBOARD_APPLET}._selectedLayout"
            f" === [...{KEYBOARD_APPLET}._layoutItems.values()][1])")

    def wait_for_daemon(self):
        pass


def cinnamon_desktop():
    if Path(INPUT_SOURCE_MANAGER).exists():
        return InputSourceCinnamonDesktop()

    return CinnamonDesktop()


DESKTOPS = {"gnome": GnomeDesktop, "cinnamon": cinnamon_desktop}
desktop = None


def desktop_eval(code):
    result = run_quietly("gdbus", "call", "--session", "--dest", desktop.eval_name,
                         "--object-path", desktop.eval_path,
                         "--method", f"{desktop.eval_name}.Eval", code)

    return result.stdout.strip()


class FakeRouter:
    """Listens where the real router would, and speaks its protocol."""

    def __init__(self, path):
        self.path = path
        self.listener = socket.socket(socket.AF_UNIX)
        self.listener.bind(str(path))
        self.listener.listen()
        self.connection = None
        self.buffer = b""

    def accept(self, timeout=WAIT_LIMIT_SECONDS):
        self.listener.settimeout(timeout)

        try:
            self.connection, _address = self.listener.accept()
        except TimeoutError:
            return False

        self.buffer = b""
        return True

    def receive(self, timeout=WAIT_LIMIT_SECONDS):
        """Returns the next message, or None on timeout, or EOFError if the agent closed."""
        self.connection.settimeout(timeout)

        while b"\n" not in self.buffer:
            try:
                chunk = self.connection.recv(4096)
            except TimeoutError:
                return None

            if not chunk:
                raise EOFError

            self.buffer += chunk

        line, self.buffer = self.buffer.split(b"\n", 1)

        return json.loads(line)

    def send(self, message):
        self.connection.sendall(json.dumps(message).encode() + b"\n")

    def wait_for_drop(self):
        """True once the agent has closed its connection."""
        try:
            while self.receive() is not None:
                pass
        except EOFError:
            return True

        return False

    def close_connection(self):
        self.connection.close()
        self.connection = None

    def handshake(self):
        """Completes the greeting and returns the configuration the agent sends."""
        check("agent greets the router", self.receive() == {"hello": {"version": PROTOCOL_VERSION}})
        self.send({"hello": {"version": PROTOCOL_VERSION}})

        message = self.receive()
        check("agent sends a configuration", message is not None and "configure" in message, message)

        return message["configure"]

    def expect_quiet(self, description):
        message = self.receive(timeout=QUIET_SECONDS)
        check(description, message is None, message)

    def expect_message(self, description, expected):
        """Waits for one message, reporting any others that arrive first."""
        deadline = time.monotonic() + WAIT_LIMIT_SECONDS

        while time.monotonic() < deadline:
            message = self.receive(timeout=deadline - time.monotonic())

            if message == expected:
                check(description, True)
                return

            print(f"     router received {message}", flush=True)

        check(description, False, f"never received {expected}")

    def next_configuration(self):
        """Waits for a configuration, reporting any other messages that arrive first."""
        deadline = time.monotonic() + WAIT_LIMIT_SECONDS

        while time.monotonic() < deadline:
            message = self.receive(timeout=deadline - time.monotonic())

            if message is not None and "configure" in message:
                return message["configure"]

            print(f"     router received {message}", flush=True)

        check("a configuration arrives", False)

    def drain(self):
        """Discards messages already on their way, reporting them for the test log."""
        while (message := self.receive(timeout=QUIET_SECONDS)) is not None:
            print(f"     router received {message}", flush=True)


class TestWindow:
    """A focused GTK client; its output lines are collected as it reports them."""

    def __init__(self, script, mode):
        self.lines = []
        self.process = subprocess.Popen(
            [sys.executable, "-I", script, mode],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        threading.Thread(target=self._collect, daemon=True).start()
        self.title = f"keyway-test-{mode}"

    def _collect(self):
        for line in self.process.stdout:
            self.lines.append(line.rstrip("\n"))

    def wait_for_focus(self, expect_text_input):
        def is_ready():
            desktop_eval("Main.overview.hide()")
            focused = self.title in desktop_eval("global.display.focus_window?.title")

            return focused and desktop.text_input_matches(expect_text_input)

        wait_until(f"{self.title} has focus", is_ready)

    def close(self):
        self.process.terminate()
        self.process.wait()


def read_config_section(configuration):
    return {binding["chord"]: binding["action"] for binding in configuration["shortcuts"]}


def test_waits_for_extension_then_configures(router):
    check("agent waits for the extension", not router.accept(timeout=QUIET_SECONDS))

    desktop.enable_extension()
    check("agent connects once the extension appears", router.accept())

    configuration = router.handshake()
    check("default shortcut is Alt+Shift",
          read_config_section(configuration) == {"Alt+Shift": "next_layout"},
          configuration["shortcuts"])
    check("Brazilian Right Alt stays AltGr",
          configuration["alt_codes"]["allow_right_alt"] is False)


def settings_menu_labels():
    return desktop_eval(f"{desktop.indicator}?.menu._getMenuItems()"
                        ".map(item => item.label?.text).filter(Boolean).join('|')")


# What the indicator shows for each test layout, from data/layout_labels.json.
INDICATOR_LABELS = {"br": "PT-BR", "gr+simple": "EL-GR"}


def indicator_shows(layout):
    label = f"{desktop.indicator}?._label.text"

    return '"true"' in desktop_eval(f"String({label} === '{INDICATOR_LABELS[layout]}')")


def indicator_flag_shown():
    """True when the indicator and every layout in its menu show a flag."""
    menu_flags = f"[...{desktop.indicator}._items.values()].every(entry => entry.flag)"

    return '"true"' in desktop_eval(f"String(!!{desktop.indicator}?._flag.visible && {menu_flags})")


def menu_rows_show_only_names():
    """True when each layout's row holds one text; a label and the text actor in it count once."""
    texts = ("const texts = actor => [typeof actor.text === 'string' ? actor.text : '',"
             " ...actor.get_children().flatMap(texts)];")
    rows = f"{desktop.indicator}._layoutSection._getMenuItems()"
    one_text = "item => new Set(texts(item.actor ?? item).filter(Boolean)).size === 1"

    code = f"(() => {{ {texts} return String({rows}.every({one_text})); }})()"

    return '"true"' in desktop_eval(code)


def layout_item_count():
    return desktop_eval(f"String({desktop.indicator}?._layoutSection._getMenuItems().length)")


def desktop_switcher_is_visible():
    return '"true"' in desktop_eval(f"String({desktop.desktop_switcher}.visible)")


def keyway_indicator_exists():
    return '"true"' in desktop_eval(f"String(!!{desktop.indicator})")


def save_screenshot(name):
    """Only when a run asks for screenshots, as they take time and no check depends on them."""
    directory = os.environ.get("KEYWAY_SCREENSHOT_DIR")

    if not directory:
        return

    Path(directory).mkdir(parents=True, exist_ok=True)
    time.sleep(SCREENSHOT_SECONDS)
    desktop.save_screenshot(Path(directory) / name)


def screenshot_indicator_menu():
    desktop_eval(f"{desktop.indicator}.menu.open()")
    save_screenshot("indicator-menu.png")
    desktop_eval(f"{desktop.indicator}.menu.close()")


def test_replaces_the_desktop_switcher():
    wait_until("Keyway's indicator shows the active layout", lambda: indicator_shows("br"))
    check("the indicator and its menu show flags", indicator_flag_shown())
    wait_until("the desktop's own switcher is hidden", lambda: not desktop_switcher_is_visible())
    check("the menu lists each layout", '"2"' in layout_item_count(), layout_item_count())
    check("by its full name alone, without its code", menu_rows_show_only_names())

    screenshot_indicator_menu()

    desktop_eval(f"{desktop.indicator}._layoutSection._getMenuItems()[1].activate(null)")
    wait_until("choosing a layout in the menu switches to it",
               lambda: active_source() == "gr+simple")
    wait_until("the indicator follows the switch", lambda: indicator_shows("gr+simple"))

    desktop.set_layouts(THREE_LAYOUTS)
    wait_until("the menu follows the layout list", lambda: '"3"' in layout_item_count())
    check("the desktop's switcher stays hidden when layouts change",
          not desktop_switcher_is_visible())

    desktop.set_layouts(TWO_LAYOUTS)
    desktop.wait_for_daemon()
    wait_until("the menu returns to two layouts", lambda: '"2"' in layout_item_count())
    desktop_eval(f"{desktop.indicator}._layoutSection._getMenuItems()[0].activate(null)")
    wait_until("the first layout is active again", lambda: active_source() == "br")
    check("the desktop's switcher is still hidden", not desktop_switcher_is_visible())


def test_taskbar_menu_opens_the_settings_window():
    wait_until("the taskbar menu offers the settings window",
               lambda: "Keyway settings" in settings_menu_labels())

    desktop_eval(f"{desktop.indicator}.menu._getMenuItems()"
                 ".find(item => item.label?.text === 'Keyway settings').activate(null)")

    def settings_window_is_open():
        desktop_eval("Main.overview.hide()")
        return '"Keyway"' in desktop_eval("global.display.focus_window?.title")

    wait_until("choosing it opens the real settings window", settings_window_is_open)

    desktop_eval("global.display.focus_window.delete(global.get_current_time())")
    wait_until("the settings window closes",
               lambda: '"Keyway"' not in desktop_eval("global.display.focus_window?.title"))


# Evdev codes of the keys the on-screen keyboard test clicks, and its menu entry.
KEY_1 = 2
KEY_Q = 16
KEY_I = 23
KEY_RIGHT_OF_P = 26
KEY_A = 30
KEY_SEMICOLON = 39
KEY_LEFT_SHIFT = 42
KEY_LEFT_ALT = 56
KEY_CAPS_LOCK = 58
KEY_102ND = 86
KEY_ABNT_EXTRA = 89
KEY_RIGHT_ALT = 100
LATCHING_KEYS = (KEY_LEFT_SHIFT, KEY_RIGHT_ALT)
KEYBOARD_MENU_LABEL = "On-screen keyboard"
DRAG_DISTANCE = (60, -40)


def keyboard():
    return f"{desktop.indicator}._viewer"


def keyboard_is_shown():
    return '"true"' in desktop_eval(f"String(!!{keyboard()}?._panel)")


def keyboard_key(code):
    """The key a keyboard has at this code, not one that types its shifted character."""
    return f"{keyboard()}._keys.find(key => key.code === {code} && key.role !== 'shifted')"


def own_key(text):
    """The key that types this character, which the layout itself has on Shift."""
    return (f"{keyboard()}._keys.find(key => key.role === 'shifted'"
            f" && key.label.text === {json.dumps(text)})")


def key_shows(code, text):
    """True when the key is drawn with this label. The text travels as an ASCII literal."""
    return '"true"' in desktop_eval(f"String({keyboard_key(code)}?.label.text === {json.dumps(text)})")


def key_hints(code, shift, level3=None):
    """True when the key's corners show these characters. No level3 leaves that corner unchecked."""
    wanted = f"key.shiftHint.text === {json.dumps(shift)}"

    if level3 is not None:
        wanted += f" && key.level3Hint.text === {json.dumps(level3)}"

    return '"true"' in desktop_eval(f"String((key => {wanted})({keyboard_key(code)}))")


def is_marked(key, pseudo_class):
    return '"true"' in desktop_eval(
        f"String(!!{key}?.actor.has_style_pseudo_class('{pseudo_class}'))")


def key_is_marked(code, pseudo_class):
    return is_marked(keyboard_key(code), pseudo_class)


def centre_of(actor):
    """Returns the middle of an actor on the screen, where a click reaches it."""
    result = desktop_eval(
        f"(() => {{ const actor = {actor};"
        " const [x, y] = actor.get_transformed_position();"
        " const [width, height] = actor.get_transformed_size();"
        " return [Math.round(x + width / 2), Math.round(y + height / 2)].join(','); })()")
    x, y = re.search(r"(-?\d+),(-?\d+)", result).groups()

    return int(x), int(y)


def click(actor):
    x, y = centre_of(actor)

    desktop.pointer(x, y, pressed=True)
    desktop.pointer(x, y, pressed=False)


def click_and_see(key, stays_down=False):
    """Sees a key go down and up, as clicks sent in a row can otherwise all reach one key."""
    x, y = centre_of(f"{key}.actor")

    desktop.pointer(x, y, pressed=True)
    wait_quietly(lambda: stays_down or is_marked(key, "active"))
    desktop.pointer(x, y, pressed=False)
    wait_quietly(lambda: stays_down or not is_marked(key, "active"))


def click_key(code):
    click_and_see(keyboard_key(code), stays_down=code in LATCHING_KEYS)


def keyboard_position():
    result = desktop_eval(f"[{keyboard()}._panel.x, {keyboard()}._panel.y].join(',')")
    x, y = re.search(r"(-?\d+),(-?\d+)", result).groups()

    return int(x), int(y)


def keyboard_has_theme(theme):
    return '"true"' in desktop_eval(
        f"String({keyboard()}._panel.has_style_class_name('keyway-keyboard-{theme}'))")


def test_on_screen_keyboard(router, window_script, config_path):
    """Real clicks on the keyboard, with a window that must keep the focus and get the keys."""
    wait_until("the taskbar menu offers the on-screen keyboard",
               lambda: KEYBOARD_MENU_LABEL in settings_menu_labels())
    check("which is hidden until asked for", not keyboard_is_shown())

    window = TestWindow(window_script, "entry")
    window.wait_for_focus(expect_text_input=True)

    def typed(text):
        return lambda: bool(window.lines) and window.lines[-1] == f"TEXT {text}"

    def window_has_focus():
        return window.title in desktop_eval("global.display.focus_window?.title")

    desktop_eval(f"{desktop.indicator}.menu._getMenuItems()"
                 f".find(item => item.label?.text === '{KEYBOARD_MENU_LABEL}').activate(null)")
    wait_until("choosing it shows the keyboard", keyboard_is_shown)
    wait_until("with the Brazilian layout's letters", lambda: key_shows(KEY_Q, "q"))
    check("its title names the layout",
          "Portuguese (Brazil)" in desktop_eval(f"{keyboard()}._name.text"),
          desktop_eval(f"{keyboard()}._name.text"))
    check("the ABNT2 key is drawn where the layout uses it", key_shows(KEY_ABNT_EXTRA, "/"))
    check("a key's corners show what Shift and AltGr type on it",
          key_hints(KEY_1, "!", "\u00b9"))
    check("with no corner for a letter's capital", key_hints(KEY_Q, "", "/"))
    check("the accents that the layout has on Shift have keys of their own",
          '"true"' in desktop_eval(f"String(!!{own_key('`')} && !!{own_key('^')})"))

    desktop.hold_key("Shift_L", True)
    wait_until("Shift held on the real keyboard shows the capitals too",
               lambda: key_shows(KEY_Q, "Q"))
    check("and lights the Shift key", key_is_marked(KEY_LEFT_SHIFT, "checked"))
    desktop.hold_key("Shift_L", False)
    wait_until("until it is let go",
               lambda: key_shows(KEY_Q, "q") and not key_is_marked(KEY_LEFT_SHIFT, "checked"))
    desktop.hold_key("ISO_Level3_Shift", True)
    wait_until("AltGr held there shows the third level and lights AltGr",
               lambda: key_shows(KEY_1, "\u00b9") and key_is_marked(KEY_RIGHT_ALT, "checked"))
    desktop.hold_key("ISO_Level3_Shift", False)
    wait_until("until it is let go too",
               lambda: key_shows(KEY_1, "1") and not key_is_marked(KEY_RIGHT_ALT, "checked"))
    desktop.hold_key("Alt_L", True)
    wait_until("and Alt held there lights Alt", lambda: key_is_marked(KEY_LEFT_ALT, "checked"))
    desktop.hold_key("Alt_L", False)
    wait_until("for as long as it is held", lambda: not key_is_marked(KEY_LEFT_ALT, "checked"))
    check("showing it leaves the window focused", window_has_focus())
    check("it is light unless the user chose otherwise", keyboard_has_theme("light"))
    save_screenshot("keyboard.png")

    click_key(KEY_Q)
    wait_until("a clicked key types into the focused window", typed("q"))
    check("which keeps the focus", window_has_focus())

    click_key(KEY_LEFT_SHIFT)
    wait_until("Shift stays down and the keys show capitals", lambda: key_shows(KEY_Q, "Q"))
    check("Shift is marked as held", key_is_marked(KEY_LEFT_SHIFT, "checked"))
    check("and the corner then shows the character without it",
          key_shows(KEY_1, "!") and key_hints(KEY_1, "1"))
    save_screenshot("keyboard-shift.png")
    click_key(KEY_Q)
    wait_until("the next key is typed shifted", typed("qQ"))
    wait_until("and Shift goes up after it", lambda: key_shows(KEY_Q, "q"))

    click_key(KEY_RIGHT_ALT)
    wait_until("AltGr shows the third level", lambda: key_shows(KEY_1, "\u00b9"))
    click_key(KEY_1)
    wait_until("and types it", typed("qQ\u00b9"))

    check("an accent key shows its accent", key_shows(KEY_RIGHT_OF_P, "\u00b4"))
    click_key(KEY_RIGHT_OF_P)
    click_key(KEY_A)
    wait_until("an accent key and then a letter type the accented letter",
               typed("qQ\u00b9\u00e1"))

    click_key(KEY_CAPS_LOCK)
    wait_until("Caps Lock shows capitals while it is on", lambda: key_shows(KEY_Q, "Q"))
    check("leaves the digits alone", key_shows(KEY_1, "1"))
    check("and is marked as on", key_is_marked(KEY_CAPS_LOCK, "checked"))
    click_key(KEY_CAPS_LOCK)
    wait_until("and lower case again once it is off", lambda: key_shows(KEY_Q, "q"))

    router.send(select_layout("gr+simple"))
    wait_until("the keyboard follows a switch to Greek", lambda: key_shows(KEY_A, "\u03b1"))
    check("its title follows too", "Greek" in desktop_eval(f"{keyboard()}._name.text"))
    check("the ABNT2 key goes where the layout has nothing on it",
          '"false"' in desktop_eval(f"String(!!{keyboard_key(KEY_ABNT_EXTRA)})"))
    click_key(KEY_A)
    wait_until("a clicked key types the Greek letter", typed("qQ\u00b9\u00e1\u03b1"))
    check("the Greek accent key shows its diaeresis in the corner",
          key_shows(KEY_SEMICOLON, "\u00b4") and key_hints(KEY_SEMICOLON, "\u00a8", ""))
    check("and the bracket key its closing bracket",
          key_shows(KEY_102ND, "\u00ab") and key_hints(KEY_102ND, "\u00bb", ""))
    check("while a Greek letter shows only itself", key_hints(KEY_A, "", ""))
    click_key(KEY_LEFT_SHIFT)
    wait_until("Shift shows the Greek diaeresis", lambda: key_shows(KEY_SEMICOLON, "\u00a8"))
    click_key(KEY_SEMICOLON)
    click_key(KEY_I)
    wait_until("which goes on the next letter",
               typed("qQ\u00b9\u00e1\u03b1\u03ca"))
    wait_until("and Shift is up again", lambda: key_shows(KEY_SEMICOLON, "\u00b4"))

    check("the closing bracket and the diaeresis have keys of their own in Greek",
          '"true"' in desktop_eval(
              f"String(!!{own_key(chr(0xbb))} && !!{own_key(chr(0xa8))})"))
    click_and_see(own_key("\u00bb"))
    wait_until("the bracket's key types it without Shift",
               typed("qQ\u00b9\u00e1\u03b1\u03ca\u00bb"))
    click_and_see(own_key("\u00a8"))
    click_key(KEY_I)
    wait_until("and the diaeresis from its key goes on the next letter",
               typed("qQ\u00b9\u00e1\u03b1\u03ca\u00bb\u03ca"))
    click_key(KEY_A)
    wait_until("after which keys are unshifted as before",
               typed("qQ\u00b9\u00e1\u03b1\u03ca\u00bb\u03ca\u03b1"))
    save_screenshot("keyboard-greek.png")

    config_path.parent.mkdir(parents=True, exist_ok=True)
    config_path.write_text('[keyboard]\ntheme = "dark"\n')
    wait_until("choosing the dark theme darkens the keyboard while it is open",
               lambda: keyboard_has_theme("dark") and not keyboard_has_theme("light"))
    save_screenshot("keyboard-dark.png")
    click_key(KEY_LEFT_SHIFT)
    wait_until("Shift on the dark keyboard", lambda: key_shows(KEY_A, "\u0391"))
    save_screenshot("keyboard-dark-shift.png")
    click_key(KEY_LEFT_SHIFT)
    config_path.unlink()
    wait_until("and without the choice it is light again", lambda: keyboard_has_theme("light"))
    router.next_configuration()
    router.next_configuration()

    before = keyboard_position()
    x, y = centre_of(f"{keyboard()}._name")

    desktop.pointer(x, y, pressed=True)
    wait_until("pressing the title takes hold of the keyboard",
               lambda: '"true"' in desktop_eval(f"String(!!{keyboard()}._drag)"))
    desktop.pointer(x + DRAG_DISTANCE[0], y + DRAG_DISTANCE[1])
    wait_until("dragging it moves the keyboard",
               lambda: keyboard_position() == (before[0] + DRAG_DISTANCE[0],
                                               before[1] + DRAG_DISTANCE[1]))
    desktop.pointer(x + DRAG_DISTANCE[0], y + DRAG_DISTANCE[1], pressed=False)
    wait_until("and releasing lets go of it",
               lambda: '"false"' in desktop_eval(f"String(!!{keyboard()}._drag)"))

    click(f"{keyboard()}._panel.get_first_child().get_last_child()")
    wait_until("its close button hides it", lambda: not keyboard_is_shown())
    check("the window still has the focus", window_has_focus())

    router.send(select_layout("br"))
    wait_until("back on br after the keyboard", lambda: active_source() == "br")

    window.close()


def touch(action):
    desktop_eval(SHELL_TOUCH % action)


def tap(actor):
    touch(TOUCH_DOWN % centre_of(actor))
    touch(TOUCH_UP)


def test_on_screen_keyboard_by_touch(window_script):
    """Runs only on GNOME: an X server has no touch screen here, and sends touches as clicks."""
    window = TestWindow(window_script, "entry")
    window.wait_for_focus(expect_text_input=True)

    def typed(text):
        return lambda: bool(window.lines) and window.lines[-1] == f"TEXT {text}"

    desktop_eval(f"{keyboard()}.show()")
    wait_until("the keyboard is shown for touching", lambda: key_shows(KEY_Q, "q"))

    tap(f"{keyboard_key(KEY_Q)}.actor")
    wait_until("a tapped key types into the focused window", typed("q"))

    tap(f"{keyboard_key(KEY_LEFT_SHIFT)}.actor")
    wait_until("a tap holds Shift for the next key", lambda: key_shows(KEY_Q, "Q"))
    tap(f"{keyboard_key(KEY_Q)}.actor")
    wait_until("which is typed shifted", typed("qQ"))
    wait_until("and Shift goes up after the tap", lambda: key_shows(KEY_Q, "q"))

    before = keyboard_position()
    x, y = centre_of(f"{keyboard()}._name")

    touch(TOUCH_DOWN % (x, y))
    wait_until("a finger on the title takes hold of the keyboard",
               lambda: '"true"' in desktop_eval(f"String(!!{keyboard()}._drag)"))
    touch(TOUCH_MOTION % (x + DRAG_DISTANCE[0], y + DRAG_DISTANCE[1]))
    wait_until("and moves it",
               lambda: keyboard_position() == (before[0] + DRAG_DISTANCE[0],
                                               before[1] + DRAG_DISTANCE[1]))
    touch(TOUCH_UP)
    wait_until("until it is lifted",
               lambda: '"false"' in desktop_eval(f"String(!!{keyboard()}._drag)"))

    tap(f"{keyboard()}._panel.get_first_child().get_last_child()")
    wait_until("a tap on the close button hides the keyboard", lambda: not keyboard_is_shown())
    check("the window kept the focus through the touches",
          window.title in desktop_eval("global.display.focus_window?.title"))

    window.close()


def test_screenshot_tools_cover_the_keyboard(window_script):
    """A tool shows a picture of the screen, keyboard included, and must get the clicks on it."""
    desktop_eval(f"{keyboard()}.show()")
    wait_until("the keyboard is shown for a screenshot", lambda: key_shows(KEY_Q, "q"))

    x, y = centre_of(f"{keyboard_key(KEY_Q)}.actor")

    if isinstance(desktop, GnomeDesktop):
        desktop_eval("Main.screenshotUI.open()")
        wait_until("the Shell's own screenshot tool opens over the keyboard",
                   lambda: '"true"' in desktop_eval(
                       f"String(Main.screenshotUI.visible && !{keyboard()}._panel.contains("
                       f"{SHELL_ACTOR_AT % (x, y)}))"))
        desktop_eval("Main.screenshotUI.close(true)")
        wait_until("and closes again",
                   lambda: '"false"' in desktop_eval("String(Main.screenshotUI.visible)"))

    window = TestWindow(window_script, "overlay")
    window.wait_for_focus(expect_text_input=False)
    desktop.keep_above_others(window.title)

    def window_was_clicked():
        desktop.pointer(x, y, pressed=True)
        desktop.pointer(x, y, pressed=False)

        return "CLICK" in window.lines

    wait_until("a full screen window kept above the others gets a click where a key is",
               window_was_clicked)
    check("which presses no key", '"0"' in desktop_eval(f"String({keyboard()}._held.size)"))
    window.close()

    def close_button_hides_it():
        if keyboard_is_shown():
            click(f"{keyboard()}._panel.get_first_child().get_last_child()")

        return not keyboard_is_shown()

    wait_until("once it is gone the keyboard takes clicks again, on its close button",
               close_button_hides_it)


# Keyway's languages as the taskbar menu offers them, and its texts in those the test chooses.
LANGUAGE_NAMES = ["English", "Espa\u00f1ol", "Portugu\u00eas (Brasil)",
                  "\u0395\u03bb\u03bb\u03b7\u03bd\u03b9\u03ba\u03ac", "Portugu\u00eas (Portugal)"]
LANGUAGE_MENU_LABEL = "Language"
GREEK_MENU_LABELS = ["\u03a0\u03bb\u03b7\u03ba\u03c4\u03c1\u03bf\u03bb\u03cc\u03b3\u03b9\u03bf"
                     " \u03bf\u03b8\u03cc\u03bd\u03b7\u03c2",
                     "\u0393\u03bb\u03ce\u03c3\u03c3\u03b1",
                     "\u03a1\u03c5\u03b8\u03bc\u03af\u03c3\u03b5\u03b9\u03c2 \u03c4\u03bf\u03c5 Keyway"]
BRAZILIAN_MENU_LABELS = ["Teclado virtual", "Idioma", "Configura\u00e7\u00f5es do Keyway"]
ENGLISH_MENU_LABELS = [KEYBOARD_MENU_LABEL, LANGUAGE_MENU_LABEL, "Keyway settings"]
BRAZILIAN_LAYOUT_NAMES = Path("/usr/share/locale-langpack/pt_BR/LC_MESSAGES/xkeyboard-config.mo")
BRAZILIAN_NAME_OF_BR = "Portugu\u00eas (Brasil)"


def compact_json(value):
    """Writes a value as the desktop's JSON.stringify does, in ASCII."""
    return json.dumps(value, separators=(",", ":"))


def menu_shows(labels):
    """True when the menu's own entries read exactly so. The texts travel as ASCII literals."""
    shown = (f"{desktop.indicator}.menu._getMenuItems()"
             ".map(item => item.label?.text).filter(Boolean)")

    return '"true"' in desktop_eval(f"String(JSON.stringify({shown}) === '{compact_json(labels)}')")


def keyway_language():
    return desktop_eval(f"{desktop.indicator}._service.language")


def choose_language(tag):
    desktop_eval(f"{desktop.indicator}._languageItems.get('{tag}').activate(null)")


def test_language_menu(router, config_path):
    """The menu's languages: a choice is saved by the agent and comes back as the menu's texts."""
    def saved(tag):
        return lambda: (config_path.exists()
                        and f'language = "{tag}"' in config_path.read_text())

    wait_until("the taskbar menu offers Keyway's languages",
               lambda: menu_shows(ENGLISH_MENU_LABELS))
    check("each under its own name",
          '"true"' in desktop_eval(
              f"String(JSON.stringify({desktop.indicator}._languagesItem.menu._getMenuItems()"
              f".map(item => item.label.text)) === '{compact_json(LANGUAGE_NAMES)}')"))
    check("and follows the desktop's until one is chosen", '"en"' in keyway_language(),
          keyway_language())

    choose_language("el")
    wait_until("choosing Greek saves it in the configuration file", saved("el"))
    wait_until("and puts the menu in Greek", lambda: menu_shows(GREEK_MENU_LABELS))
    check("with Greek as the language in use", '"el"' in keyway_language())
    router.next_configuration()

    desktop_eval(f"{desktop.indicator}.menu.open(); {desktop.indicator}._languagesItem.menu.open()")
    check("the open list of languages is not shaded, nor its title",
          '"true"' in desktop_eval(
              f"(item => String(!item.menu.actor.has_style_class_name('popup-sub-menu')"
              " && !(item.actor ?? item).has_style_pseudo_class('checked')))"
              f"({desktop.indicator}._languagesItem)"))
    save_screenshot("language-menu.png")
    desktop_eval(f"{desktop.indicator}.menu.close()")

    choose_language("pt-BR")
    wait_until("choosing another replaces it", saved("pt-BR"))
    wait_until("in the menu too", lambda: menu_shows(BRAZILIAN_MENU_LABELS))
    router.next_configuration()

    if BRAZILIAN_LAYOUT_NAMES.exists():
        wait_until("layouts are named in the chosen language, not the desktop's",
                   lambda: '"true"' in desktop_eval(
                       f"String({desktop.indicator}._service.layoutName('br', '')"
                       f" === {json.dumps(BRAZILIAN_NAME_OF_BR)})"))
    else:
        print("skip layouts named in the chosen language: this system lacks its translations",
              flush=True)

    config_path.unlink()
    wait_until("without a saved language the menu follows the desktop again",
               lambda: menu_shows(ENGLISH_MENU_LABELS) and '"en"' in keyway_language())
    check("and layouts keep the desktop's names",
          '"desktop"' in desktop_eval(f"{desktop.indicator}._service.layoutName('br', 'desktop')"))
    router.next_configuration()


def test_shortcuts_switch_layouts(router):
    check("starts on br", active_source() == "br", active_source())

    router.send(NEXT_LAYOUT)
    wait_until("next switches to gr+simple", lambda: active_source() == "gr+simple")

    if isinstance(desktop, CinnamonDesktop):
        wait_until("Cinnamon's taskbar applet follows the switch",
                   desktop.applet_shows_second_layout)

    router.send(NEXT_LAYOUT)
    wait_until("next wraps to br", lambda: active_source() == "br")

    router.send(PREVIOUS_LAYOUT)
    wait_until("previous wraps to gr+simple", lambda: active_source() == "gr+simple")

    router.send(select_layout("br"))
    wait_until("direct shortcut selects br", lambda: active_source() == "br")

    router.drain()
    router.send(select_layout("no-such-layout"))
    router.expect_quiet("a removed layout is ignored")
    check("layout unchanged", active_source() == "br")


def test_alt_codes_reach_a_text_window(router, window_script):
    window = TestWindow(window_script, "entry")
    window.wait_for_focus(expect_text_input=True)

    def typed(text):
        return lambda: bool(window.lines) and window.lines[-1] == f"TEXT {text}"

    router.send(decimal_code("130"))
    wait_until("Alt+130 on br types é", typed("é"))

    router.send(decimal_code("0128"))
    wait_until("Alt+0128 on br types €", typed("é€"))

    router.send(decimal_code("321"))
    wait_until("Alt+321 wraps to A", typed("é€A"))

    router.send(decimal_code("0129"))
    time.sleep(QUIET_SECONDS)
    check("Alt+0129 types nothing", typed("é€A")())

    router.send(NEXT_LAYOUT)
    wait_until("switched to Greek", lambda: active_source() == "gr+simple")

    router.send(decimal_code("152"))
    wait_until("Alt+152 on Greek types α", typed("é€Aα"))

    router.send(decimal_code("0225"))
    wait_until("Alt+0225 on Greek types α", typed("é€Aαα"))

    router.send(select_layout("br"))
    wait_until("back on br", lambda: active_source() == "br")

    window.close()


def test_alt_codes_without_text_input(router, window_script):
    window = TestWindow(window_script, "keys")
    window.wait_for_focus(expect_text_input=False)

    router.send(decimal_code("97"))
    wait_until("a key on the layout reaches a window without text input",
               lambda: "KEY a" in window.lines)

    window.close()


def test_more_layouts_than_an_x_keymap_holds(router, window_script):
    """Cinnamon 6.6 chains keymaps past four layouts, so switching and typing must still work."""
    desktop.set_layouts(FIVE_LAYOUTS)
    wait_until("five layouts are listed", lambda: "'fr'" in extension_call("ListSources").stdout)

    window = TestWindow(window_script, "entry")
    window.wait_for_focus(expect_text_input=True)

    router.send(select_layout("fr"))
    wait_until("the fifth layout is selected", lambda: active_source() == "fr")
    wait_until("the X server gets the fifth layout",
               lambda: "fr" in run_quietly("setxkbmap", "-query").stdout)
    router.expect_message("fr frees Right Alt", {"set_allow_right_alt": True})

    router.send(decimal_code("0128"))
    wait_until("Alt+0128 on the fifth layout types €",
               lambda: bool(window.lines) and window.lines[-1] == "TEXT €")

    window.close()

    router.send(select_layout("br"))
    wait_until("back to the first layout", lambda: active_source() == "br")
    router.expect_message("br takes Right Alt back", {"set_allow_right_alt": False})

    desktop.set_layouts(TWO_LAYOUTS)
    wait_until("back to two layouts", lambda: "'fr'" not in extension_call("ListSources").stdout)


def shell_keys(keys):
    desktop_eval(SHELL_KEYS % keys)


def shell_popup_exists():
    return '"true"' in desktop_eval(f"String(!!{SHELL_INPUT_SOURCES}._switcherPopup)")


def test_super_space_has_no_popup():
    """Runs only on GNOME: Cinnamon 6.6 switches with no popup, and 6.4 has no such shortcut."""
    check("starts on br for Super+Space", active_source() == "br", active_source())

    shell_keys(HOLD_SUPER_TAP_SPACE)
    wait_until("Super+Space switches while still held", lambda: active_source() == "gr+simple")
    check("with no popup from the Shell", not shell_popup_exists())
    shell_keys(RELEASE_SUPER)
    wait_until("the indicator follows Super+Space", lambda: indicator_shows("gr+simple"))

    shell_keys(HOLD_SUPER_TAP_SPACE)
    wait_until("Super+Space wraps around", lambda: active_source() == "br")
    shell_keys(RELEASE_SUPER)

    shell_keys(HOLD_SHIFT_SUPER_TAP_SPACE)
    wait_until("Shift+Super+Space goes backward", lambda: active_source() == "gr+simple")
    check("also with no popup", not shell_popup_exists())
    shell_keys(RELEASE_SHIFT_SUPER)

    shell_keys(HOLD_SHIFT_SUPER_TAP_SPACE)
    wait_until("and back to br", lambda: active_source() == "br")
    shell_keys(RELEASE_SHIFT_SUPER)


def test_super_space_returns_to_the_shell():
    """With the extension off. Seeing the popup here also shows the check above can see one."""
    shell_keys(HOLD_SUPER_TAP_SPACE)
    wait_until("the Shell's popup returns without the extension", shell_popup_exists)
    shell_keys(RELEASE_SUPER)
    wait_until("and closes when Super is released", lambda: not shell_popup_exists())
    check("the Shell switched the layout", '"gr+simple"' in desktop_eval(
        f"{SHELL_INPUT_SOURCES}.currentSource.id"))

    desktop_eval(f"{SHELL_INPUT_SOURCES}.inputSources[0].activate(true)")
    wait_until("back on the first layout", lambda: '"br"' in desktop_eval(
        f"{SHELL_INPUT_SOURCES}.currentSource.id"))


def test_right_alt_follows_the_layout(router):
    desktop.set_layouts(THREE_LAYOUTS)
    wait_until("the added layout is listed", lambda: "'us'" in extension_call("ListSources").stdout)

    router.send(select_layout("us"))
    wait_until("switched to us", lambda: active_source() == "us")
    router.expect_message("us frees Right Alt", {"set_allow_right_alt": True})

    desktop.set_layouts(TWO_LAYOUTS)
    wait_until("removing the active layout falls back to br", lambda: active_source() == "br")
    router.expect_message("br takes Right Alt back", {"set_allow_right_alt": False})


def test_configuration_reloads(router, config_path):
    config_path.parent.mkdir(parents=True, exist_ok=True)

    config_path.write_text('[switching]\nnext = "Ctrl+Shift"\norder = "recent"\n')
    configuration = router.next_configuration()
    check("a new configuration reaches the router",
          read_config_section(configuration) == {"Ctrl+Shift": "next_layout"}, configuration)

    desktop.set_layouts(THREE_LAYOUTS)
    wait_until("three layouts", lambda: "'us'" in extension_call("ListSources").stdout)
    router.send(PREVIOUS_LAYOUT)
    wait_until("recent order ends with the layout not used yet", lambda: active_source() == "us")
    router.expect_message("right alt freed", {"set_allow_right_alt": True})
    router.send(select_layout("br"))
    wait_until("on br", lambda: active_source() == "br")
    router.expect_message("right alt taken", {"set_allow_right_alt": False})
    router.send(NEXT_LAYOUT)
    wait_until("recent order returns to us, not the next in the list",
               lambda: active_source() == "us")
    router.expect_message("right alt freed again", {"set_allow_right_alt": True})
    router.send(select_layout("br"))
    wait_until("on br again", lambda: active_source() == "br")
    router.expect_message("right alt taken again", {"set_allow_right_alt": False})
    desktop.set_layouts(TWO_LAYOUTS)
    wait_until("two layouts", lambda: "'us'" not in extension_call("ListSources").stdout)

    config_path.write_text('[indicator]\nflags = false\n')
    wait_until("turning flags off hides them", lambda: not indicator_flag_shown())
    check("the code stays without flags", indicator_shows("br"))
    config_path.write_text('[indicator]\nflags = true\n')
    wait_until("turning flags on shows them again", indicator_flag_shown)

    router.drain()
    config_path.write_text("this is not valid\n")
    router.expect_quiet("an invalid configuration is not sent")

    config_path.unlink()
    configuration = router.next_configuration()
    check("removing the file restores the defaults",
          read_config_section(configuration) == {"Alt+Shift": "next_layout"}, configuration)


def test_own_shortcut_leaves_the_switching(router, config_path, keyway):
    desktop.set_layouts(THREE_LAYOUTS)
    wait_until("three layouts again", lambda: "'us'" in extension_call("ListSources").stdout)

    config_path.write_text('[switching.direct]\n"gr+simple" = "Ctrl+Shift+2"\n')
    configuration = router.next_configuration()
    check("a layout's own shortcut reaches the router",
          read_config_section(configuration).get("Ctrl+Shift+2") == {"select_layout": "gr+simple"},
          configuration)

    router.send(NEXT_LAYOUT)
    wait_until("next passes over the layout with its own shortcut",
               lambda: active_source() == "us")
    router.expect_message("us frees Right Alt", {"set_allow_right_alt": True})

    router.send(PREVIOUS_LAYOUT)
    wait_until("previous passes over it too", lambda: active_source() == "br")
    router.expect_message("br takes Right Alt", {"set_allow_right_alt": False})

    run_quietly(keyway, "next")
    wait_until("and so does the command", lambda: active_source() == "us")
    router.expect_message("us frees it", {"set_allow_right_alt": True})

    router.send(select_layout("gr+simple"))
    wait_until("its own shortcut still selects it", lambda: active_source() == "gr+simple")

    router.send(PREVIOUS_LAYOUT)
    wait_until("switching leaves it for its neighbour", lambda: active_source() == "br")
    router.expect_message("br takes Right Alt back", {"set_allow_right_alt": False})

    config_path.unlink()
    router.next_configuration()
    desktop.set_layouts(TWO_LAYOUTS)
    wait_until("two layouts again", lambda: "'us'" not in extension_call("ListSources").stdout)


def test_command_line(keyway):
    status = run_quietly(keyway, "status")
    check("status marks the active layout", "* br" in status.stdout, status.stdout)

    run_quietly(keyway, "next")
    wait_until("command next", lambda: active_source() == "gr+simple")

    run_quietly(keyway, "previous")
    wait_until("command previous", lambda: active_source() == "br")

    run_quietly(keyway, "select", "gr+simple")
    wait_until("command select", lambda: active_source() == "gr+simple")

    run_quietly(keyway, "select", "br")
    wait_until("command select back", lambda: active_source() == "br")


def systemctl_stand_in(work_dir):
    """Returns an environment whose systemctl only logs its arguments, and that log. A test bus
    cannot reach the real one."""
    stand_in = work_dir / "stand-in"
    stand_in.mkdir()
    systemctl_log = work_dir / "systemctl.log"
    (stand_in / "systemctl").write_text(f'#!/bin/sh\necho "$*" >> {systemctl_log}\nexit 0\n')
    (stand_in / "systemctl").chmod(0o755)

    return {**os.environ, "PATH": f"{stand_in}:{os.environ['PATH']}"}, systemctl_log


def run_with(environment, *command):
    return subprocess.run(command, env=environment, capture_output=True, text=True, check=False)


def test_restart_loads_a_changed_extension(router, keyway, config_path, stand_in):
    """Changes the installed extension as an upgrade would, in a desktop that keeps running."""
    environment, systemctl_log = stand_in
    extension_file = Path(os.environ["XDG_DATA_HOME"]) / desktop.reloadable_file

    config_path.write_text('[indicator]\nflags = false\n')
    wait_until("flags are off before the restart", lambda: not indicator_flag_shown())
    extension_file.write_text(f"{extension_file.read_text()}\nglobalThis.{RELOAD_MARKER} = true;\n")

    result = run_with(environment, keyway, "restart")
    check("restart succeeds", result.returncode == 0, result.stderr)
    check("and says so", "Desktop extension restarted" in result.stdout, result.stdout)
    check("it restarts the agent and the router",
          systemctl_log.read_text().splitlines() == RESTART_CALLS, systemctl_log.read_text())
    wait_until("the desktop runs the changed extension",
               lambda: '"true"' in desktop_eval(f"String(globalThis.{RELOAD_MARKER} === true)"))

    check("a running agent lets go of the router meanwhile", router.wait_for_drop())
    router.close_connection()
    check("and returns to it", router.accept())
    router.handshake()
    wait_until("the changed extension replaces the desktop's switcher",
               lambda: indicator_shows("br") and not desktop_switcher_is_visible())
    check("and learns the flag choice from the agent", not indicator_flag_shown())

    config_path.unlink()
    wait_until("flags return with the defaults", indicator_flag_shown)
    router.drain()


def test_restart_without_the_loader(router, keyway, stand_in):
    """Takes the extension out of the Shell, which is then as one whose login began with an
    extension older than the loader, and follows what keyway restart prints."""
    environment, systemctl_log = stand_in
    manager = "Main.extensionManager"

    desktop_eval(f"{manager}.unloadExtension({manager}.lookup('{EXTENSION_UUID}'))")
    wait_until("the Shell forgets the extension", lambda: not keyway_indicator_exists())
    check("agent drops the router with it", router.wait_for_drop())
    router.close_connection()

    result = run_with(environment, keyway, "restart")
    lines = [line for line in result.stderr.splitlines() if line.startswith("const ")]
    check("restart fails without the loader", result.returncode == 1, result.stderr)
    check("it gives a Looking Glass line, and no new login",
          len(lines) == 1 and "Alt+F2" in result.stderr and "login" not in result.stderr,
          result.stderr)
    check("it still restarts the agent and the router",
          systemctl_log.read_text().splitlines()[-len(RESTART_CALLS):] == RESTART_CALLS,
          systemctl_log.read_text())

    desktop_eval(f"Main.createLookingGlass()._evaluate({json.dumps(lines[0])}).then(() => 'done')")
    check("the line brings the extension back, and the agent with it", router.accept())
    router.handshake()
    wait_until("it replaces the desktop's switcher again",
               lambda: indicator_shows("br") and not desktop_switcher_is_visible())

    result = run_with(environment, keyway, "restart")
    check("restart then works", result.returncode == 0, result.stderr)
    check("the agent lets go of the router for it", router.wait_for_drop())
    router.close_connection()
    check("and returns", router.accept())
    router.handshake()
    wait_until("the indicator is back after it", lambda: indicator_shows("br"))
    router.drain()


def test_extension_loss_releases_the_router(router, keyway, stand_in):
    desktop.disable_extension()
    wait_until("disabling removes Keyway's indicator", lambda: not keyway_indicator_exists())
    check("the desktop's own switcher comes back", desktop_switcher_is_visible())

    result = run_with(stand_in[0], keyway, "restart")
    check("restart says that the extension is off, and offers no way around that",
          "the extension is not running" in result.stderr and "Alt+F2" not in result.stderr,
          result.stderr)

    check("agent drops the router without the extension", router.wait_for_drop())
    router.close_connection()
    check("agent stays away while it is gone", not router.accept(timeout=QUIET_SECONDS))

    if isinstance(desktop, GnomeDesktop):
        test_super_space_returns_to_the_shell()

    desktop.enable_extension()
    check("agent returns with the extension", router.accept())
    router.handshake()
    wait_until("the desktop's switcher is replaced again", lambda: indicator_shows("br"))
    check("and hidden", not desktop_switcher_is_visible())


def test_router_loss_reconnects(router):
    router.close_connection()
    check("agent reconnects after losing the router", router.accept())
    router.handshake()


def test_stop_signal(router, agent):
    agent.send_signal(signal.SIGTERM)
    check("agent exits cleanly", agent.wait(timeout=WAIT_LIMIT_SECONDS) == 0, agent.returncode)

    try:
        check("the socket closes", router.receive() is None and False)
    except EOFError:
        check("the socket closes", True)


def test_user_setup_and_removal(keyway, stand_in):
    environment, systemctl_log = stand_in
    autostart = Path(os.environ["XDG_CONFIG_HOME"]) / "autostart" / "keyway.desktop"
    marker = Path(os.environ["XDG_STATE_HOME"]) / "keyway" / "turned-on"
    packaged = any((Path(directory) / PACKAGED_MARKER).exists()
                   for directory in os.environ.get("XDG_DATA_DIRS", "").split(":") if directory)

    def run_keyway(*arguments):
        return run_with(environment, keyway, *arguments)

    desktop.disable_extension()
    wait_until("the indicator goes with the extension", lambda: not keyway_indicator_exists())

    result = run_keyway("session-start")
    check("the first login's session entry succeeds", result.returncode == 0, result.stderr)
    check("it enables the extension", EXTENSION_UUID in desktop.enabled_extensions(),
          desktop.enabled_extensions())
    wait_until("the desktop loads it and replaces the switcher at once",
               lambda: indicator_shows("br") and not desktop_switcher_is_visible())
    check("it starts the agent",
          "--user start keyway.service" in systemctl_log.read_text())

    desktop.disable_extension()
    wait_until("the user turns the extension off", lambda: not keyway_indicator_exists())
    result = run_keyway("session-start")
    check("the next login's session entry succeeds", result.returncode == 0, result.stderr)
    check("and respects that", EXTENSION_UUID not in desktop.enabled_extensions(),
          desktop.enabled_extensions())

    result = run_keyway("setup")
    check("setup succeeds", result.returncode == 0, result.stderr)
    check("setup enables the extension", EXTENSION_UUID in desktop.enabled_extensions(),
          desktop.enabled_extensions())
    wait_until("setup overrules a user's earlier off switch", keyway_indicator_exists)

    if packaged:
        check("setup leaves the package's session entry in charge", not autostart.exists())
    else:
        check("setup adds the session start", autostart.exists())

    result = run_keyway("remove")
    check("remove succeeds", result.returncode == 0, result.stderr)
    check("remove disables the extension", EXTENSION_UUID not in desktop.enabled_extensions(),
          desktop.enabled_extensions())
    wait_until("the desktop's own switcher returns", desktop_switcher_is_visible)
    check("remove forgets that Keyway was on", not packaged or not marker.exists())

    if packaged:
        check("remove hides the package's session entry",
              "Hidden=true" in autostart.read_text())
    else:
        check("remove takes the session start away", not autostart.exists())


def main():
    global desktop

    desktop = DESKTOPS[sys.argv[1]]()
    keyway, window_script, work_dir = sys.argv[2], sys.argv[3], Path(sys.argv[4])
    runtime_dir = Path(os.environ["XDG_RUNTIME_DIR"])
    config_path = Path(os.environ["XDG_CONFIG_HOME"]) / "keyway" / "config.toml"

    check("environment is isolated",
          os.environ["GSETTINGS_BACKEND"] == "keyfile"
          and str(work_dir) in os.environ["HOME"])

    result = run_quietly(keyway, "setup", "--no-activate")
    check("setup writes the extension", result.returncode == 0, result.stderr)
    desktop.set_layouts(TWO_LAYOUTS)
    stand_in = systemctl_stand_in(work_dir)

    with (work_dir / "shell.log").open("w") as shell_log, (work_dir / "agent.log").open("w") as agent_log:
        processes = desktop.start(shell_log)
        agent = None

        try:
            wait_until("desktop is up", lambda: '"true"' in desktop_eval("String(!!global.display)"))

            router = FakeRouter(runtime_dir / SOCKET_NAME)
            agent = subprocess.Popen([keyway, "run", "--socket", str(router.path)],
                                     stdout=agent_log, stderr=subprocess.STDOUT)

            test_waits_for_extension_then_configures(router)
            test_replaces_the_desktop_switcher()
            test_taskbar_menu_opens_the_settings_window()
            test_on_screen_keyboard(router, window_script, config_path)

            if isinstance(desktop, GnomeDesktop):
                test_on_screen_keyboard_by_touch(window_script)

            test_screenshot_tools_cover_the_keyboard(window_script)
            test_language_menu(router, config_path)
            test_shortcuts_switch_layouts(router)

            if isinstance(desktop, GnomeDesktop):
                test_super_space_has_no_popup()

            test_alt_codes_reach_a_text_window(router, window_script)
            test_alt_codes_without_text_input(router, window_script)

            if isinstance(desktop, CinnamonDesktop) and not desktop.has_layout_limit:
                test_more_layouts_than_an_x_keymap_holds(router, window_script)

            test_right_alt_follows_the_layout(router)
            test_configuration_reloads(router, config_path)
            test_own_shortcut_leaves_the_switching(router, config_path, keyway)
            test_command_line(keyway)
            test_restart_loads_a_changed_extension(router, keyway, config_path, stand_in)

            if isinstance(desktop, GnomeDesktop):
                test_restart_without_the_loader(router, keyway, stand_in)

            test_extension_loss_releases_the_router(router, keyway, stand_in)
            test_router_loss_reconnects(router)
            test_stop_signal(router, agent)
            test_user_setup_and_removal(keyway, stand_in)
        finally:
            if agent is not None and agent.poll() is None:
                agent.terminate()
                agent.wait()

            for process in processes:
                process.terminate()
                process.wait()

    print("ALL HEADLESS TESTS PASSED", flush=True)


if __name__ == "__main__":
    main()
