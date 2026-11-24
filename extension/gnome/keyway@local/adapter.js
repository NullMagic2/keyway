// Module: keyway.extension.gnome.adapter
// Purpose: Give the Keyway agent the input source and text APIs only the Shell has, replace the
// Shell's input source indicator and switcher popup with Keyway's own, and draw its keyboard.
// Created: 2026-11-08
// Architecture: A thin D-Bus adapter with no policy; crates/keyway-desktop is its client, and
// the Cinnamon extension offers the same interface. extension.js loads this file, and loads it
// again after an upgrade, each time from a copy with a name of its own.
// GSettings can edit the source list, but only the Shell can change the active source.
// The Shell's indicator comes back whenever this is disabled, as on the lock screen.

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {getInputSourceManager} from 'resource:///org/gnome/shell/ui/status/keyboard.js';

const BUS_NAME = 'org.keyway.Shell';
const OBJECT_PATH = '/org/keyway/Shell';
const INPUT_SOURCES_SCHEMA = 'org.gnome.desktop.input-sources';
const XKB_OPTIONS_KEY = 'xkb-options';
const MICROSECONDS_PER_MILLISECOND = 1000;
const SETTINGS_APP_ID = 'keyway-settings.desktop';
const SETTINGS_LABELS = {
    en: 'Keyway settings',
    pt: 'Configurações do Keyway',
    el: 'Ρυθμίσεις Keyway',
};
const KEYBOARD_LABELS = {
    en: 'On-screen keyboard',
    pt: 'Teclado virtual',
    el: 'Εικονικό πληκτρολόγιο',
};
const DEFAULT_LABEL_LANGUAGE = 'en';
const INDICATOR_ROLE = 'keyway';
const INDICATOR_NAME = 'Keyway';
const MENU_ALIGNMENT = 0.5;
const PANEL_BOX = 'right';
const LABELS_FILE = 'keyway/layout_labels.json';
const FLAGS_DIRECTORY = 'keyway/flags';
const FLAG_EXTENSION = '.svg';
const EXTENSION_UUID = 'keyway@local';
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

// Key roles, as keyway-core names them. A latching key stays down until the next plain key.
const GAP_ROLE = 'gap';
const PLAIN_ROLE = 'plain';
const SHIFT_ROLE = 'shift';
const LEVEL3_ROLE = 'level3';
const CAPS_LOCK_ROLE = 'caps_lock';
const LATCHING_ROLES = ['modifier', SHIFT_ROLE, LEVEL3_ROLE];

// A character key has a label for each combination of these, at the sum of the active ones.
const SHIFT_LABELS = 1;
const LEVEL3_LABELS = 2;
const CAPS_LOCK_LABELS = 4;

// A Shell that can still run on X11 says which it runs on; later ones are always on Wayland.
const ON_WAYLAND = Meta.is_wayland_compositor?.() ?? true;

// GNOME 48 named the direction of a box; before it a box was either vertical or not.
const VERTICAL_BOX = 'orientation' in St.BoxLayout.prototype
    ? {orientation: Clutter.Orientation.VERTICAL}
    : {vertical: true};

// The Shell accepts a type name once per login, so each loaded copy of this file brings its own.
const COPY_NAME = GLib.path_get_basename(import.meta.url).replace(/\W/g, '_');
const INDICATOR_TYPE_NAME = `KeywayIndicator_${COPY_NAME}`;

// The Shell's shortcuts for the next and previous source, such as Super+Space.
const SWITCH_SHORTCUTS = [
    {name: 'switch-input-source', step: 1},
    {name: 'switch-input-source-backward', step: -1},
];

// GNOME 46 has no NO_DOT; there, NONE leaves the same space beside the label.
const INACTIVE_ORNAMENT = PopupMenu.Ornament.NO_DOT ?? PopupMenu.Ornament.NONE;

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
    <signal name="ActiveSourceChanged">
      <arg type="s" name="source_id"/>
    </signal>
    <signal name="SourcesChanged"/>
  </interface>
