//! Module: keyway_router::router
//! Purpose: The event loop connecting devices, the virtual keyboard and the agent.
//! Created: 2026-11-08
//! Architecture: Performs only the I/O; key_router and keyway-core make every decision.
//! Keyboards stay closed until an agent is configured, so a login screen is untouched.

use crate::devices::{DeviceFilter, INPUT_DIRECTORY, Keyboard, LedState, is_event_node};
use crate::key_router::{DeviceId, KeyRouter, RouterOutput};
use crate::native::LED_NUML;
use crate::session::{SEAT_STATE_DIRECTORY, SessionCheck};
use crate::socket::{AgentConnection, AgentListener};
use crate::virtual_keyboard::{ROUTER_DEVICE_NAME, VirtualKeyboard};
use keyway_core::protocol::{AgentMessage, PROTOCOL_VERSION, RouterConfig, RouterMessage};
use nix::errno::Errno;
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor};
use nix::sys::signal::{SigSet, Signal};
use nix::sys::signalfd::{SfdFlags, SignalFd};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::fd::{AsFd, BorrowedFd};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct RouterOptions {
    pub socket_path: PathBuf,
    /// When not empty, the only keyboard names that may be grabbed.
    pub only_devices: Vec<String>,
    /// Accept this user without asking logind, for tests with no session.
    pub trusted_uid: Option<u32>,
}

/// How far a connected agent has got. It is not read from until logind has confirmed it
/// owns the active session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentStage {
    AwaitingSessionCheck,
    AwaitingHello,
    Ready,
}

#[derive(Debug)]
struct Agent {
    connection: AgentConnection,
    uid: u32,
    stage: AgentStage,
}

/// Something poll reported as ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Signal,
    Inotify,
    Listener,
    VirtualKeyboard,
    Agent,
    SessionCheck,
    Keyboard(DeviceId),
}

#[derive(Debug)]
pub struct Router {
    options: RouterOptions,
    signals: SignalFd,
    inotify: Inotify,
    input_watch: WatchDescriptor,
    seat_watch: Option<WatchDescriptor>,
    listener: AgentListener,
    virtual_keyboard: VirtualKeyboard,
    logic: KeyRouter,
    agent: Option<Agent>,
    agent_failed: bool,
    session_check: Option<SessionCheck>,
    keyboards: Vec<Keyboard>,
    next_device_id: u64,
    filter: DeviceFilter,
    leds: LedState,
}

impl Router {
    /// Sets up everything except the keyboards. Signals are blocked before the first
    /// thread starts, so only the signal descriptor ever receives them.
    pub fn new(options: RouterOptions) -> io::Result<Self> {
        let mut signal_mask = SigSet::empty();
        signal_mask.add(Signal::SIGINT);
        signal_mask.add(Signal::SIGTERM);
        signal_mask.thread_block()?;

        let signals =
            SignalFd::with_flags(&signal_mask, SfdFlags::SFD_NONBLOCK | SfdFlags::SFD_CLOEXEC)?;

        let inotify = Inotify::init(InitFlags::IN_NONBLOCK | InitFlags::IN_CLOEXEC)?;
        let input_flags =
            AddWatchFlags::IN_CREATE | AddWatchFlags::IN_ATTRIB | AddWatchFlags::IN_DELETE;
        let input_watch = inotify.add_watch(Path::new(INPUT_DIRECTORY), input_flags)?;

        let seat_flags = AddWatchFlags::IN_CLOSE_WRITE | AddWatchFlags::IN_MOVED_TO;
        let seat_watch = match options.trusted_uid {
            Some(_) => None,
            None => Some(inotify.add_watch(Path::new(SEAT_STATE_DIRECTORY), seat_flags)?),
        };

        let filter = DeviceFilter {
            excluded: Vec::new(),
            only: options.only_devices.clone(),
        };

        Ok(Self {
            signals,
            inotify,
            input_watch,
            seat_watch,
            listener: AgentListener::bind(&options.socket_path)?,
            virtual_keyboard: VirtualKeyboard::create(ROUTER_DEVICE_NAME)?,
            options,
            logic: KeyRouter::default(),
            agent: None,
            agent_failed: false,
            session_check: None,
            keyboards: Vec::new(),
            next_device_id: 0,
            filter,
            leds: LedState::default(),
        })
    }

