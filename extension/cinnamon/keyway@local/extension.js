// Module: keyway.extension.cinnamon
// Purpose: Give the Keyway agent the input source and text APIs only Cinnamon has, replace the
// keyboard applet with Keyway's own indicator, and draw its on-screen keyboard.
// Created: 2026-11-08
// Architecture: A thin D-Bus adapter with no policy; crates/keyway-desktop is its client, and
// the GNOME extension offers the same interface. Cinnamon changed its layout system in 6.6, so
// two small classes cover the releases on either side of that change. The applet comes back
// whenever this is disabled.

const Clutter = imports.gi.Clutter;
const Gio = imports.gi.Gio;
const GLib = imports.gi.GLib;
const Meta = imports.gi.Meta;
const St = imports.gi.St;
const XApp = imports.gi.XApp;
const CinnamonApps = imports.gi.Cinnamon;
const Config = imports.misc.config;
const AppletManager = imports.ui.appletManager;
const Main = imports.ui.main;
const PopupMenu = imports.ui.popupMenu;

const BUS_NAME = 'org.keyway.Shell';
const OBJECT_PATH = '/org/keyway/Shell';
const INPUT_SOURCES_SCHEMA = 'org.cinnamon.desktop.input-sources';
const XKB_OPTIONS_KEY = 'xkb-options';
const XKB_VARIANT_SEPARATOR = '+';
const XKB_QUERY_COMMAND = ['setxkbmap', '-query'];
const MICROSECONDS_PER_MILLISECOND = 1000;
const KEYBOARD_APPLET_UUID = 'keyboard@cinnamon.org';
const APPLET_RETRY_MILLISECONDS = 1000;
const APPLET_RETRY_LIMIT = 30;
const SETTINGS_APP_ID = 'keyway-settings.desktop';

// Keyway's languages by the tags the configuration names them with, each under its own name, in
// the order the menu offers them. The first serves a desktop in a language that is not here.
const LANGUAGES = [
    ['en', 'English'],
    ['es', 'Español'],
    ['pt-BR', 'Português (Brasil)'],
    ['el', 'Ελληνικά'],
    ['pt-PT', 'Português (Portugal)'],
];
const PORTUGUESE = 'pt';
const PORTUGAL = 'PT';
const BRAZIL = 'BR';
const SETTINGS_LABELS = {
    'en': 'Keyway settings',
    'es': 'Configuración de Keyway',
    'pt-BR': 'Configurações do Keyway',
    'el': 'Ρυθμίσεις Keyway',
    'pt-PT': 'Definições do Keyway',
};
const KEYBOARD_LABELS = {
    'en': 'On-screen keyboard',
    'es': 'Teclado en pantalla',
    'pt-BR': 'Teclado virtual',
    'el': 'Εικονικό πληκτρολόγιο',
    'pt-PT': 'Teclado virtual',
};
const LANGUAGE_LABELS = {
    'en': 'Language',
    'es': 'Idioma',
    'pt-BR': 'Idioma',
    'el': 'Γλώσσα',
    'pt-PT': 'Idioma',
};
// The class by which a theme shades an open submenu.
const SUBMENU_SHADE_CLASS = 'popup-sub-menu';
const INDICATOR_NAME = 'keyway-indicator';
const LEFT_MOUSE_BUTTON = 1;
const ENABLED_APPLETS_CHANGED = 'changed::enabled-applets';
const LABELS_FILE = 'keyway/layout_labels.json';
const FLAGS_DIRECTORY = 'keyway/flags';
const FLAG_EXTENSION = '.svg';
const KEYBOARD_STYLESHEET = 'keyboard.css';
const CLOSE_ICON = 'window-close-symbolic';

// The keyboard's colours, as the configuration names them; each is a style class with this
// prefix. Light until the agent passes on the user's choice.
const KEYBOARD_THEMES = ['light', 'dark'];
const KEYBOARD_THEME_CLASS = 'keyway-keyboard-';

// An ordinary key's side, the space between keys and the space under the keyboard where it
// first appears, in pixels before the desktop's scale. The agent gives widths in quarter keys.
const KEY_SIZE = 46;
const KEY_SPACING = 5;
const KEYBOARD_BOTTOM_MARGIN = 28;
const QUARTERS_PER_KEY = 4;

// Icon sizes in the keyboard's title, which this toolkit takes from the code, not the style.
const KEYBOARD_FLAG_SIZE = 18;
const CLOSE_ICON_SIZE = 14;

// Cinnamon before 6.8 hands a key code to the X server as it is, and the server's codes are the
// evdev ones plus eight. From 6.8, and on Wayland, its keyboard device takes evdev codes.
const X_KEY_CODE_OFFSET = 8;
const FIRST_VERSION_TAKING_EVDEV_CODES = [6, 8];

// Key roles, as keyway-core names them. A latching key stays down until the next plain key.
const GAP_ROLE = 'gap';
const PLAIN_ROLE = 'plain';
const SHIFT_ROLE = 'shift';
const LEVEL3_ROLE = 'level3';
const CAPS_LOCK_ROLE = 'caps_lock';
const SHIFTED_ROLE = 'shifted';
const LATCHING_ROLES = ['modifier', SHIFT_ROLE, LEVEL3_ROLE];
// After a key of these roles, the modifiers held for it go up.
const TYPING_ROLES = [PLAIN_ROLE, SHIFTED_ROLE];
// The Shift that a key of the shifted role holds while it is down: the right one's evdev code.
const SHIFT_KEY_CODE = 54;

