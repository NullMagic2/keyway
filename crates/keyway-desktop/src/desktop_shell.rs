//! Module: keyway_desktop::desktop_shell
//! Purpose: Client for the D-Bus interface that Keyway's desktop extensions share.
//! Created: 2026-11-08
//! Architecture: The desktop owns its source list; this only reads it and asks for changes.
//! The GNOME and Cinnamon extensions under extension/ are the other end of this interface.

use crate::xkb::right_alt_is_altgr;
use keyway_core::layout_id::LayoutId;
use std::collections::HashMap;
use std::fmt;
use std::thread;
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::message::Type;
use zbus::zvariant::OwnedValue;
use zbus::{MatchRule, Message};

/// The name both desktops know Keyway's extension by.
pub const EXTENSION_UUID: &str = "keyway@local";
const BUS_NAME: &str = "org.keyway.Shell";
const OBJECT_PATH: &str = "/org/keyway/Shell";
const INTERFACE: &str = "org.keyway.Shell";
const DBUS_NAME: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const NAME_OWNER_CHANGED: &str = "NameOwnerChanged";
const NAME_ARGUMENT: u8 = 0;

/// The GNOME extension's loader, which is on the Shell's own bus name so that it can be reached
/// while no adapter is running.
const GNOME_SHELL_NAME: &str = "org.gnome.Shell";
const LOADER_PATH: &str = "/org/keyway/Loader";
const LOADER_INTERFACE: &str = "org.keyway.Loader";

/// The Shell's answers while no extension exports the loader.
const NO_LOADER_ERRORS: [&str; 2] = [
    "org.freedesktop.DBus.Error.UnknownMethod",
    "org.freedesktop.DBus.Error.UnknownObject",
];
const GNOME_SHELL_PATH: &str = "/org/gnome/Shell";
const GNOME_EXTENSIONS_INTERFACE: &str = "org.gnome.Shell.Extensions";
const GNOME_STATE_FIELD: &str = "state";

/// The Shell's states for an extension that is turned off: INACTIVE, and INITIALIZED for one
/// not turned on since the login.
const GNOME_TURNED_OFF_STATES: [f64; 2] = [2.0, 6.0];

/// Cinnamon reloads any of its extensions itself.
const CINNAMON_NAME: &str = "org.Cinnamon";
const CINNAMON_PATH: &str = "/org/Cinnamon";
const CINNAMON_EXTENSION_TYPE: &str = "EXTENSION";
const EXTENSION_NOT_RUNNING: &str = "the extension is not running";

/// Why the desktop could not be reached; usually the extension is not running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopError(String);

impl fmt::Display for DesktopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DesktopError {}

impl From<zbus::Error> for DesktopError {
    fn from(error: zbus::Error) -> Self {
        Self(error.to_string())
    }
}

/// Why the desktop did not load the extension's files again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReloadError {
    /// GNOME Shell has the extension on, or does not know it yet, and runs no loader: the one
    /// it read at login is older than the loader, failed, or was installed later.
    NoLoader,
    Failed(DesktopError),
}

impl fmt::Display for ReloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoLoader => {
                formatter.write_str("GNOME Shell has not loaded this version of the extension")
            }
            Self::Failed(error) => error.fmt(formatter),
        }
    }
}

impl<E: Into<DesktopError>> From<E> for ReloadError {
    fn from(error: E) -> Self {
        Self::Failed(error.into())
    }
}

/// What a watch reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopEvent {
    Changed,
    /// The extension's bus name lost its owner. An extension that answers afterwards may be a
    /// new one, which knows nothing the earlier one was told.
    ExtensionStopped,
}

/// One input source. The id names it to the desktop; the XKB id is its underlying layout,
/// which differs from the id for input method engines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputSource {
    pub id: LayoutId,
    pub xkb_id: LayoutId,
    pub display_name: String,
    pub short_name: String,
}

/// The desktop's input state at one moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopSnapshot {
    pub sources: Vec<InputSource>,
    pub active: LayoutId,
    pub right_alt_is_altgr: bool,
}

#[derive(Clone, Debug)]
pub struct DesktopShell {
    connection: Connection,
    proxy: Proxy<'static>,
}

impl DesktopShell {
    /// Connects to the session bus. The extension itself may not be running yet.
    pub fn connect() -> Result<Self, DesktopError> {
        let connection = Connection::session()?;
        let proxy = Proxy::new(&connection, BUS_NAME, OBJECT_PATH, INTERFACE)?;

        Ok(Self { connection, proxy })
    }

    /// Reads the current state. Err means the extension is not available.
    pub fn snapshot(&self) -> Result<DesktopSnapshot, DesktopError> {
        let sources = self.list_sources()?;
        let active = self.active_source()?;
        let xkb_options: Vec<String> = self.proxy.call("XkbOptions", &())?;

        let active_xkb_id = sources
            .iter()
            .find(|source| source.id == active)
            .map_or(&active, |source| &source.xkb_id);
        let right_alt_is_altgr = right_alt_is_altgr(active_xkb_id, &xkb_options);

        Ok(DesktopSnapshot {
            sources,
            active,
            right_alt_is_altgr,
        })
    }

