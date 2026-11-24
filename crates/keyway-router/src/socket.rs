//! Module: keyway_router::socket
//! Purpose: The agent's Unix socket: peer identity, message framing and removal.
//! Created: 2026-11-08
//! Architecture: Any local user may connect; the router then decides by peer credentials.
//! Nothing here may block, because the same thread routes every key.

use keyway_core::protocol::{
    AgentMessage, LineFramer, RouterMessage, decode_message, encode_message,
};
use nix::sys::socket::{getsockopt, sockopt};
use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

const SOCKET_MODE: u32 = 0o666;
const READ_CHUNK_BYTES: usize = 4096;

#[derive(Debug)]
pub struct AgentListener {
    listener: UnixListener,
    path: PathBuf,
}

impl AgentListener {
    /// Binds the socket. An old socket left by a crash is replaced, but any other kind of
    /// file at that path is an error rather than something to delete.
    pub fn bind(path: &Path) -> io::Result<Self> {
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if !metadata.file_type().is_socket() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("not a socket: {}", path.display()),
                ));
            }

            fs::remove_file(path)?;
        }

        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        fs::set_permissions(path, fs::Permissions::from_mode(SOCKET_MODE))?;

        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }

    /// Accepts one waiting connection with its peer's user id, if there is one.
    pub fn accept(&self) -> io::Result<Option<(AgentConnection, u32)>> {
        let stream = match self.listener.accept() {
            Ok((stream, _address)) => stream,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(None),
            Err(error) => return Err(error),
        };

        let credentials = getsockopt(&stream, sockopt::PeerCredentials)?;

        Ok(Some((AgentConnection::new(stream)?, credentials.uid())))
    }
}

impl AsFd for AgentListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.listener.as_fd()
    }
}

impl Drop for AgentListener {
    fn drop(&mut self) {
        // A leftover socket is replaced at the next start, so a failure here is harmless.
        let _ = fs::remove_file(&self.path);
    }
}

/// A connected agent exchanging one JSON message per line.
#[derive(Debug)]
pub struct AgentConnection {
    stream: UnixStream,
    framer: LineFramer,
}

impl AgentConnection {
    fn new(stream: UnixStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;

        Ok(Self {
            stream,
            framer: LineFramer::default(),
        })
    }

    /// Reads what one read returns and decodes the messages it completes; poll reports the
    /// socket again if more is waiting. Err means the agent must be dropped: it hung up, or
    /// sent something too long or malformed.
    pub fn read_messages(&mut self) -> io::Result<Vec<AgentMessage>> {
        let mut chunk = [0; READ_CHUNK_BYTES];

        let bytes_read = match self.stream.read(&mut chunk) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(bytes_read) => bytes_read,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };

        let lines = self
            .framer
            .push(&chunk[..bytes_read])
            .map_err(invalid_data)?;

        lines
            .iter()
            .map(|line| decode_message(line).map_err(invalid_data))
            .collect()
    }

    /// Sends a message. An agent too slow to take it is dropped by the caller, because
    /// the router must never wait for one while it has keys to route.
    pub fn send(&mut self, message: &RouterMessage) -> io::Result<()> {
        let line = encode_message(message).map_err(invalid_data)?;

        self.stream.write_all(line.as_bytes())
    }
}

impl AsFd for AgentConnection {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.stream.as_fd()
    }
}

fn invalid_data(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyway_core::protocol::PROTOCOL_VERSION;

    fn connected_pair() -> (AgentConnection, UnixStream) {
        let (router_end, agent_end) = UnixStream::pair().unwrap();

        (AgentConnection::new(router_end).unwrap(), agent_end)
    }

    fn hello_line() -> String {
        let hello = AgentMessage::Hello {
            version: PROTOCOL_VERSION,
        };

        encode_message(&hello).unwrap()
    }

    #[test]
    fn nothing_waiting_is_not_an_error() {
        let (mut connection, _agent) = connected_pair();

        assert!(connection.read_messages().unwrap().is_empty());
    }

    #[test]
    fn messages_are_framed_across_partial_reads() {
        let (mut connection, mut agent) = connected_pair();
        let hello = hello_line();
        let (start, rest) = hello.split_at(5);

        agent.write_all(start.as_bytes()).unwrap();
        assert!(connection.read_messages().unwrap().is_empty());

        agent
            .write_all(format!("{rest}{hello}").as_bytes())
            .unwrap();
        assert_eq!(connection.read_messages().unwrap().len(), 2);
    }

    #[test]
    fn a_hang_up_is_reported_after_the_last_message() {
        let (mut connection, mut agent) = connected_pair();
        agent.write_all(hello_line().as_bytes()).unwrap();
        drop(agent);

        assert_eq!(connection.read_messages().unwrap().len(), 1);
        assert!(connection.read_messages().is_err());
    }

    #[test]
    fn malformed_and_oversized_input_drops_the_agent() {
        let (mut malformed, mut agent) = connected_pair();
        agent.write_all(b"{\"unknown\":1}\n").unwrap();

        assert!(malformed.read_messages().is_err());

        let (mut flooded, mut agent) = connected_pair();
        let chunk = vec![b'x'; READ_CHUNK_BYTES];

        let result = loop {
            agent.write_all(&chunk).unwrap();

            match flooded.read_messages() {
                Ok(messages) => assert!(messages.is_empty()),
                Err(error) => break error,
            }
        };

        assert_eq!(result.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_router_message_arrives_as_one_line() {
        let (mut connection, mut agent) = connected_pair();
        let hello = RouterMessage::Hello {
            version: PROTOCOL_VERSION,
        };
        connection.send(&hello).unwrap();
        drop(connection);

        let mut received = String::new();
        agent.read_to_string(&mut received).unwrap();

        assert_eq!(received.lines().count(), 1);
        assert_eq!(decode_message::<RouterMessage>(&received).unwrap(), hello);
    }

    #[test]
    fn only_a_stale_socket_may_be_replaced() {
        let directory = std::env::temp_dir().join(format!("keyway-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let socket_path = directory.join("router.sock");
        let file_path = directory.join("not-a-socket");
        fs::write(&file_path, "keep me").unwrap();

        assert!(AgentListener::bind(&file_path).is_err());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "keep me");

        let stale = UnixListener::bind(&socket_path).unwrap();
        drop(stale);
        let listener = AgentListener::bind(&socket_path).unwrap();
        let mode = fs::metadata(&socket_path).unwrap().permissions().mode();

        assert_eq!(mode & SOCKET_MODE, SOCKET_MODE);

        drop(listener);

        assert!(!socket_path.exists());

        fs::remove_dir_all(&directory).unwrap();
    }
}