// A character key has a label for each combination of these, at the sum of the active ones.
const SHIFT_LABELS = 1;
const LEVEL3_LABELS = 2;
const CAPS_LOCK_LABELS = 4;

// The modifiers that choose those labels: Shift, and AltGr, which every layout has on Mod5.
const SHIFT_MASK = Clutter.ModifierType.SHIFT_MASK;
const LEVEL3_MASK = Clutter.ModifierType.MOD5_MASK;
const CONTROL_MASK = Clutter.ModifierType.CONTROL_MASK;
const ALT_MASK = Clutter.ModifierType.MOD1_MASK;
const SUPER_MASK = Clutter.ModifierType.MOD4_MASK | Clutter.ModifierType.SUPER_MASK;
// The modifier that each latching key holds, by evdev code. Right Alt holds AltGr instead
// where it has that role.
const KEY_MODIFIERS = new Map([[42, SHIFT_MASK], [54, SHIFT_MASK], [29, CONTROL_MASK],
    [97, CONTROL_MASK], [56, ALT_MASK], [100, ALT_MASK], [125, SUPER_MASK], [126, SUPER_MASK]]);

function modifierOf(key) {
    return key.role === LEVEL3_ROLE ? LEVEL3_MASK : KEY_MODIFIERS.get(key.code) ?? 0;
}
// How often the keyboard looks at them while shown, as an X server tells nobody of a change.
const MODIFIER_POLL_MS = 100;

// What a key types with one more modifier, shown small in a corner of the key. Nothing for the
// same letter in the other case, which the key's own label already tells.
function hintLabel(labels, state, modifier) {
    const shown = labels[state] ?? '';
    const other = labels[state ^ modifier] ?? '';

    return other.toUpperCase() === shown.toUpperCase() ? '' : other;
}

// Flag sizes before the desktop's scale, as Cinnamon's own keyboard applet draws them in its
// menu, and in the panel until the panel's own icon size is known.
const MENU_FLAG_SIZE = 22;
const PANEL_FLAG_SIZE = 16;
const FLAG_NAME_SPACING = 'spacing: 8px;';

// The side of a menu that faces its panel, by the panel's position: top, bottom, left, right.
const MENU_SIDES = [St.Side.TOP, St.Side.BOTTOM, St.Side.LEFT, St.Side.RIGHT];

const INTERFACE_XML = `
<node>
  <interface name="org.keyway.Shell">
    <method name="ListSources">
      <arg type="a(ssss)" name="sources" direction="out"/>
    </method>
    <method name="ActiveSource">
      <arg type="s" name="source_id" direction="out"/>
    </method>
    <method name="XkbOptions">
      <arg type="as" name="options" direction="out"/>
    </method>
    <method name="Activate">
      <arg type="s" name="source_id" direction="in"/>
    </method>
    <method name="CommitText">
      <arg type="s" name="text" direction="in"/>
    </method>
    <method name="ShowFlags">
      <arg type="b" name="shown" direction="in"/>
    </method>
    <method name="SetKeyboard">
      <arg type="aa(uusas)" name="rows" direction="in"/>
    </method>
    <method name="SetKeyboardTheme">
      <arg type="s" name="theme" direction="in"/>
    </method>
    <method name="SetLanguage">
      <arg type="s" name="language" direction="in"/>
      <arg type="a{ss}" name="layout_names" direction="in"/>
    </method>
    <signal name="LanguageChosen">
      <arg type="s" name="language"/>
    </signal>
    <signal name="ActiveSourceChanged">
      <arg type="s" name="source_id"/>
    </signal>
    <signal name="SourcesChanged"/>
  </interface>
</node>`;

// Reads one field of setxkbmap -query, such as the layouts in "layout:     br,gr".
function xkbQueryField(output, name) {
    const line = output.split('\n').find(candidate => candidate.startsWith(`${name}:`));

    return line ? line.slice(name.length + 1).trim().split(',') : [];
}

// Cinnamon 6.6 and later: layouts are input sources, as they are in GNOME Shell.
class InputSourceLayouts {
    constructor(manager, onSourcesChanged, onActiveChanged) {
        this._manager = manager;
        this._settings = new Gio.Settings({schema_id: INPUT_SOURCES_SCHEMA});
        this._activeChangedId = manager.connect('current-source-changed', onActiveChanged);
        this._sourcesChangedId = manager.connect('sources-changed', onSourcesChanged);
    }

    whenReady(ready) {
        ready();
    }

    destroy() {
        this._manager.disconnect(this._activeChangedId);
        this._manager.disconnect(this._sourcesChangedId);
    }

    _sources() {
        const sources = Object.values(this._manager.inputSources);

        return sources.sort((first, second) => first.index - second.index);
    }

    list() {
        return this._sources().map(source =>
            [source.id, source.xkbId, source.displayName, source.shortName]);
    }

    activeId() {
        return this._manager.currentSource?.id ?? '';
    }

    xkbOptions() {
        return this._settings.get_strv(XKB_OPTIONS_KEY);
    }

    activate(id) {
        const source = this._sources().find(candidate => candidate.id === id);

        if (!source)
            throw new Error(`Unknown input source: ${id}`);

        source.activate();
    }
}