</node>`;

class KeywayService {
    // The listener's sourcesChanged and activeChanged are called as the Shell reports each,
    // flagsChanged when the agent passes on the user's choice, keyboardChanged when it describes
    // the active layout's keys, and keyboardThemeChanged when it names their colours.
    constructor(listener) {
        this._listener = listener;
        this.flagsShown = true;
        this.keyboardRows = [];
        [this.keyboardTheme] = KEYBOARD_THEMES;
        this._manager = getInputSourceManager();
        this._settings = new Gio.Settings({schema_id: INPUT_SOURCES_SCHEMA});

        const seat = Clutter.get_default_backend().get_default_seat();
        this._virtualKeyboard = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);

        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE_XML, this);
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);

        this._activeChangedId = this._manager.connect('current-source-changed', () => {
            listener.activeChanged();
            this._dbus.emit_signal('ActiveSourceChanged',
                new GLib.Variant('(s)', [this.ActiveSource()]));
        });
        this._sourcesChangedId = this._manager.connect('sources-changed', () => {
            listener.sourcesChanged();
            this._dbus.emit_signal('SourcesChanged', null);
        });
    }

    destroy() {
        this._manager.disconnect(this._activeChangedId);
        this._manager.disconnect(this._sourcesChangedId);
        this._dbus.unexport();
        this._virtualKeyboard.run_dispose();
    }

    // Returns id, XKB id, display name and short name, in the desktop's switching order.
    ListSources() {
        const sources = Object.values(this._manager.inputSources);

        sources.sort((first, second) => first.index - second.index);

        return sources.map(source => [source.id, source.xkbId, source.displayName, source.shortName]);
    }

    ActiveSource() {
        return this._manager.currentSource?.id ?? '';
    }

    XkbOptions() {
        return this._settings.get_strv(XKB_OPTIONS_KEY);
    }

    Activate(sourceId) {
        const sources = Object.values(this._manager.inputSources);
        const source = sources.find(candidate => candidate.id === sourceId);

        if (!source)
            throw new Error(`Unknown input source: ${sourceId}`);

        source.activate(true);
    }

    // Moves through the desktop's switching order, wrapping around.
    switchLayout(step) {
        const sources = this.ListSources();
        const activeId = this.ActiveSource();

        if (sources.length < 2)
            return;

        const active = sources.findIndex(([id]) => id === activeId);
        const [targetId] = sources[(active + step + sources.length) % sources.length];

        this.Activate(targetId);
    }

    // Follows the Shell's on-screen keyboard: text goes through the input method when the
    // focused client accepts it, and as key events for any other client.
    CommitText(text) {
        if (Main.inputMethod.currentFocus) {
            Main.inputMethod.commit(text);
            return;
        }

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

    // Presses or releases a key by its evdev code, as a keyboard would; the active layout
    // decides what it types.
    notifyKey(keyCode, pressed) {
        const time = Clutter.get_current_event_time() * MICROSECONDS_PER_MILLISECOND;
        const state = pressed ? Clutter.KeyState.PRESSED : Clutter.KeyState.RELEASED;

        this._virtualKeyboard.notify_key(time, keyCode, state);
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

// Returns a menu label in the first of the desktop's languages that has one.
function menuLabel(labels) {
    const languages = GLib.get_language_names().map(name => name.split(/[_.@]/)[0]);
    const language = languages.find(name => name in labels);

    return labels[language ?? DEFAULT_LABEL_LANGUAGE];
}

// The keyboard's look. The Shell reads an extension's own stylesheet only at login, so this one
// has another name and is loaded here, with each adapter.
class KeyboardStyle {
    constructor() {
        const directory = Main.extensionManager.lookup(EXTENSION_UUID)?.dir;

        this._file = directory?.get_child(KEYBOARD_STYLESHEET) ?? null;

        try {
            this._theme().load_stylesheet(this._file);
        } catch (error) {
            logError(error, EXTENSION_UUID);
            this._file = null;
        }
    }

    // The Shell makes a new theme when its own style changes, and carries this stylesheet over.
    _theme() {
        return St.ThemeContext.get_for_stage(global.stage).get_theme();
    }

    destroy() {
        if (this._file)
            this._theme().unload_stylesheet(this._file);
    }
}

// The on-screen keyboard: the active layout's keys as the agent lays them out, in a panel that
// can be moved by its title. The Shell's own keyboard device presses a clicked key, and a click
// on something the Shell draws takes no focus, so the window being typed in receives the key.
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
    }

    destroy() {
        this.hide();
        this._keymap.disconnect(this._keymapId);
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

        this._flag = new St.Icon({style_class: 'keyway-keyboard-flag'});
        this._name = new St.Label({style_class: 'keyway-keyboard-name', x_expand: true,
            y_align: Clutter.ActorAlign.CENTER});

        // The handle moves the keyboard. The close button is beside it, not in it, because a
        // press on a button also reaches whatever the button is in.
        const handle = new St.BoxLayout({style_class: 'keyway-keyboard-handle', reactive: true,
            x_expand: true});
        const close = new St.Button({style_class: 'keyway-keyboard-close',
            child: new St.Icon({style_class: 'keyway-keyboard-close-icon', icon_name: CLOSE_ICON})});
        const title = new St.BoxLayout({style_class: 'keyway-keyboard-title'});

        handle.add_child(this._flag);
        handle.add_child(this._name);
        title.add_child(handle);
        title.add_child(close);
        close.connect('clicked', () => this.hide());
        handle.connect('button-press-event', (_actor, event) => this._startDrag(handle, event));
        handle.connect('motion-event', (_actor, event) => this._dragTo(event));
        handle.connect('button-release-event', () => this._endDrag());
        handle.connect('touch-event', (_actor, event) => this._touchHandle(handle, event));

        this._keyArea = new St.Widget();
        this._panel = new St.BoxLayout({style_class: 'keyway-keyboard', reactive: true,
            ...VERTICAL_BOX});
        this._panel.add_child(title);
        this._panel.add_child(this._keyArea);

        // Above windows, full screen ones included, and part of what takes clicks.
        Main.layoutManager.addTopChrome(this._panel);

        this.showTheme();
        this.showLayout();
        this.showKeys();
    }

    hide() {
        if (!this._panel)
            return;

        this._releaseAll();
        this._endDrag();
        Main.layoutManager.removeChrome(this._panel);
        this._panel.destroy();
        this._panel = null;
        this._keys = [];
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

        this._name.text = active?.[2] ?? '';
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

        const scale = St.ThemeContext.get_for_stage(global.stage).scale_factor;
        const rowHeight = KEY_SIZE * scale;
        const quarter = rowHeight / QUARTERS_PER_KEY;
        const spacing = KEY_SPACING * scale;
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
        key.label = new St.Label({x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER});
        key.actor = new St.Bin({style_class: 'keyway-key', reactive: true, track_hover: true,
            child: key.label});

        // A key with one label types nothing that the layout decides.
        if (key.labels.length === 1)
            key.actor.add_style_class_name('keyway-key-named');

        key.actor.set_position(x, y);
        key.actor.set_size(width, height);
        key.actor.connect('button-press-event', (_actor, event) => this._press(key, event));
        key.actor.connect('button-release-event', () => this._release(key));
        key.actor.connect('leave-event', () => this._release(key));
        key.actor.connect('touch-event', (_actor, event) => this._touchKey(key, event));

        this._keyArea.add_child(key.actor);
        this._keys.push(key);
    }

    _isLatched(role) {
        return this._keys.some(key => key.role === role && this._latched.has(key.code));
    }

    // Shows each key's label for the modifiers held here and the desktop's Caps Lock.
    _showLabels() {
        if (!this._panel)
            return;

        const capsLock = this._keymap.get_caps_lock_state();
        const state = (this._isLatched(SHIFT_ROLE) ? SHIFT_LABELS : 0) +
            (this._isLatched(LEVEL3_ROLE) ? LEVEL3_LABELS : 0) +
            (capsLock ? CAPS_LOCK_LABELS : 0);

        for (const key of this._keys) {
            key.label.text = key.labels[key.labels.length === 1 ? 0 : state] ?? '';

            if (key.role === CAPS_LOCK_ROLE)
                this._mark(key, 'checked', capsLock);
        }
    }

    _mark(key, pseudoClass, marked) {
        if (marked)
            key.actor.add_style_pseudo_class(pseudoClass);
        else
            key.actor.remove_style_pseudo_class(pseudoClass);
    }

    _press(key, event) {
        if (event.get_button() !== Clutter.BUTTON_PRIMARY)
            return Clutter.EVENT_PROPAGATE;

        return this._pressKey(key);
    }

    // A finger does what the mouse button does. On X11 a touch also arrives as pointer events,
    // which the handlers above take, so only Wayland's touches are acted on here.
    _touchKey(key, event) {
        if (!ON_WAYLAND)
            return Clutter.EVENT_PROPAGATE;

        switch (event.type()) {
        case Clutter.EventType.TOUCH_BEGIN:
            return this._pressKey(key);
        case Clutter.EventType.TOUCH_END:
        case Clutter.EventType.TOUCH_CANCEL:
            return this._release(key);
        default:
            return Clutter.EVENT_PROPAGATE;
        }
    }

    // A second finger on a key already down changes nothing.
    _pressKey(key) {
        if (this._held.has(key.code))
            return Clutter.EVENT_STOP;

        if (LATCHING_ROLES.includes(key.role)) {
            this._setLatched(key, !this._latched.has(key.code));
            this._showLabels();

            return Clutter.EVENT_STOP;
        }

        this._held.add(key.code);
        this._mark(key, 'active', true);
        this._service.notifyKey(key.code, true);

        return Clutter.EVENT_STOP;
    }

    // Also when the pointer leaves a key, so that no key stays down behind it.
    _release(key) {
        if (!this._held.delete(key.code))
            return Clutter.EVENT_PROPAGATE;

        this._mark(key, 'active', false);
        this._service.notifyKey(key.code, false);

        // The modifiers held for this key have done their work.
        if (key.role === PLAIN_ROLE)
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

    _startDrag(handle, event) {
        if (event.get_button() !== Clutter.BUTTON_PRIMARY)
            return Clutter.EVENT_PROPAGATE;

        return this._takeHold(handle, event);
    }

    // A finger moves the keyboard as the mouse does, under the same rule as for a key.
    _touchHandle(handle, event) {
        if (!ON_WAYLAND)
            return Clutter.EVENT_PROPAGATE;

        switch (event.type()) {
        case Clutter.EventType.TOUCH_BEGIN:
            return this._takeHold(handle, event);
        case Clutter.EventType.TOUCH_UPDATE:
            return this._dragTo(event);
        case Clutter.EventType.TOUCH_END:
        case Clutter.EventType.TOUCH_CANCEL:
            return this._endDrag();
        default:
            return Clutter.EVENT_PROPAGATE;
        }
    }

    // Everything the pointer or finger does goes to the handle until it lets go.
    _takeHold(handle, event) {
        if (this._drag)
            return Clutter.EVENT_STOP;

        const [pointerX, pointerY] = event.get_coords();

        this._drag = {
            offsetX: pointerX - this._panel.x,
            offsetY: pointerY - this._panel.y,
            grab: global.stage.grab(handle),
        };

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
        this._drag?.grab.dismiss();
        this._drag = null;

        return Clutter.EVENT_PROPAGATE;
    }

    // Where the user left it, or else centred at the bottom of the primary monitor's free area.
    // Always wholly on the screen, so the title stays within reach.
    _place() {
        const [, , width, height] = this._panel.get_preferred_size();
        const workspace = global.workspace_manager.get_active_workspace();
        const area = workspace.get_work_area_for_monitor(global.display.get_primary_monitor());
        const scale = St.ThemeContext.get_for_stage(global.stage).scale_factor;
        const [x, y] = this._position ?? [
            area.x + (area.width - width) / 2,
            area.y + area.height - height - KEYBOARD_BOTTOM_MARGIN * scale,
        ];
        const within = (value, limit) => Math.round(Math.max(0, Math.min(value, limit)));

        this._panel.set_position(within(x, global.stage.width - width),
            within(y, global.stage.height - height));
    }
}

// The layout switcher in the taskbar: the active layout's flag and label, such as PT-BR, and a
// menu of the layouts by their full names, the on-screen keyboard and the settings window.
const KeywayIndicator = GObject.registerClass({GTypeName: INDICATOR_TYPE_NAME},
class KeywayIndicator extends PanelMenu.Button {
    _init(service, labels, viewer) {
        super._init(MENU_ALIGNMENT, INDICATOR_NAME);

        this._service = service;
        this._labels = labels;
        this._viewer = viewer;
        this._items = new Map();

        const box = new St.BoxLayout({style_class: 'panel-status-indicators-box'});
        this._flag = new St.Icon({style_class: 'system-status-icon'});
        this._label = new St.Label({y_align: Clutter.ActorAlign.CENTER});
        box.add_child(this._flag);
        box.add_child(this._label);
        this.add_child(box);

        this._layoutSection = new PopupMenu.PopupMenuSection();
        this.menu.addMenuItem(this._layoutSection);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this.menu.addAction(menuLabel(KEYBOARD_LABELS), () => viewer.toggle());
        this.menu.addAction(menuLabel(SETTINGS_LABELS),
            () => Shell.AppSystem.get_default().lookup_app(SETTINGS_APP_ID)?.activate());

        this.showLayouts();
    }

    showLayouts() {
        this._layoutSection.removeAll();
        this._items.clear();

        const flagsShown = this._service.flagsShown;

        for (const [id, , displayName, shortName] of this._service.ListSources()) {
            const text = this._labels.text(id, shortName);
            const flag = flagsShown ? this._labels.flag(id) : null;

            // With flags on, a layout without one keeps an empty space, so the names line up.
            const item = flagsShown
                ? new PopupMenu.PopupImageMenuItem(displayName, flag)
                : new PopupMenu.PopupMenuItem(displayName);

            // The name takes the spare width, so the selection dot stays at the edge on Shells
            // that put it last.
            item.label.x_expand = true;
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
            item.setOrnament(id === activeId ? PopupMenu.Ornament.DOT : INACTIVE_ORNAMENT);
    }
});

// Keeps an actor hidden for as long as this lives. Hiding it once is not enough for the Shell's
// indicator, which shows itself again whenever the sources change.
class KeptHidden {
    constructor(actor) {
        this._actor = actor;
        this._visibleId = actor.connect('notify::visible', () => {
            if (actor.visible)
                actor.hide();
        });

        actor.hide();
    }

    release(visible) {
        this._actor.disconnect(this._visibleId);
        this._actor.visible = visible;
    }
}

// Makes the Shell's shortcuts switch at once, without its popup. The user's choice of keys stays
// in the Shell's settings.
function takeOverSwitchShortcuts(service) {
    for (const {name, step} of SWITCH_SHORTCUTS) {
        Main.wm.setCustomKeybindingHandler(name, Shell.ActionMode.ALL,
            () => service.switchLayout(step));
    }
}

// Mutter cannot hand the Shell's own handler back once it is replaced, so this calls the same
// method, and the shortcuts work as they did before Keyway, popup included.
function returnSwitchShortcuts() {
    const manager = getInputSourceManager();

    for (const {name} of SWITCH_SHORTCUTS) {
        Main.wm.setCustomKeybindingHandler(name, Shell.ActionMode.ALL,
            (...binding) => manager._switchInputSource(...binding));
    }
}

// Everything Keyway does in the Shell, from construction until destroy.
export default class Adapter {
    constructor() {
        this._service = new KeywayService({
            sourcesChanged: () => {
                this._indicator?.showLayouts();
                this._viewer?.showLayout();
            },
            activeChanged: () => {
                this._indicator?.showActive();
                this._viewer?.showLayout();
            },
            flagsChanged: () => {
                this._indicator?.showLayouts();
                this._viewer?.showLayout();
            },
            keyboardChanged: () => this._viewer?.showKeys(),
            keyboardThemeChanged: () => this._viewer?.showTheme(),
        });

        const labels = new LayoutLabels();

        this._keyboardStyle = new KeyboardStyle();
        this._viewer = new KeyboardViewer(this._service, labels);

        // Keyway's indicator takes the place of the Shell's, which session modes without a panel
        // do not have.
        const shellIndicator = Main.panel.statusArea.keyboard?.container;
        const position = shellIndicator?.get_parent()?.get_children().indexOf(shellIndicator) ?? 0;

        this._indicator = new KeywayIndicator(this._service, labels, this._viewer);
        Main.panel.addToStatusArea(INDICATOR_ROLE, this._indicator, Math.max(position, 0), PANEL_BOX);

        this._shellIndicator = shellIndicator ? new KeptHidden(shellIndicator) : null;

        takeOverSwitchShortcuts(this._service);

        this._nameId = Gio.bus_own_name(Gio.BusType.SESSION, BUS_NAME,
            Gio.BusNameOwnerFlags.NONE, null, null, null);
    }

    destroy() {
        Gio.bus_unown_name(this._nameId);
        returnSwitchShortcuts();

        // The Shell shows its indicator only when there is something to switch between.
        this._shellIndicator?.release(this._service.ListSources().length > 1);
        this._shellIndicator = null;

        this._indicator.destroy();
        this._indicator = null;
        this._viewer.destroy();
        this._viewer = null;
        this._keyboardStyle.destroy();
        this._service.destroy();
        this._service = null;
    }
}
