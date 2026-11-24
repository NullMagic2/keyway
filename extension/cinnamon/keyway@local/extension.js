// Module: keyway.extension.cinnamon
// Purpose: Give the Keyway agent the input source and text APIs only Cinnamon has, and replace
// the keyboard applet with Keyway's own indicator.
// Created: 2026-11-08
// Architecture: A thin D-Bus adapter with no policy; crates/keyway-desktop is its client, and
// the GNOME extension offers the same interface. Cinnamon changed its layout system in 6.6, so
// two small classes cover the releases on either side of that change. The applet comes back
// whenever this is disabled.

const Clutter = imports.gi.Clutter;
const Gio = imports.gi.Gio;
const GLib = imports.gi.GLib;
const St = imports.gi.St;
const XApp = imports.gi.XApp;
const CinnamonApps = imports.gi.Cinnamon;
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
const SETTINGS_LABELS = {
    en: 'Keyway settings',
    pt: 'Configurações do Keyway',
    el: 'Ρυθμίσεις Keyway',
};
const DEFAULT_LABEL_LANGUAGE = 'en';
const INDICATOR_NAME = 'keyway-indicator';
const LEFT_MOUSE_BUTTON = 1;
const ENABLED_APPLETS_CHANGED = 'changed::enabled-applets';
const LABELS_FILE = 'keyway/layout_labels.json';
const FLAGS_DIRECTORY = 'keyway/flags';
const FLAG_EXTENSION = '.svg';

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

class KeywayService {
    // The listener's sourcesChanged and activeChanged are called as Cinnamon reports each, and
    // flagsChanged when the agent passes on the user's choice.
    constructor(listener) {
        this._listener = listener;
        this.flagsShown = true;
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

// The layout switcher in the panel: the active layout's flag and label, such as PT-BR, and a
// menu of the layouts by their full names.
class KeywayIndicator {
    constructor(service, labels) {
        this._service = service;
        this._labels = labels;
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
        this.menu.addAction(settingsLabel(),
            () => CinnamonApps.AppSystem.get_default().lookup_app(SETTINGS_APP_ID)?.activate());

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

    showLayouts() {
        this._layoutSection.removeAll();
        this._items.clear();

        const flagsShown = this._service.flagsShown;

        for (const [id, , displayName, shortName] of this._service.ListSources()) {
            const text = this._labels.text(id, shortName);
            const flag = flagsShown ? this._labels.flag(id) : null;
            const item = new PopupMenu.PopupBaseMenuItem();

            // One column for flag and name, because the menu sizes each column to its widest
            // item, settings included. A layout without a flag keeps the space, so names line up.
            const name = new St.BoxLayout({style: FLAG_NAME_SPACING});

            if (flagsShown) {
                name.add_child(new St.Icon({style_class: 'popup-menu-icon', gicon: flag,
                    icon_size: Math.round(MENU_FLAG_SIZE * global.ui_scale)}));
            }

            name.add_child(new St.Label({text: displayName, y_align: Clutter.ActorAlign.CENTER}));
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

let service = null;
let indicator = null;
let appletReplacement = null;
let nameId = 0;

function init(_metadata) {
}

function enable() {
    const starting = new KeywayService({
        sourcesChanged: () => indicator?.showLayouts(),
        activeChanged: () => indicator?.showActive(),
        flagsChanged: () => indicator?.showLayouts(),
    });

    service = starting;
    indicator = new KeywayIndicator(starting, new LayoutLabels());
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
    service.destroy();
    service = null;
}