    /// Runs until a stop signal, and returns Err only for failures of the virtual keyboard
    /// or the loop itself. The process then exits and the kernel releases every grab.
    pub fn run(mut self) -> io::Result<()> {
        log_state("ready");

        loop {
            for source in self.wait_for_sources()? {
                if source == Source::Signal {
                    let outputs = self.logic.release_all();
                    self.perform(outputs)?;
                    log_state("stopping");

                    return Ok(());
                }

                self.handle_source(source)?;

                if self.agent_failed {
                    self.disconnect_agent()?;
                }
            }
        }
    }

    fn wait_for_sources(&self) -> io::Result<Vec<Source>> {
        let mut sources: Vec<(Source, BorrowedFd<'_>)> = vec![
            (Source::Signal, self.signals.as_fd()),
            (Source::Inotify, self.inotify.as_fd()),
            (Source::Listener, self.listener.as_fd()),
            (Source::VirtualKeyboard, self.virtual_keyboard.as_fd()),
        ];

        if let Some(agent) = &self.agent
            && agent.stage != AgentStage::AwaitingSessionCheck
        {
            sources.push((Source::Agent, agent.connection.as_fd()));
        }

        if let Some(check) = &self.session_check {
            sources.push((Source::SessionCheck, check.as_fd()));
        }

        for keyboard in &self.keyboards {
            sources.push((Source::Keyboard(keyboard.id), keyboard.as_fd()));
        }

        let mut poll_fds: Vec<PollFd<'_>> = sources
            .iter()
            .map(|(_source, fd)| PollFd::new(*fd, PollFlags::POLLIN))
            .collect();

        loop {
            match poll(&mut poll_fds, PollTimeout::NONE) {
                Ok(_) => break,
                Err(Errno::EINTR) => {}
                Err(error) => return Err(error.into()),
            }
        }

        let is_ready = |fd: &PollFd<'_>| fd.revents().is_some_and(|flags| !flags.is_empty());

        Ok(sources
            .iter()
            .zip(&poll_fds)
            .filter(|(_source, fd)| is_ready(fd))
            .map(|((source, _fd), _poll_fd)| *source)
            .collect())
    }

    fn handle_source(&mut self, source: Source) -> io::Result<()> {
        match source {
            Source::Signal => Ok(()),
            Source::Inotify => self.handle_inotify(),
            Source::Listener => self.handle_connection(),
            Source::VirtualKeyboard => self.handle_leds(),
            Source::Agent => self.handle_agent_messages(),
            Source::SessionCheck => self.handle_session_answer(),
            Source::Keyboard(id) => self.handle_keyboard(id),
        }
    }

    fn perform(&mut self, outputs: Vec<RouterOutput>) -> io::Result<()> {
        for output in outputs {
            match output {
                RouterOutput::Emit(event) => self.virtual_keyboard.emit(&[event])?,
                RouterOutput::Send(message) => self.send_to_agent(&message),
            }
        }

        Ok(())
    }

    fn send_to_agent(&mut self, message: &RouterMessage) {
        if let Some(agent) = &mut self.agent
            && agent.connection.send(message).is_err()
        {
            self.agent_failed = true;
        }
    }

    fn handle_keyboard(&mut self, id: DeviceId) -> io::Result<()> {
        let Some(keyboard) = self.keyboards.iter_mut().find(|kb| kb.id == id) else {
            return Ok(());
        };

        let events = match keyboard.read_keys() {
            Ok(events) => events,
            Err(_) => return self.remove_keyboard(id),
        };

        if !keyboard.is_grabbed() {
            return self.try_grab_keyboard(id);
        }

        for event in events {
            let outputs = self.logic.key_event(id, event);
            self.perform(outputs)?;
        }

        Ok(())
    }