// Cinnamon 6.2 and 6.4: layouts are XKB groups. XApp names them and selects one, but only the
// X server knows which layout and variant each group is, and those identify it to the agent.
class XkbGroupLayouts {
    constructor(onSourcesChanged, onActiveChanged) {
        this._controller = new XApp.KbdLayoutController();
        this._ids = [];
        this._options = [];
        this._onSourcesChanged = onSourcesChanged;
        this._activeChangedId = this._controller.connect('layout-changed', onActiveChanged);
        this._configChangedId = this._controller.connect('config-changed',
            () => this._readServerLayouts(onSourcesChanged));
    }

    whenReady(ready) {
        this._readServerLayouts(ready);
    }

    destroy() {
        this._controller.disconnect(this._activeChangedId);
        this._controller.disconnect(this._configChangedId);
        this._controller.run_dispose();
    }

    // Asynchronous, because the desktop's compositor must never wait on another process.
    _readServerLayouts(done) {
        const process = Gio.Subprocess.new(XKB_QUERY_COMMAND, Gio.SubprocessFlags.STDOUT_PIPE);

        process.communicate_utf8_async(null, null, (finished, result) => {
            const [, output] = finished.communicate_utf8_finish(result);
            const variants = xkbQueryField(output, 'variant');

            this._ids = xkbQueryField(output, 'layout').map((layout, index) =>
                (variants[index] ? `${layout}${XKB_VARIANT_SEPARATOR}${variants[index]}` : layout));
            this._options = xkbQueryField(output, 'options');
            done();
        });
    }

    // The controller drops a removed group before the server's layouts are read again, so it
    // may have no name or label for the last ones here.
    list() {
        const names = this._controller.get_all_names();

        return this._ids.map((id, group) => [id, id, names[group] ?? id,
            this._controller.get_short_group_label_for_group(group) ?? id]);
    }

    activeId() {
        return this._ids[this._controller.get_current_group()] ?? '';
    }

    xkbOptions() {
        return this._options;
    }

    activate(id) {
        const group = this._ids.indexOf(id);

        if (group < 0)
            throw new Error(`Unknown input source: ${id}`);

        this._controller.set_current_group(group);
    }
}

// The layout system of the running Cinnamon. Only 6.6 and later have this module.
function createLayouts(onSourcesChanged, onActiveChanged) {
    let keyboardManager = null;

    try {
        keyboardManager = imports.ui.keyboardManager;
    } catch (_error) {
        return new XkbGroupLayouts(onSourcesChanged, onActiveChanged);
    }

    return new InputSourceLayouts(keyboardManager.getInputSourceManager(),
        onSourcesChanged, onActiveChanged);
}

// Returns what to add to an evdev key code for this Cinnamon's keyboard device.
function keyCodeOffset() {
    if (Meta.is_wayland_compositor())
        return 0;

    const [major, minor] = Config.PACKAGE_VERSION.split('.').map(Number);
    const [firstMajor, firstMinor] = FIRST_VERSION_TAKING_EVDEV_CODES;
    const takesEvdevCodes = major > firstMajor || (major === firstMajor && minor >= firstMinor);

    return takesEvdevCodes ? 0 : X_KEY_CODE_OFFSET;
}

class KeywayService {
    // The listener's sourcesChanged and activeChanged are called as Cinnamon reports each,
    // flagsChanged when the agent passes on the user's choice, keyboardChanged when it describes
    // the active layout's keys, keyboardThemeChanged when it names their colours, and
    // languageChanged when it names the language of the texts.
    constructor(listener) {
        this._listener = listener;
        this.flagsShown = true;
        this.keyboardRows = [];
        [this.keyboardTheme] = KEYBOARD_THEMES;
        this.language = desktopLanguage();
        this._layoutNames = {};
        this._keyCodeOffset = keyCodeOffset();
        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE_XML, this);
        this._layouts = createLayouts(
            () => {
                listener.sourcesChanged();
                this._dbus.emit_signal('SourcesChanged', null);
            },
            () => {
                listener.activeChanged();
                this._dbus.emit_signal('ActiveSourceChanged',
                    new GLib.Variant('(s)', [this.ActiveSource()]));
            });

