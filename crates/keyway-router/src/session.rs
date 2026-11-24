//! Module: keyway_router::session
//! Purpose: Find which user owns the active session, without blocking key routing.
//! Created: 2026-11-08
//! Architecture: The router only accepts an agent from this user. It asks systemd's sd-login
//! library, which needs no bus. Asking logind over D-Bus does not do: the system bus must then
//! resolve this service's dynamic user, and it dropped the connection on a real system instead.
//! The query runs on its own thread, because nothing here may ever delay the keyboard.

use crate::native;
use std::ffi::CStr;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::thread;

const SEAT_NAME: &CStr = c"seat0";

/// Directory logind rewrites when sessions change. Its contents are private, so it is
/// only watched as a signal to ask again, and never parsed.
pub const SEAT_STATE_DIRECTORY: &str = "/run/systemd/seats";

/// A running query for the active session's user. Dropping it abandons the query; the
/// thread then finishes on its own and its result goes nowhere.
#[derive(Debug)]
pub struct SessionCheck {
    result: UnixStream,
}

impl SessionCheck {
    pub fn start() -> io::Result<Self> {
        let (result, mut sender) = UnixStream::pair()?;

        thread::spawn(move || {
            let answer =
                native::seat_active_uid(SEAT_NAME).map_or_else(String::new, |uid| uid.to_string());
            let _ = sender.write_all(answer.as_bytes());
        });

        Ok(Self { result })
    }

    /// Reads the answer once the descriptor is readable: the active user, or None when
    /// systemd could not name one. The thread writes it all and closes, so this ends at once.
    pub fn finish(mut self) -> Option<u32> {
        let mut answer = String::new();
        self.result.read_to_string(&mut answer).ok()?;

        answer.parse().ok()
    }
}

impl AsFd for SessionCheck {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.result.as_fd()
    }
}