    fn try_grab_keyboard(&mut self, id: DeviceId) -> io::Result<()> {
        let led_changes = self.leds.as_changes();
        let Some(keyboard) = self.keyboards.iter_mut().find(|kb| kb.id == id) else {
            return Ok(());
        };

        let result = match keyboard.try_grab() {
            Ok(true) => keyboard.apply_leds(&led_changes).map(|()| true),
            other => other,
        };

        match result {
            Ok(true) => {
                log_state(&format!("grabbed {:?}", keyboard.name));
                Ok(())
            }
            Ok(false) => Ok(()),
            Err(_) => self.remove_keyboard(id),
        }
    }

    fn remove_keyboard(&mut self, id: DeviceId) -> io::Result<()> {
        let Some(index) = self.keyboards.iter().position(|kb| kb.id == id) else {
            return Ok(());
        };

        let keyboard = self.keyboards.remove(index);
        log_state(&format!("lost {:?}", keyboard.name));

        let outputs = self.logic.device_removed(id);

        self.perform(outputs)
    }

    fn handle_leds(&mut self) -> io::Result<()> {
        let changes = self.virtual_keyboard.read_led_changes()?;

        for change in &changes {
            self.leds.record(*change);

            if change.led == LED_NUML {
                self.logic.set_numlock(change.on);
            }
        }

        // A failed write means the keyboard is going away; its read error will remove it.
        for keyboard in &mut self.keyboards {
            if keyboard.is_grabbed() {
                let _ = keyboard.apply_leds(&changes);
            }
        }

        Ok(())
    }

    fn handle_inotify(&mut self) -> io::Result<()> {
        let events = match self.inotify.read_events() {
            Ok(events) => events,
            Err(Errno::EAGAIN) => return Ok(()),
            Err(error) => return Err(error.into()),
        };

        for event in events {
            if Some(event.wd) == self.seat_watch {
                self.start_session_check()?;
            } else if event.wd == self.input_watch
                && let Some(name) = &event.name
            {
                self.handle_input_node_change(name, event.mask)?;
            }
        }

        Ok(())
    }

    fn handle_input_node_change(&mut self, name: &OsStr, mask: AddWatchFlags) -> io::Result<()> {
        let is_event = name.to_str().is_some_and(is_event_node);

        if !is_event || !self.logic.is_configured() {
            return Ok(());
        }

        let path = Path::new(INPUT_DIRECTORY).join(name);

        if mask.contains(AddWatchFlags::IN_DELETE) {
            let removed = self.keyboards.iter().find(|kb| kb.path == path);

            return match removed.map(|kb| kb.id) {
                Some(id) => self.remove_keyboard(id),
                None => Ok(()),
            };
        }

        self.open_keyboard(&path)
    }

    /// Opens and tries to grab one node. Failure is normal here: it may not be ready, may
    /// already be gone, or may be someone else's, and a later event retries it.
    fn open_keyboard(&mut self, path: &Path) -> io::Result<()> {
        if self.keyboards.iter().any(|kb| kb.path == path) {
            return Ok(());
        }

        let id = DeviceId(self.next_device_id);

        if let Ok(Some(keyboard)) = Keyboard::open(id, path, &self.filter) {
            self.next_device_id += 1;
            self.keyboards.push(keyboard);

            return self.try_grab_keyboard(id);
        }

        Ok(())
    }

    fn open_all_keyboards(&mut self) -> io::Result<()> {
        for entry in fs::read_dir(INPUT_DIRECTORY)? {
            let entry = entry?;

            if entry.file_name().to_str().is_some_and(is_event_node) {
                self.open_keyboard(&entry.path())?;
            }
        }

        Ok(())
    }

