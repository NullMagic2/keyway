// Module: keyway.extension.gnome.adapter
// Purpose: Give the Keyway agent the input source and text APIs only the Shell has, and
// replace the Shell's input source indicator and switcher popup with Keyway's own behavior.
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
const DEFAULT_LABEL_LANGUAGE = 'en';
const INDICATOR_ROLE = 'keyway';
const INDICATOR_NAME = 'Keyway';
const MENU_ALIGNMENT = 0.5;
const PANEL_BOX = 'right';
const LABELS_FILE = 'keyway/layout_labels.json';
const FLAGS_DIRECTORY = 'keyway/flags';
const FLAG_EXTENSION = '.svg';

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
    <signal name="ActiveSourceChanged">
      <arg type="s" name="source_id"/>
    </signal>
    <signal name="SourcesChanged"/>
  </interface>
</node>`;

class KeywayService {
    // The listener's sourcesChanged and activeChanged are called as the Shell reports each, and
    // flagsChanged when the agent passes on the user's choice.
    constructor(listener) {
        this._listener = listener;
        this.flagsShown = true;
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

// Returns the menu label in the first of the desktop's languages that has one.
function settingsLabel() {
    const languages = GLib.get_language_names().map(name => name.split(/[_.@]/)[0]);
    const language = languages.find(name => name in SETTINGS_LABELS);

    return SETTINGS_LABELS[language ?? DEFAULT_LABEL_LANGUAGE];
}

// The layout switcher in the taskbar: the active layout's flag and label, such as PT-BR, and a
// menu of the layouts by their full names.
const KeywayIndicator = GObject.registerClass({GTypeName: INDICATOR_TYPE_NAME},
class KeywayIndicator extends PanelMenu.Button {
    _init(service, labels) {
        super._init(MENU_ALIGNMENT, INDICATOR_NAME);

        this._service = service;
        this._labels = labels;
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
        this.menu.addAction(settingsLabel(),
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
            sourcesChanged: () => this._indicator?.showLayouts(),
            activeChanged: () => this._indicator?.showActive(),
            flagsChanged: () => this._indicator?.showLayouts(),
        });

        // Keyway's indicator takes the place of the Shell's, which session modes without a panel
        // do not have.
        const shellIndicator = Main.panel.statusArea.keyboard?.container;
        const position = shellIndicator?.get_parent()?.get_children().indexOf(shellIndicator) ?? 0;

        this._indicator = new KeywayIndicator(this._service, new LayoutLabels());
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
        this._service.destroy();
        this._service = null;
    }
}