        const seat = Clutter.get_default_backend().get_default_seat();
        this._virtualKeyboard = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);

        this._dbus.export(Gio.DBus.session, OBJECT_PATH);
    }

    whenReady(ready) {
        this._layouts.whenReady(ready);
    }

    destroy() {
        this._layouts.destroy();
        this._dbus.unexport();
        this._virtualKeyboard.run_dispose();
    }

    // Returns id, XKB id, display name and short name, in the desktop's switching order.
    ListSources() {
        return this._layouts.list();
    }

    ActiveSource() {
        return this._layouts.activeId();
    }

    XkbOptions() {
        return this._layouts.xkbOptions();
    }

    Activate(sourceId) {
        this._layouts.activate(sourceId);
    }

    // Cinnamon's input method has no client to commit to, so text always goes as key events.
    // On X11 the keyboard device can borrow a key for a character the layout does not have.
    CommitText(text) {
        const time = Clutter.get_current_event_time() * MICROSECONDS_PER_MILLISECOND;

        for (const character of text) {
            const keyval = Clutter.unicode_to_keysym(character.codePointAt(0));

            this._virtualKeyboard.notify_keyval(time, keyval, Clutter.KeyState.PRESSED);
            this._virtualKeyboard.notify_keyval(time, keyval, Clutter.KeyState.RELEASED);
        }
    }

    ShowFlags(shown) {
        if (shown === this.flagsShown)
            return;

        this.flagsShown = shown;
        this._listener.flagsChanged();
    }

    // Each row lists its places from the left: evdev key code, width in quarters of a key, role
    // and labels.
    SetKeyboard(rows) {
        this.keyboardRows = rows;
        this._listener.keyboardChanged();
    }

    SetKeyboardTheme(theme) {
        if (!KEYBOARD_THEMES.includes(theme))
            throw new Error(`Unknown keyboard theme: ${theme}`);

        this.keyboardTheme = theme;
        this._listener.keyboardThemeChanged();
    }

    // An empty language means the desktop's. The names are those of layouts in the language,
    // by source id; a layout without one keeps the name the desktop gives it.
    SetLanguage(language, layoutNames) {
        if (language && !LANGUAGES.some(([tag]) => tag === language))
            throw new Error(`Unknown language: ${language}`);

        this.language = language || desktopLanguage();
        this._layoutNames = layoutNames;
        this._listener.languageChanged();
    }

    // Passes the user's choice on to the agent, which saves it and answers with SetLanguage.
    chooseLanguage(language) {
        this._dbus.emit_signal('LanguageChosen', new GLib.Variant('(s)', [language]));
    }

    layoutName(id, displayName) {
        return this._layoutNames[id] ?? displayName;
    }

    // Presses or releases a key by its evdev code, as a keyboard would; the active layout
    // decides what it types.
    notifyKey(keyCode, pressed) {
        const time = Clutter.get_current_event_time() * MICROSECONDS_PER_MILLISECOND;
        const state = pressed ? Clutter.KeyState.PRESSED : Clutter.KeyState.RELEASED;

        this._virtualKeyboard.notify_key(time, keyCode + this._keyCodeOffset, state);
    }
}

// Each layout's label and flag, from the table that keyway setup or the package installs. A
// layout missing from it, such as an input method engine, keeps the desktop's short name.
class LayoutLabels {
    constructor() {
        this._layouts = {};
        this._flagsDirectory = null;

        const directories = [GLib.get_user_data_dir(), ...GLib.get_system_data_dirs()];
        const directory = directories.find(candidate =>
            GLib.file_test(GLib.build_filenamev([candidate, LABELS_FILE]), GLib.FileTest.IS_REGULAR));

        if (!directory)
            return;

        const [, contents] = GLib.file_get_contents(GLib.build_filenamev([directory, LABELS_FILE]));

        this._layouts = JSON.parse(new TextDecoder().decode(contents)).layouts;
        this._flagsDirectory = GLib.build_filenamev([directory, FLAGS_DIRECTORY]);
    }

    // Returns the layout's code, such as EL-GR, which its variants share.
    text(id, shortName) {
        return this._layouts[id]?.code ?? shortName.toUpperCase();
    }

    // Returns the flag as an icon, or null for a layout that has none, such as Braille.
    flag(id) {
        const flag = this._layouts[id]?.flag;

        if (!flag)
            return null;

        const path = GLib.build_filenamev([this._flagsDirectory, `${flag}${FLAG_EXTENSION}`]);

        return new Gio.FileIcon({file: Gio.File.new_for_path(path)});
    }
}

// Returns the first of the desktop's languages that Keyway has, as one of its tags. Plain
// Portuguese is Brazil's, as in the settings window.
function desktopLanguage() {
    for (const name of GLib.get_language_names()) {
        const [language, region] = name.split(/[_.@]/);
        const tag = language === PORTUGUESE
            ? `${PORTUGUESE}-${region === PORTUGAL ? PORTUGAL : BRAZIL}`
            : language;

        if (LANGUAGES.some(([known]) => known === tag))
            return tag;
    }

    return LANGUAGES[0][0];
}

// The keyboard's look, from the file of that name beside this one. Cinnamon would load a file
// called stylesheet.css by itself; this name is the one the GNOME extension must use.
class KeyboardStyle {
    constructor(extensionDirectory) {
        this._path = GLib.build_filenamev([extensionDirectory, KEYBOARD_STYLESHEET]);

        try {
            this._theme().load_stylesheet(this._path);
        } catch (error) {
            global.logError(error);
            this._path = null;
        }
    }

    // Cinnamon makes a new theme when its own style changes, and carries this stylesheet over.
    _theme() {
        return St.ThemeContext.get_for_stage(global.stage).get_theme();
    }

    destroy() {
        if (this._path)
            this._theme().unload_stylesheet(this._path);
    }
}

// The on-screen keyboard: the active layout's keys as the agent lays them out, in a panel that
// can be moved by its title. Cinnamon's own keyboard device presses a clicked key, and a click
// on something Cinnamon draws takes no focus, so the window being typed in receives the key.
class KeyboardViewer {
    constructor(service, labels) {
        this._service = service;
        this._labels = labels;
        this._panel = null;
        this._keys = [];
        this._held = new Set();
        this._latched = new Set();
        this._position = null;
        this._drag = null;

        this._keymap = Clutter.get_default_backend().get_default_seat().get_keymap();
        this._keymapId = this._keymap.connect('state-changed', () => this._showLabels());
        this._focusId = global.display.connect('notify::focus-window', () => this._giveWay());
    }

