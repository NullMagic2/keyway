//! Module: keyway::run
//! Purpose: The agent's event loop: router socket, desktop, configuration file and signals.
//! Created: 2026-11-08
//! Architecture: Performs only the I/O; agent.rs decides. Each source blocks on its own
//! thread and sends one event queue, so the agent itself never waits on any of them.

use crate::agent::{Agent, AgentAction};
use keyway_core::code_pages::{CodePageCatalog, LayoutCodePages};
use keyway_core::config::Config;
use keyway_core::protocol::{AgentMessage, LineFramer, decode_message, encode_message};
use keyway_desktop::config_file::{CONFIG_FILE_NAME, config_directory, load_config};
use keyway_desktop::desktop_shell::{DesktopEvent, DesktopShell};
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use nix::sys::signal::{SigSet, Signal};
use std::fs;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

const ROUTER_RETRY_DELAY: Duration = Duration::from_secs(1);
const READ_CHUNK_BYTES: usize = 4096;

enum Event {
    Stop,
    DesktopChanged,
    ExtensionStopped,
    ConfigChanged,
    RouterConnected(u64, UnixStream),
    RouterLine(u64, String),
    RouterLost(u64),
}

/// Identifies the current router connection, so a thread still reporting on an abandoned
/// connection can be recognized and ignored.
#[derive(Clone, Debug, Default)]
struct ConnectionGeneration(Arc<AtomicU64>);

impl ConnectionGeneration {
    fn current(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    fn advance(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
}

fn spawn_signal_thread(events: Sender<Event>) -> io::Result<()> {
    let mut signals = SigSet::empty();
    signals.add(Signal::SIGINT);
    signals.add(Signal::SIGTERM);
    signals.thread_block()?;

    thread::spawn(move || {
        if signals.wait().is_ok() {
            let _ = events.send(Event::Stop);
        }
    });

    Ok(())
}

fn spawn_config_watcher(directory: &Path, events: Sender<Event>) -> io::Result<()> {
    fs::create_dir_all(directory)?;

    let inotify = Inotify::init(InitFlags::IN_CLOEXEC)?;
    let flags =
        AddWatchFlags::IN_CLOSE_WRITE | AddWatchFlags::IN_MOVED_TO | AddWatchFlags::IN_DELETE;
    inotify.add_watch(directory, flags)?;

    thread::spawn(move || {
        while let Ok(changes) = inotify.read_events() {
            let is_config = |name: &std::ffi::OsString| name == CONFIG_FILE_NAME;

            if changes
                .iter()
                .any(|change| change.name.as_ref().is_some_and(is_config))
                && events.send(Event::ConfigChanged).is_err()
            {
                return;
            }
        }
    });

    Ok(())
}

/// Keeps trying to reach the router until it succeeds or this attempt is abandoned.
fn spawn_router_connector(
    socket_path: PathBuf,
    generation: ConnectionGeneration,
    attempt: u64,
    delay_first: bool,
    events: Sender<Event>,
) {
    thread::spawn(move || {
        if delay_first {
            thread::sleep(ROUTER_RETRY_DELAY);
        }

        while generation.current() == attempt {
            if let Ok(stream) = UnixStream::connect(&socket_path) {
                let _ = events.send(Event::RouterConnected(attempt, stream));

                return;
            }

            thread::sleep(ROUTER_RETRY_DELAY);
        }
    });
}

fn spawn_router_reader(mut stream: UnixStream, connection: u64, events: Sender<Event>) {
    thread::spawn(move || {
        let mut framer = LineFramer::default();
        let mut chunk = [0; READ_CHUNK_BYTES];

        loop {
            let bytes_read = match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(bytes_read) => bytes_read,
            };

            let Ok(lines) = framer.push(&chunk[..bytes_read]) else {
                break;
            };

            for line in lines {
                if events.send(Event::RouterLine(connection, line)).is_err() {
                    return;
                }
            }
        }

        let _ = events.send(Event::RouterLost(connection));
    });
}

fn log_state(message: &str) {
    eprintln!("keyway: {message}");
}

struct Runtime {
    agent: Agent,
    shell: DesktopShell,
    socket_path: PathBuf,
    config_path: PathBuf,
    events: Sender<Event>,
    generation: ConnectionGeneration,
    router: Option<UnixStream>,
    connecting: bool,
}

impl Runtime {
    fn handle_desktop_changed(&mut self) {
        let was_available = self.agent.desktop_available();

        match self.shell.snapshot() {
            Ok(snapshot) => {
                if !was_available {
                    log_state("desktop extension available");
                }

                let actions = self.agent.desktop_changed(snapshot);
                self.perform(actions);
                self.connect_router(false);
            }
            Err(error) => {
                if was_available {
                    log_state(&format!("desktop extension unavailable: {error}"));
                }

                self.lose_desktop();
            }
        }
    }

    /// Without the extension nothing can be switched or typed, so the keyboards go back to
    /// the desktop until it returns.
    fn lose_desktop(&mut self) {
        self.agent.desktop_lost();
        self.drop_router();
    }

    /// The extension may already be back, as when an upgrade replaced it, so this starts over
    /// with whichever one answers now.
    fn handle_extension_stopped(&mut self) {
        if self.agent.desktop_available() {
            log_state("desktop extension stopped");
        }

        self.lose_desktop();
        self.handle_desktop_changed();
    }

