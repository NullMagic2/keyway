// Module: keyway.extension.gnome
// Purpose: Load Keyway's adapter into the Shell, and load it again when keyway restart asks, so
// that an upgrade takes effect without a new login.
// Created: 2026-11-09
// Architecture: The Shell reads an extension's files once per login and keeps every module it
// imported under that file's address. So all of Keyway's behavior is in adapter.js, imported
// from a copy named after its contents, which makes changed contents a module the Shell has
// not seen. A change to this file still waits for the next login, so it does nothing else.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const LOADER_PATH = '/org/keyway/Loader';
const LOADER_ERROR = 'org.keyway.Loader.Failed';
const ADAPTER_FILE = 'adapter.js';
const COPY_DIRECTORY = 'keyway';
const COPY_DIRECTORY_MODE = 0o700;

// Reached through the Shell's own bus name, which outlives every adapter.
const LOADER_XML = `
<node>
  <interface name="org.keyway.Loader">
    <method name="Reload"/>
  </interface>
</node>`;

export default class KeywayExtension extends Extension {
    enable() {
        this._dbus = Gio.DBusExportedObject.wrapJSObject(LOADER_XML, this);
        this._dbus.export(Gio.DBus.session, LOADER_PATH);

        this._load().catch(error => logError(error, this.uuid));
    }

    disable() {
        this._dbus.unexport();
        this._dbus = null;

        this._adapter?.destroy();
        this._adapter = null;
    }

    // Answers once the adapter now on disk is the one running, or with why it is not.
    ReloadAsync(_parameters, invocation) {
        this._load().then(
            () => invocation.return_value(null),
            error => invocation.return_dbus_error(LOADER_ERROR, error.message));
    }

    // A failed import leaves the running adapter in place.
    async _load() {
        const {default: Adapter} = await import(this._copyAdapter().get_uri());

        // Disabled while the import was under way.
        if (!this._dbus)
            return;

        this._adapter?.destroy();
        this._adapter = null;
        this._adapter = new Adapter();
    }

    // Returns a copy of the adapter in the user's runtime directory, named after its checksum.
    _copyAdapter() {
        const [, contents] = this.dir.get_child(ADAPTER_FILE).load_contents(null);
        const checksum = GLib.compute_checksum_for_data(GLib.ChecksumType.SHA256, contents);
        const directory = GLib.build_filenamev([GLib.get_user_runtime_dir(), COPY_DIRECTORY]);
        const copy = Gio.File.new_for_path(
            GLib.build_filenamev([directory, `adapter-${checksum}.js`]));

        GLib.mkdir_with_parents(directory, COPY_DIRECTORY_MODE);
        copy.replace_contents(contents, null, false, Gio.FileCreateFlags.NONE, null);

        return copy;
    }
}