    destroy() {
        this.hide();
        this._keymap.disconnect(this._keymapId);
        global.display.disconnect(this._focusId);
    }

    toggle() {
        if (this._panel)
            this.hide();
        else
            this.show();
    }

    show() {
        if (this._panel)
            return;

        this._flag = new St.Icon({style_class: 'keyway-keyboard-flag',
            icon_size: Math.round(KEYBOARD_FLAG_SIZE * global.ui_scale)});
        this._name = new St.Label({style_class: 'keyway-keyboard-name',
            y_align: Clutter.ActorAlign.CENTER});

        // The handle moves the keyboard. The close button is beside it, not in it, so that a
        // press on the button never starts a move.
        const handle = new St.BoxLayout({style_class: 'keyway-keyboard-handle', reactive: true});
        const close = new St.Button({style_class: 'keyway-keyboard-close',
            child: new St.Icon({style_class: 'keyway-keyboard-close-icon', icon_name: CLOSE_ICON,
                icon_type: St.IconType.SYMBOLIC,
                icon_size: Math.round(CLOSE_ICON_SIZE * global.ui_scale)})});
        const title = new St.BoxLayout({style_class: 'keyway-keyboard-title'});

        handle.add_child(this._flag);
        handle.add_child(this._name);
        title.add(handle, {expand: true});
        title.add_child(close);
        close.connect('clicked', () => this.hide());
        handle.connect('button-press-event', (_actor, event) => this._startDrag(handle, event));
        handle.connect('motion-event', (_actor, event) => this._dragTo(event));
        handle.connect('button-release-event', () => this._endDrag());

        this._keyArea = new St.Widget();
        this._panel = new St.BoxLayout({style_class: 'keyway-keyboard', reactive: true,
            vertical: true});
        this._panel.add_child(title);
        this._panel.add_child(this._keyArea);

        // Above windows, full screen ones included, and part of what takes clicks.
        Main.layoutManager.addChrome(this._panel, {visibleInFullscreen: true});

        this._giveWay();
        this.showTheme();
        this.showLayout();
        this.showKeys();

        this._followed = 0;
        this._followId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, MODIFIER_POLL_MS, () => {
            this._followModifiers();

            return GLib.SOURCE_CONTINUE;
        });
    }

    hide() {
        if (!this._panel)
            return;

        GLib.source_remove(this._followId);
        this._releaseAll();
        this._endDrag();
        Main.layoutManager.removeChrome(this._panel);
        this._panel.destroy();
        this._panel = null;
        this._keys = [];
    }

    // A full screen window kept above the others is a screenshot tool showing a picture of the
    // screen, keyboard included. Cinnamon draws the keyboard above any window, so it hides.
    _giveWay() {
        if (!this._panel)
            return;

        const window = global.display.focus_window;

        this._panel.visible = !(window?.is_fullscreen() && window.is_above());
    }

    // Gives the keyboard the colours the user chose.
    showTheme() {
        if (!this._panel)
            return;

        for (const theme of KEYBOARD_THEMES)
            this._panel.remove_style_class_name(`${KEYBOARD_THEME_CLASS}${theme}`);

        this._panel.add_style_class_name(`${KEYBOARD_THEME_CLASS}${this._service.keyboardTheme}`);
    }

    // Names the active layout in the title, with its flag when the indicator shows flags.
    showLayout() {
        if (!this._panel)
            return;

        const activeId = this._service.ActiveSource();
        const active = this._service.ListSources().find(([id]) => id === activeId);
        const flag = this._service.flagsShown ? this._labels.flag(activeId) : null;

        this._name.text = active ? this._service.layoutName(activeId, active[2]) : '';
        this._flag.gicon = flag;
        this._flag.visible = Boolean(flag);
    }

    // Draws the keys the agent last described, each row from the left.
    showKeys() {
        if (!this._panel)
            return;

        this._releaseAll();
        this._keyArea.destroy_all_children();
        this._keys = [];

        const rowHeight = KEY_SIZE * global.ui_scale;
        const quarter = rowHeight / QUARTERS_PER_KEY;
        const spacing = KEY_SPACING * global.ui_scale;
        const rows = this._service.keyboardRows;
        let widest = 0;

        rows.forEach((row, rowIndex) => {
            let quarters = 0;

            for (const [code, width, role, labels] of row) {
                const left = Math.round(quarters * quarter);
                const right = Math.round((quarters + width) * quarter);

                if (role !== GAP_ROLE) {
                    this._addKey({code, role, labels}, left, rowIndex * rowHeight,
                        right - left - spacing, rowHeight - spacing);
                }

                quarters += width;
            }

            widest = Math.max(widest, quarters);
        });

        this._keyArea.set_size(Math.max(Math.round(widest * quarter) - spacing, 0),
            Math.max(rows.length * rowHeight - spacing, 0));
        this._showLabels();
        this._place();
    }

    _addKey(key, x, y, width, height) {
        const corner = y_align => new St.Label({style_class: 'keyway-key-hint',
            x_expand: true, y_expand: true, x_align: Clutter.ActorAlign.END, y_align});

        key.label = new St.Label({x_expand: true, y_expand: true,
            x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
        key.shiftHint = corner(Clutter.ActorAlign.START);
        key.level3Hint = corner(Clutter.ActorAlign.END);
        key.actor = new St.Widget({style_class: 'keyway-key', reactive: true, track_hover: true,
            layout_manager: new Clutter.BinLayout()});

        for (const label of [key.label, key.shiftHint, key.level3Hint])
            key.actor.add_child(label);

        // A key with one label types nothing that the layout decides, unless it is a
        // character the layout has on Shift.
        if (key.labels.length === 1 && key.role !== SHIFTED_ROLE)
            key.actor.add_style_class_name('keyway-key-named');

        key.actor.set_position(x, y);
        key.actor.set_size(width, height);
        // A touch arrives as these pointer events too, from the X server.
        key.actor.connect('button-press-event', (_actor, event) => this._press(key, event));
        key.actor.connect('button-release-event', () => this._release(key));
        key.actor.connect('leave-event', () => this._release(key));

        this._keyArea.add_child(key.actor);
        this._keys.push(key);
    }

    _isLatched(role) {
        return this._keys.some(key => key.role === role && this._latched.has(key.code));
    }

    // The modifiers down on any keyboard, the real one included.
    _heldModifiers() {
        const [, , modifiers] = global.get_pointer();

        return modifiers;
    }

    // Shows the keys again once a modifier went down or up on any keyboard.
    _followModifiers() {
        const followed = this._heldModifiers() &
            (SHIFT_MASK | LEVEL3_MASK | CONTROL_MASK | ALT_MASK | SUPER_MASK);

        if (followed === this._followed)
            return;

        this._followed = followed;
        this._showLabels();
    }

    // Shows each key's label for the modifiers held here or on the real keyboard and for the
    // desktop's Caps Lock, and lights the modifiers that are held.
    _showLabels() {
        if (!this._panel)
            return;

        const capsLock = this._keymap.get_caps_lock_state();
        const modifiers = this._heldModifiers();
        const shift = this._isLatched(SHIFT_ROLE) || (modifiers & SHIFT_MASK) !== 0;
        const level3 = this._isLatched(LEVEL3_ROLE) || (modifiers & LEVEL3_MASK) !== 0;
        const state = (shift ? SHIFT_LABELS : 0) + (level3 ? LEVEL3_LABELS : 0) +
            (capsLock ? CAPS_LOCK_LABELS : 0);

        for (const key of this._keys) {
            const fixed = key.labels.length === 1;

            key.label.text = key.labels[fixed ? 0 : state] ?? '';
            key.shiftHint.text = fixed ? '' : hintLabel(key.labels, state, SHIFT_LABELS);
            key.level3Hint.text = fixed ? '' : hintLabel(key.labels, state, LEVEL3_LABELS);

            if (key.role === CAPS_LOCK_ROLE)
                this._mark(key, 'checked', capsLock);

            // A modifier is lit while it is held, here or on the real keyboard.
            if (LATCHING_ROLES.includes(key.role)) {
                this._mark(key, 'checked',
                    this._latched.has(key.code) || (modifiers & modifierOf(key)) !== 0);
            }
        }
    }

    _mark(key, pseudoClass, marked) {
        if (marked)
            key.actor.add_style_pseudo_class(pseudoClass);
        else
            key.actor.remove_style_pseudo_class(pseudoClass);
    }

    _press(key, event) {
        if (event.get_button() !== LEFT_MOUSE_BUTTON)
            return Clutter.EVENT_PROPAGATE;

        if (LATCHING_ROLES.includes(key.role)) {
            this._setLatched(key, !this._latched.has(key.code));
            this._showLabels();

            return Clutter.EVENT_STOP;
        }

        this._held.add(key);
        this._mark(key, 'active', true);

        // A character from the layout's Shift needs Shift down, unless it already is here.
        key.holdsShift = key.role === SHIFTED_ROLE && !this._isLatched(SHIFT_ROLE);

        if (key.holdsShift)
            this._service.notifyKey(SHIFT_KEY_CODE, true);

        this._service.notifyKey(key.code, true);

        return Clutter.EVENT_STOP;
    }

    // Also when the pointer leaves a key, so that no key stays down behind it.
    _release(key) {
        if (!this._held.delete(key))
            return Clutter.EVENT_PROPAGATE;

        this._mark(key, 'active', false);
        this._service.notifyKey(key.code, false);

        if (key.holdsShift)
            this._service.notifyKey(SHIFT_KEY_CODE, false);

        // The modifiers held for this key have done their work.
        if (TYPING_ROLES.includes(key.role))
            this._releaseLatched();

        return Clutter.EVENT_STOP;
    }

    _setLatched(key, latched) {
        if (latched)
            this._latched.add(key.code);
        else
            this._latched.delete(key.code);

        this._mark(key, 'checked', latched);
        this._service.notifyKey(key.code, latched);
    }

    _releaseLatched() {
        for (const key of this._keys) {
            if (this._latched.has(key.code))
                this._setLatched(key, false);
        }

        this._showLabels();
    }

    // Nothing this keyboard pressed may stay down once its keys are gone.
    _releaseAll() {
        for (const key of this._keys)
            this._release(key);

        this._releaseLatched();
    }

    // The pointer's device sends everything to the handle until the button is released.
    _startDrag(handle, event) {
        if (event.get_button() !== LEFT_MOUSE_BUTTON)
            return Clutter.EVENT_PROPAGATE;

        const [pointerX, pointerY] = event.get_coords();

        this._drag = {
            offsetX: pointerX - this._panel.x,
            offsetY: pointerY - this._panel.y,
            device: event.get_device(),
        };
        this._drag.device.grab(handle);

        return Clutter.EVENT_STOP;
    }

    _dragTo(event) {
        if (!this._drag)
            return Clutter.EVENT_PROPAGATE;

        const [pointerX, pointerY] = event.get_coords();

        this._position = [pointerX - this._drag.offsetX, pointerY - this._drag.offsetY];
        this._place();

        return Clutter.EVENT_STOP;
    }

    _endDrag() {
        this._drag?.device.ungrab();
        this._drag = null;

        return Clutter.EVENT_PROPAGATE;
    }

    // Where the user left it, or else centred at the bottom of the primary monitor's free area.
    // Always wholly on the screen, so the title stays within reach.
    _place() {
        const [, , width, height] = this._panel.get_preferred_size();
        const workspace = global.workspace_manager.get_active_workspace();
        const area = workspace.get_work_area_for_monitor(global.display.get_primary_monitor());
        const [x, y] = this._position ?? [
            area.x + (area.width - width) / 2,
            area.y + area.height - height - KEYBOARD_BOTTOM_MARGIN * global.ui_scale,
        ];
        const within = (value, limit) => Math.round(Math.max(0, Math.min(value, limit)));

        this._panel.set_position(within(x, global.stage.width - width),
            within(y, global.stage.height - height));
    }
}

// The layout switcher in the panel: the active layout's flag and label, such as PT-BR, and a
// menu of the layouts by their full names, the on-screen keyboard, the languages of Keyway's
// texts and the settings window.
class KeywayIndicator {
    constructor(service, labels, viewer) {
        this._service = service;
        this._labels = labels;
        this._viewer = viewer;
        this._items = new Map();

        this.actor = new St.BoxLayout({name: INDICATOR_NAME, style_class: 'applet-box',
            reactive: true, track_hover: true});
        this.actor._delegate = this;
        this.actor.add_style_class_name('panel-status-button');

        this._flag = new St.Icon({style_class: 'applet-icon', y_align: Clutter.ActorAlign.CENTER,
            icon_size: Math.round(PANEL_FLAG_SIZE * global.ui_scale)});
        this._label = new St.Label({style_class: 'applet-label', y_align: Clutter.ActorAlign.CENTER});
        this.actor.add_child(this._flag);
        this.actor.add_child(this._label);

        this.menu = new PopupMenu.PopupMenu(this.actor, St.Side.TOP);
        Main.uiGroup.add_actor(this.menu.actor);
        this.menu.actor.hide();
        this.menu.connect('open-state-changed',
            (_menu, open) => this.actor.change_style_pseudo_class('checked', open));

        this._menuManager = new PopupMenu.PopupMenuManager(this);
        this._menuManager.addMenu(this.menu);

        this._layoutSection = new PopupMenu.PopupMenuSection();
        this.menu.addMenuItem(this._layoutSection);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._keyboardItem = this.menu.addAction('', () => viewer.toggle());
        this._languagesItem = new PopupMenu.PopupSubMenuMenuItem('');
        // Without the shade a theme gives an open submenu: the languages look like the menu's
        // other items.
        this._languagesItem.menu.actor.remove_style_class_name(SUBMENU_SHADE_CLASS);
        this.menu.addMenuItem(this._languagesItem);
        this._settingsItem = this.menu.addAction('',
            () => CinnamonApps.AppSystem.get_default().lookup_app(SETTINGS_APP_ID)?.activate());

        this._languageItems = new Map();

        for (const [tag, name] of LANGUAGES) {
            const item = new PopupMenu.PopupMenuItem(name);

            item.connect('activate', () => service.chooseLanguage(tag));
            this._languagesItem.menu.addMenuItem(item);
            this._languageItems.set(tag, item);
        }

        this._showMenuTexts();

        this.actor.connect('button-press-event', (_actor, event) => {
            if (event.get_button() !== LEFT_MOUSE_BUTTON)
                return Clutter.EVENT_PROPAGATE;

            this.menu.toggle();

            return Clutter.EVENT_STOP;
        });

        Main.panel?._rightBox.insert_child_at_index(this.actor, 0);
    }

    destroy() {
        this.menu.destroy();
        this.actor.destroy();
    }

    // Moves to just after an applet, in its panel's direction, with a flag of its icon size.
    placeAfter(applet) {
        this.actor.get_parent()?.remove_child(this.actor);
        applet.actor.get_parent().insert_child_above(this.actor, applet.actor);
        this.menu.setOrientation(MENU_SIDES[applet.panel.panelPosition]);
        this._flag.icon_size = applet.getPanelIconSize(St.IconType.SYMBOLIC) ?? this._flag.icon_size;
    }

    _showMenuTexts() {
        const language = this._service.language;

        this._keyboardItem.label.text = KEYBOARD_LABELS[language];
        this._languagesItem.label.text = LANGUAGE_LABELS[language];
        this._settingsItem.label.text = SETTINGS_LABELS[language];

        for (const [tag, item] of this._languageItems)
            item.setShowDot(tag === language);
    }

    // Puts the menu's own texts and the layouts' names in the language Keyway uses.
    showLanguage() {
        this._showMenuTexts();
        this.showLayouts();
    }

    showLayouts() {
        this._layoutSection.removeAll();
        this._items.clear();

        const flagsShown = this._service.flagsShown;

        for (const [id, , displayName, shortName] of this._service.ListSources()) {
            const text = this._labels.text(id, shortName);
            const flag = flagsShown ? this._labels.flag(id) : null;
            const item = new PopupMenu.PopupBaseMenuItem();
            const layoutName = this._service.layoutName(id, displayName);

            // One column for flag and name, because the menu sizes each column to its widest
            // item, settings included. A layout without a flag keeps the space, so names line up.
            const name = new St.BoxLayout({style: FLAG_NAME_SPACING});

            if (flagsShown) {
                name.add_child(new St.Icon({style_class: 'popup-menu-icon', gicon: flag,
                    icon_size: Math.round(MENU_FLAG_SIZE * global.ui_scale)}));
            }

            name.add_child(new St.Label({text: layoutName, y_align: Clutter.ActorAlign.CENTER}));
            item.addActor(name);
            item.connect('activate', () => this._service.Activate(id));

            this._layoutSection.addMenuItem(item);
            this._items.set(id, {item, text, flag});
        }

        this.showActive();
    }

    showActive() {
        const activeId = this._service.ActiveSource();
        const active = this._items.get(activeId);

        this._label.text = active?.text ?? '';
        this._flag.gicon = active?.flag ?? null;
        this._flag.visible = Boolean(active?.flag);

        for (const [id, {item}] of this._items)
            item.setShowDot(id === activeId);
    }
}

// Keeps the keyboard applet hidden, with the indicator in its place. Applets and extensions load
// side by side at startup, so it may not exist yet, the panel may not have one at all, and it is
// created anew when the panel's applets change. It also shows itself again as layouts change.
class KeyboardAppletReplacement {
    constructor(indicator) {
        this._indicator = indicator;
        this._appletActor = null;
        this._retryId = 0;
        this._appletsChangedId = global.settings.connect(ENABLED_APPLETS_CHANGED,
            () => this._lookForApplet());

        this._lookForApplet();
    }

    _lookForApplet() {
        this._stopLooking();

        if (this._replaceApplet())
            return;

        let attempts = 0;

        this._retryId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, APPLET_RETRY_MILLISECONDS, () => {
            attempts += 1;

            if (this._replaceApplet() || attempts >= APPLET_RETRY_LIMIT) {
                this._retryId = 0;

                return GLib.SOURCE_REMOVE;
            }

            return GLib.SOURCE_CONTINUE;
        });
    }

    _stopLooking() {
        if (this._retryId)
            GLib.source_remove(this._retryId);

        this._retryId = 0;
    }

    _replaceApplet() {
        const [applet] = AppletManager.getRunningInstancesForUuid(KEYBOARD_APPLET_UUID);

        if (!applet)
            return false;

        if (applet.actor === this._appletActor)
            return true;

        this._release(true);
        this._indicator.placeAfter(applet);

        const actor = applet.actor;

        this._appletActor = actor;
        this._visibleId = actor.connect('notify::visible', () => {
            if (actor.visible)
                actor.hide();
        });
        this._destroyId = actor.connect('destroy', () => {
            this._appletActor = null;
        });

        actor.hide();

        return true;
    }

    _release(visible) {
        if (!this._appletActor)
            return;

        this._appletActor.disconnect(this._visibleId);
        this._appletActor.disconnect(this._destroyId);
        this._appletActor.visible = visible;
        this._appletActor = null;
    }

    // The applet shows itself only when there is something to switch between.
    destroy(appletVisible) {
        this._stopLooking();
        global.settings.disconnect(this._appletsChangedId);
        this._release(appletVisible);
    }
}

