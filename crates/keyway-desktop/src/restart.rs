//! Module: keyway_desktop::restart
//! Purpose: Restart every part of Keyway in the running session, as after an upgrade.
//! Created: 2026-11-09
//! Architecture: Shared by the keyway command and the settings window. Nothing here needs to
//! be root: systemctl itself asks for an administrator's password, in the desktop's dialog or
//! in the terminal.

use crate::command::run_command;
use crate::desktop_shell::{DesktopShell, EXTENSION_UUID, ReloadError};
use std::fmt::Display;
use std::io;

pub const AGENT_UNIT_NAME: &str = "keyway.service";
pub const ROUTER_UNIT_NAME: &str = "keyway-router.service";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Extension,
    Agent,
    Router,
}

/// How a restart went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestartReport {
    /// Each part, with why it was not restarted, or None for one that was.
    pub parts: [(Part, Option<String>); 3],

    /// GNOME Shell runs no loader to ask, so only the person can give it the extension, with
    /// looking_glass_line. The agent and the router need nothing more once that is done.
    pub needs_looking_glass: bool,
}

impl RestartReport {
    pub fn succeeded(&self) -> bool {
        self.parts.iter().all(|(_part, failure)| failure.is_none())
    }
}

fn failure(result: &Result<(), impl Display>) -> Option<String> {
    result.as_ref().err().map(ToString::to_string)
}

fn reload_extension() -> Result<(), ReloadError> {
    DesktopShell::connect()?.reload_extension()
}

/// Reads the agent's unit again first, as a package upgrade replaces it.
fn restart_agent() -> io::Result<()> {
    run_command("systemctl", &["--user", "daemon-reload"])?;

    run_command("systemctl", &["--user", "restart", AGENT_UNIT_NAME])
}

fn restart_router() -> io::Result<()> {
    run_command("systemctl", &["restart", ROUTER_UNIT_NAME])
}

/// Restarts every part in turn, so that one refusal, such as a password prompt the person
/// cancels, does not leave the others running their old version.
pub fn restart() -> RestartReport {
    let extension = reload_extension();

    RestartReport {
        parts: [
            (Part::Extension, failure(&extension)),
            (Part::Agent, failure(&restart_agent())),
            (Part::Router, failure(&restart_router())),
        ],
        needs_looking_glass: extension == Err(ReloadError::NoLoader),
    }
}

/// The line that gives a running GNOME Shell the installed extension, through an address the
/// Shell has not read it from. Looking Glass splits its input at every semicolon, so no string
/// in the line holds one. The 2 and 1 are the Shell's types for a user's and a system extension.
pub fn looking_glass_line() -> String {
    format!(
        "const u = '{EXTENSION_UUID}'; const m = Main.extensionManager; const e = m.lookup(u); \
         const d = [GLib.get_user_data_dir(), ...GLib.get_system_data_dirs()]\
         .map(p => Gio.File.new_for_path('/proc/self/root' + p + '/gnome-shell/extensions/' + u)); \
         const i = d.findIndex(f => f.query_exists(null)); \
         Promise.resolve(e && m.unloadExtension(e))\
         .then(() => m.loadExtension(m.createExtensionObject(u, d[i], i ? 1 : 2)))"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_succeeds_only_when_every_part_restarted() {
        let restarted = [
            (Part::Extension, None),
            (Part::Agent, None),
            (Part::Router, None),
        ];
        let mut refused = restarted.clone();
        refused[2].1 = Some("access denied".to_string());

        let report = |parts| RestartReport {
            parts,
            needs_looking_glass: false,
        };

        assert!(report(restarted).succeeded());
        assert!(!report(refused).succeeded());
    }

    #[test]
    fn the_looking_glass_line_survives_being_split_at_semicolons() {
        let line = looking_glass_line();
        let in_a_string = line
            .split('\'')
            .skip(1)
            .step_by(2)
            .any(|quoted| quoted.contains(';'));

        assert!(!in_a_string);
        assert!(line.contains("'keyway@local'"));
    }
}