    fn list_sources(&self) -> Result<Vec<InputSource>, DesktopError> {
        let raw_sources: Vec<(String, String, String, String)> =
            self.proxy.call("ListSources", &())?;

        raw_sources
            .into_iter()
            .map(|(id, xkb_id, display_name, short_name)| {
                Ok(InputSource {
                    id: LayoutId::try_from(id).map_err(DesktopError)?,
                    xkb_id: LayoutId::try_from(xkb_id).map_err(DesktopError)?,
                    display_name,
                    short_name,
                })
            })
            .collect()
    }

    fn active_source(&self) -> Result<LayoutId, DesktopError> {
        let id: String = self.proxy.call("ActiveSource", &())?;

        LayoutId::try_from(id).map_err(DesktopError)
    }

    pub fn activate(&self, source: &LayoutId) -> Result<(), DesktopError> {
        Ok(self.proxy.call("Activate", &(source.as_str(),))?)
    }

    pub fn commit_text(&self, text: &str) -> Result<(), DesktopError> {
        Ok(self.proxy.call("CommitText", &(text,))?)
    }

    /// Tells the extension whether its indicator shows country flags.
    pub fn show_flags(&self, shown: bool) -> Result<(), DesktopError> {
        Ok(self.proxy.call("ShowFlags", &(shown,))?)
    }

    fn name_has_owner(&self, name: &str) -> Result<bool, DesktopError> {
        let reply = self.connection.call_method(
            Some(DBUS_NAME),
            DBUS_PATH,
            Some(DBUS_NAME),
            "NameHasOwner",
            &(name,),
        )?;

        Ok(reply.body().deserialize()?)
    }

    /// Whether GNOME Shell holds the extension turned off. False too for one it does not know.
    fn gnome_extension_is_off(&self) -> Result<bool, DesktopError> {
        let reply = self.connection.call_method(
            Some(GNOME_SHELL_NAME),
            GNOME_SHELL_PATH,
            Some(GNOME_EXTENSIONS_INTERFACE),
            "GetExtensionInfo",
            &(EXTENSION_UUID,),
        )?;
        let info: HashMap<String, OwnedValue> = reply.body().deserialize()?;
        let state = info
            .get(GNOME_STATE_FIELD)
            .and_then(|state| f64::try_from(state).ok());

        Ok(state.is_some_and(|state| GNOME_TURNED_OFF_STATES.contains(&state)))
    }

    fn reload_gnome_extension(&self) -> Result<(), ReloadError> {
        let reply = self.connection.call_method(
            Some(GNOME_SHELL_NAME),
            LOADER_PATH,
            Some(LOADER_INTERFACE),
            "Reload",
            &(),
        );

        match reply {
            Ok(_) => Ok(()),
            Err(zbus::Error::MethodError(name, ..))
                if NO_LOADER_ERRORS.contains(&name.as_str()) =>
            {
                if self.gnome_extension_is_off()? {
                    return Err(DesktopError(EXTENSION_NOT_RUNNING.into()).into());
                }

                Err(ReloadError::NoLoader)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Makes the running desktop load the extension's files again, as after an upgrade. Err
    /// means it did not, such as when the extension it loaded at login is too old to be asked.
    pub fn reload_extension(&self) -> Result<(), ReloadError> {
        if !self.name_has_owner(CINNAMON_NAME)? {
            return self.reload_gnome_extension();
        }

        // Cinnamon would load an extension that is not running, even one the user turned off.
        if !self.name_has_owner(BUS_NAME)? {
            return Err(DesktopError(EXTENSION_NOT_RUNNING.into()).into());
        }

        self.connection.call_method(
            Some(CINNAMON_NAME),
            CINNAMON_PATH,
            Some(CINNAMON_NAME),
            "ReloadXlet",
            &(EXTENSION_UUID, CINNAMON_EXTENSION_TYPE),
        )?;

        Ok(())
    }

    /// Calls notify whenever the desktop's input state may have changed, or the extension
    /// appeared or went away. It is only a hint to read a new snapshot, so its source does
    /// not need to be trusted. Calls on_session_end if the bus connection ends, which means
    /// the session is over. Watching is in place when this returns.
    pub fn watch(
        &self,
        notify: impl Fn(DesktopEvent) + Clone + Send + 'static,
        on_session_end: impl Fn() + Clone + Send + 'static,
    ) -> Result<(), DesktopError> {
        let extension_signals = MatchRule::builder()
            .msg_type(Type::Signal)
            .interface(INTERFACE)?
            .build();
        let owner_changes = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(DBUS_NAME)?
            .member(NAME_OWNER_CHANGED)?
            .arg(NAME_ARGUMENT, BUS_NAME)?
            .build();

        for (rule, reports_owner) in [(extension_signals, false), (owner_changes, true)] {
            let messages = MessageIterator::for_match_rule(rule, &self.connection, None)?;
            let notify = notify.clone();
            let on_session_end = on_session_end.clone();

            thread::spawn(move || {
                for message in messages {
                    let stopped = reports_owner && message.is_ok_and(|change| owner_left(&change));

                    notify(if stopped {
                        DesktopEvent::ExtensionStopped
                    } else {
                        DesktopEvent::Changed
                    });
                }

                on_session_end();
            });
        }

        Ok(())
    }
}

/// Whether a name owner change left the name without an owner.
fn owner_left(change: &Message) -> bool {
    change
        .body()
        .deserialize::<(&str, &str, &str)>()
        .is_ok_and(|(_name, _old_owner, new_owner)| new_owner.is_empty())
}