let extensionDirectory = null;
let service = null;
let keyboardStyle = null;
let viewer = null;
let indicator = null;
let appletReplacement = null;
let nameId = 0;

function init(metadata) {
    extensionDirectory = metadata.path;
}

function enable() {
    const starting = new KeywayService({
        sourcesChanged: () => {
            indicator?.showLayouts();
            viewer?.showLayout();
        },
        activeChanged: () => {
            indicator?.showActive();
            viewer?.showLayout();
        },
        flagsChanged: () => {
            indicator?.showLayouts();
            viewer?.showLayout();
        },
        keyboardChanged: () => viewer?.showKeys(),
        keyboardThemeChanged: () => viewer?.showTheme(),
        languageChanged: () => {
            indicator?.showLanguage();
            viewer?.showLayout();
        },
    });
    const labels = new LayoutLabels();

    service = starting;
    keyboardStyle = new KeyboardStyle(extensionDirectory);
    viewer = new KeyboardViewer(starting, labels);
    indicator = new KeywayIndicator(starting, labels, viewer);
    appletReplacement = new KeyboardAppletReplacement(indicator);

    // Owning the name is what tells the agent to start asking, so the layouts come first.
    starting.whenReady(() => {
        if (service !== starting)
            return;

        indicator.showLayouts();
        nameId = Gio.bus_own_name(Gio.BusType.SESSION, BUS_NAME,
            Gio.BusNameOwnerFlags.NONE, null, null, null);
    });
}

function disable() {
    if (nameId) {
        Gio.bus_unown_name(nameId);
        nameId = 0;
    }

    appletReplacement.destroy(service.ListSources().length > 1);
    appletReplacement = null;
    indicator.destroy();
    indicator = null;
    viewer.destroy();
    viewer = null;
    keyboardStyle.destroy();
    keyboardStyle = null;
    service.destroy();
    service = null;
}