    fn handle_config_changed(&mut self) {
        match load_config(&self.config_path) {
            Ok(config) => {
                log_state("configuration reloaded");

                let actions = self.agent.config_changed(config);
                self.perform(actions);
            }
            Err(error) => log_state(&format!("keeping previous configuration: {error}")),
        }
    }

    fn connect_router(&mut self, delay_first: bool) {
        if self.router.is_some() || self.connecting || !self.agent.desktop_available() {
            return;
        }

        self.connecting = true;
        spawn_router_connector(
            self.socket_path.clone(),
            self.generation.clone(),
            self.generation.current(),
            delay_first,
            self.events.clone(),
        );
    }

    fn drop_router(&mut self) {
        if let Some(router) = self.router.take() {
            let _ = router.shutdown(Shutdown::Both);
        }

        self.generation.advance();
        self.connecting = false;
        self.agent.router_disconnected();
    }

    fn handle_router_connected(&mut self, connection: u64, stream: UnixStream) {
        if connection != self.generation.current() {
            return;
        }

        let Ok(reader) = stream.try_clone() else {
            self.handle_router_lost(connection);
            return;
        };

        spawn_router_reader(reader, connection, self.events.clone());
        self.router = Some(stream);
        self.connecting = false;

        let actions = self.agent.router_connected();
        self.perform(actions);
    }

    fn handle_router_line(&mut self, connection: u64, line: &str) {
        if connection != self.generation.current() {
            return;
        }

        match decode_message(line) {
            Ok(message) => {
                let actions = self.agent.router_message(message);
                self.perform(actions);
            }
            Err(error) => {
                log_state(&format!("dropping router: {error}"));
                self.handle_router_lost(connection);
            }
        }
    }

    /// Waits before reconnecting, because a router that refuses this agent closes at once.
    fn handle_router_lost(&mut self, connection: u64) {
        if connection != self.generation.current() {
            return;
        }

        self.drop_router();
        self.connect_router(true);
    }

    fn perform(&mut self, actions: Vec<AgentAction>) {
        for action in actions {
            match action {
                AgentAction::Activate(source) => {
                    if let Err(error) = self.shell.activate(&source) {
                        log_state(&format!("could not activate {source}: {error}"));
                    }
                }
                AgentAction::CommitText(text) => {
                    if let Err(error) = self.shell.commit_text(&text) {
                        log_state(&format!("could not type a character: {error}"));
                    }
                }
                AgentAction::ShowFlags(shown) => {
                    if let Err(error) = self.shell.show_flags(shown) {
                        log_state(&format!("could not set the indicator's flags: {error}"));
                    }
                }
                AgentAction::Send(message) => self.send_to_router(&message),
                AgentAction::DropRouter => {
                    log_state("router speaks another protocol version");
                    self.handle_router_lost(self.generation.current());
                }
            }
        }
    }

    fn send_to_router(&mut self, message: &AgentMessage) {
        let Some(router) = &mut self.router else {
            return;
        };

        let sent = encode_message(message)
            .map_err(|error| error.to_string())
            .and_then(|line| {
                router
                    .write_all(line.as_bytes())
                    .map_err(|error| error.to_string())
            });

        if sent.is_err() {
            self.handle_router_lost(self.generation.current());
        }
    }

    fn run(mut self, receiver: &Receiver<Event>) {
        self.handle_desktop_changed();

        while let Ok(event) = receiver.recv() {
            match event {
                Event::Stop => break,
                Event::DesktopChanged => self.handle_desktop_changed(),
                Event::ExtensionStopped => self.handle_extension_stopped(),
                Event::ConfigChanged => self.handle_config_changed(),
                Event::RouterConnected(connection, stream) => {
                    self.handle_router_connected(connection, stream);
                }
                Event::RouterLine(connection, line) => {
                    self.handle_router_line(connection, &line);
                }
                Event::RouterLost(connection) => self.handle_router_lost(connection),
            }
        }

        log_state("stopping");
    }
}

/// Runs the agent until it is told to stop. Closing the socket on exit is what tells the
/// router to give the keyboards back.
pub fn run_agent(socket_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let (events, receiver) = mpsc::channel();

    // Blocked before any other thread starts, so all of them inherit the mask.
    spawn_signal_thread(events.clone())?;

    let directory = config_directory()?;
    let config_path = directory.join(CONFIG_FILE_NAME);
    spawn_config_watcher(&directory, events.clone())?;

    let config = load_config(&config_path).unwrap_or_else(|error| {
        log_state(&format!("using default configuration: {error}"));
        Config::default()
    });

    let shell = DesktopShell::connect()?;
    let desktop_events = events.clone();
    let session_events = events.clone();
    shell.watch(
        move |event| {
            let _ = desktop_events.send(match event {
                DesktopEvent::Changed => Event::DesktopChanged,
                DesktopEvent::ExtensionStopped => Event::ExtensionStopped,
            });
        },
        move || {
            let _ = session_events.send(Event::Stop);
        },
    )?;

    let runtime = Runtime {
        agent: Agent::new(config, CodePageCatalog::load()?, LayoutCodePages::load()?),
        shell,
        socket_path,
        config_path,
        events,
        generation: ConnectionGeneration::default(),
        router: None,
        connecting: false,
    };

    log_state("running");
    runtime.run(&receiver);

    Ok(())
}