    /// Takes one connection. Only one agent is served at a time; a second is refused by
    /// closing it, and an agent that stops gets its socket closed so it can reconnect.
    fn handle_connection(&mut self) -> io::Result<()> {
        let Some((connection, uid)) = self.listener.accept()? else {
            return Ok(());
        };

        if self.agent.is_some() {
            log_state("refused a second agent");

            return Ok(());
        }

        if let Some(trusted_uid) = self.options.trusted_uid {
            if uid != trusted_uid {
                log_state(&format!("refused agent of user {uid}"));

                return Ok(());
            }

            self.agent = Some(Agent {
                connection,
                uid,
                stage: AgentStage::AwaitingHello,
            });

            return Ok(());
        }

        self.agent = Some(Agent {
            connection,
            uid,
            stage: AgentStage::AwaitingSessionCheck,
        });

        self.start_session_check()
    }

    fn start_session_check(&mut self) -> io::Result<()> {
        if self.agent.is_some() {
            self.session_check = Some(SessionCheck::start()?);
        }

        Ok(())
    }

    fn handle_session_answer(&mut self) -> io::Result<()> {
        let active_uid = self.session_check.take().and_then(SessionCheck::finish);
        let Some(agent) = &mut self.agent else {
            return Ok(());
        };

        if active_uid != Some(agent.uid) {
            log_state(&format!("agent of user {} is not active", agent.uid));

            return self.disconnect_agent();
        }

        if agent.stage == AgentStage::AwaitingSessionCheck {
            agent.stage = AgentStage::AwaitingHello;
        }

        Ok(())
    }

    fn handle_agent_messages(&mut self) -> io::Result<()> {
        let Some(agent) = &mut self.agent else {
            return Ok(());
        };

        let messages = match agent.connection.read_messages() {
            Ok(messages) => messages,
            Err(_) => return self.disconnect_agent(),
        };

        for message in messages {
            self.handle_agent_message(message)?;
        }

        Ok(())
    }

    fn handle_agent_message(&mut self, message: AgentMessage) -> io::Result<()> {
        let Some(stage) = self.agent.as_ref().map(|agent| agent.stage) else {
            return Ok(());
        };

        match (stage, message) {
            (AgentStage::AwaitingHello, AgentMessage::Hello { version }) => {
                self.handle_hello(version);
                Ok(())
            }
            (AgentStage::Ready, AgentMessage::Configure(config)) => self.configure(config),
            (AgentStage::Ready, AgentMessage::SetAllowRightAlt(allow_right_alt)) => {
                self.logic.set_allow_right_alt(allow_right_alt);
                Ok(())
            }
            _ => {
                log_state("agent broke the protocol");
                self.disconnect_agent()
            }
        }
    }

    fn handle_hello(&mut self, version: u32) {
        self.send_to_agent(&RouterMessage::Hello {
            version: PROTOCOL_VERSION,
        });

        if version != PROTOCOL_VERSION {
            log_state(&format!("agent speaks protocol {version}"));
            self.agent_failed = true;
        } else if let Some(agent) = &mut self.agent {
            agent.stage = AgentStage::Ready;
        }
    }

    /// Applies a configuration: closes keyboards it now excludes, then grabs the rest.
    fn configure(&mut self, config: RouterConfig) -> io::Result<()> {
        self.filter.excluded = config.excluded_devices.clone();
        self.logic.configure(config);

        let filter = self.filter.clone();
        let now_excluded: Vec<DeviceId> = self
            .keyboards
            .iter()
            .filter(|kb| !filter.allows(&kb.name))
            .map(|kb| kb.id)
            .collect();

        for id in now_excluded {
            self.remove_keyboard(id)?;
        }

        self.open_all_keyboards()
    }

    /// Releases held keys first, so nothing is stuck by the time the desktop gets the
    /// physical keyboards back.
    fn disconnect_agent(&mut self) -> io::Result<()> {
        self.agent = None;
        self.agent_failed = false;
        self.session_check = None;

        let outputs = self.logic.agent_lost();
        self.perform(outputs)?;
        self.keyboards.clear();
        log_state("agent gone; keyboards released");

        Ok(())
    }
}

/// Reports router state to the journal. Never pass this anything derived from a key.
fn log_state(message: &str) {
    eprintln!("keyway-router: {message}");
}
