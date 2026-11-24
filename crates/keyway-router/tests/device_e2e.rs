//! Module: keyway_router::tests::device_e2e
//! Purpose: Run the real router binary against real kernel devices.
//! Created: 2026-11-08
//! Architecture: Run through tools/run_vm_tests.sh only. These tests type on virtual
//! keyboards, which on a live desktop would reach whatever window has focus.

use keyway_core::alt_code::{AltCodeEntry, AltCodeRadix, AltCodeRules, NumLockPolicy};
use keyway_core::hotkey::{Chord, ShortcutAction, ShortcutBinding};
use keyway_core::key_codes::{KeyCode, KeyEvent};
use keyway_core::protocol::{
    AgentMessage, PROTOCOL_VERSION, RouterConfig, RouterMessage, decode_message, encode_message,
};
use keyway_router::native::{self, EV_LED, RawEvent};
use keyway_router::virtual_keyboard::{LedChange, ROUTER_DEVICE_NAME, VirtualKeyboard};
use nix::errno::Errno;
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::{Duration, Instant};

const VM_GUARD_VARIABLE: &str = "KEYWAY_ISOLATED_VM";
const ROUTER_BINARY_VARIABLE: &str = "KEYWAY_ROUTER_BIN";
const SOCKET_PATH: &str = "/tmp/keyway-test.sock";
const INPUT_DIRECTORY: &str = "/dev/input";
const FIRST_KEYBOARD_NAME: &str = "Keyway test keyboard one";
const SECOND_KEYBOARD_NAME: &str = "Keyway test keyboard two";
const ROOT_UID: u32 = 0;
const OTHER_UID: u32 = 12345;
const LED_CAPSL: u16 = 1;

const WAIT_LIMIT: Duration = Duration::from_secs(5);
const SETTLE_TIME: Duration = Duration::from_millis(300);
const RETRY_INTERVAL: Duration = Duration::from_millis(10);
const POLL_SLICE_MS: u16 = 50;

fn require_isolated_vm() {
    assert!(
        std::env::var_os(VM_GUARD_VARIABLE).is_some(),
        "refusing to run outside the VM: these tests would type into the desktop"
    );
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();

    while !condition() {
        assert!(start.elapsed() < WAIT_LIMIT, "timed out waiting for {what}");
        sleep(RETRY_INTERVAL);
    }
}

fn find_node(name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(INPUT_DIRECTORY).ok()? {
        let path = entry.ok()?.path();
        let is_named = OpenOptions::new()
            .read(true)
            .open(&path)
            .ok()
            .and_then(|file| native::device_name(file.as_fd()).ok())
            .is_some_and(|node_name| node_name == name);

        if is_named {
            return Some(path);
        }
    }

    None
}

fn wait_for_node(name: &str) -> PathBuf {
    let mut found = None;

    wait_until(&format!("device {name:?}"), || {
        found = find_node(name);
        found.is_some()
    });

    found.unwrap()
}

fn is_grabbed(path: &Path) -> bool {
    let file = OpenOptions::new().read(true).open(path).unwrap();

    match native::set_grabbed(file.as_fd(), true) {
        Ok(()) => {
            native::set_grabbed(file.as_fd(), false).unwrap();
            false
        }
        Err(Errno::EBUSY) => true,
        Err(error) => panic!("unexpected grab result: {error}"),
    }
}

/// A virtual keyboard standing in for a physical one, with its event node.
struct SourceKeyboard {
    device: VirtualKeyboard,
    node: PathBuf,
}

impl SourceKeyboard {
    fn create(name: &str) -> Self {
        let device = VirtualKeyboard::create(name).unwrap();

        Self {
            device,
            node: wait_for_node(name),
        }
    }

    fn press(&mut self, key: KeyCode) {
        self.device.emit(&[KeyEvent::press(key)]).unwrap();
    }

    fn release(&mut self, key: KeyCode) {
        self.device.emit(&[KeyEvent::release(key)]).unwrap();
    }

    fn tap(&mut self, key: KeyCode) {
        self.press(key);
        self.release(key);
    }

    /// Waits before probing, because the probe itself grabs for a moment and could make
    /// the router's own first attempt fail.
    fn wait_until_grabbed(&self) {
        sleep(SETTLE_TIME);
        wait_until("the keyboard to be grabbed", || is_grabbed(&self.node));
    }

    fn wait_until_released(&self) {
        wait_until("the keyboard to be released", || !is_grabbed(&self.node));
    }
}

/// Reads what the router's virtual keyboard delivers to the desktop.
struct DesktopView {
    node: File,
}

impl DesktopView {
    fn open() -> Self {
        let path = wait_for_node(ROUTER_DEVICE_NAME);
        let node = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .unwrap();

        Self { node }
    }

    fn read_keys(&self, limit: Duration, wanted: usize) -> Vec<KeyEvent> {
        let start = Instant::now();
        let mut keys = Vec::new();

        while start.elapsed() < limit && keys.len() < wanted {
            let mut poll_fds = [PollFd::new(self.node.as_fd(), PollFlags::POLLIN)];
            poll(&mut poll_fds, PollTimeout::from(POLL_SLICE_MS)).unwrap();

            let events = native::read_available_events(&self.node).unwrap();
            keys.extend(events.into_iter().filter_map(RawEvent::to_key_event));
        }

        keys
    }

    fn expect(&self, expected: &[KeyEvent]) {
        assert_eq!(self.read_keys(WAIT_LIMIT, expected.len()), expected);
    }

    fn expect_silence(&self) {
        assert_eq!(self.read_keys(SETTLE_TIME, usize::MAX), []);
    }

    fn set_led(&mut self, led: u16, on: bool) {
        let event = RawEvent {
            kind: EV_LED,
            code: led,
            value: i32::from(on),
        };
        let bytes = [event.encode(), RawEvent::SYNC.encode()].concat();

        self.node.write_all(&bytes).unwrap();
    }
}

struct TestAgent {
    reader: BufReader<UnixStream>,
}

impl TestAgent {
    fn connect() -> Self {
        let stream = UnixStream::connect(SOCKET_PATH).unwrap();
        stream.set_read_timeout(Some(WAIT_LIMIT)).unwrap();

        Self {
            reader: BufReader::new(stream),
        }
    }

    fn send(&mut self, message: &AgentMessage) {
        let line = encode_message(message).unwrap();

        self.reader.get_mut().write_all(line.as_bytes()).unwrap();
    }

    fn receive(&mut self) -> RouterMessage {
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();

        decode_message(&line).unwrap()
    }

    fn was_closed_by_router(&mut self) -> bool {
        let mut line = String::new();

        matches!(self.reader.read_line(&mut line), Ok(0))
    }

    fn greet(&mut self) {
        self.send(&AgentMessage::Hello {
            version: PROTOCOL_VERSION,
        });

        assert_eq!(
            self.receive(),
            RouterMessage::Hello {
                version: PROTOCOL_VERSION
            }
        );
    }

    fn configure(&mut self, excluded_devices: &[&str]) {
        self.send(&AgentMessage::Configure(RouterConfig {
            shortcuts: vec![ShortcutBinding {
                chord: Chord::ALT_SHIFT,
                action: ShortcutAction::NextLayout,
            }],
            alt_codes: AltCodeRules {
                enabled: true,
                numlock: NumLockPolicy::Ignored,
                hex_numpad: false,
                allow_right_alt: false,
            },
            excluded_devices: excluded_devices.iter().map(ToString::to_string).collect(),
        }));
    }
}

/// The router process; killed when dropped so a failed test leaves nothing behind.
struct RouterProcess {
    child: Child,
}

impl RouterProcess {
    fn start(trusted_uid: u32) -> Self {
        let binary = std::env::var(ROUTER_BINARY_VARIABLE)
            .unwrap_or_else(|_| env!("CARGO_BIN_EXE_keyway-router").to_string());
        let child = Command::new(binary)
            .args(["--socket", SOCKET_PATH])
            .args(["--only-device", FIRST_KEYBOARD_NAME])
            .args(["--only-device", SECOND_KEYBOARD_NAME])
            .args(["--trust-uid", &trusted_uid.to_string()])
            .spawn()
            .unwrap();

        // The router binds its socket before creating this device. A socket left by a killed
        // router still exists, so waiting for that file would not prove anything.
        wait_for_node(ROUTER_DEVICE_NAME);

        Self { child }
    }

    fn pid(&self) -> Pid {
        Pid::from_raw(i32::try_from(self.child.id()).unwrap())
    }
}

impl Drop for RouterProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn press(key: KeyCode) -> KeyEvent {
    KeyEvent::press(key)
}

fn release(key: KeyCode) -> KeyEvent {
    KeyEvent::release(key)
}

fn run_against_one_router(source: &mut SourceKeyboard) {
    let router = RouterProcess::start(ROOT_UID);
    let mut desktop = DesktopView::open();

    // Without an agent, the keyboard belongs to the desktop.
    sleep(SETTLE_TIME);
    assert!(!is_grabbed(&source.node));
    source.tap(KeyCode::A);
    desktop.expect_silence();

    // A key held when the agent arrives delays the grab, so its release is not lost.
    source.press(KeyCode::A);
    let mut agent = TestAgent::connect();
    agent.greet();
    agent.configure(&[]);
    sleep(SETTLE_TIME);
    assert!(!is_grabbed(&source.node));
    desktop.expect_silence();
    source.release(KeyCode::A);
    source.wait_until_grabbed();
    desktop.expect_silence();

    // Ordinary keys are forwarded.
    source.tap(KeyCode::B);
    desktop.expect(&[press(KeyCode::B), release(KeyCode::B)]);

    // The default shortcut fires on release, with both modifiers still forwarded.
    source.press(KeyCode::LEFT_ALT);
    source.press(KeyCode::LEFT_SHIFT);
    source.release(KeyCode::LEFT_SHIFT);
    source.release(KeyCode::LEFT_ALT);
    desktop.expect(&[
        press(KeyCode::LEFT_ALT),
        press(KeyCode::LEFT_SHIFT),
        release(KeyCode::LEFT_SHIFT),
        release(KeyCode::LEFT_ALT),
    ]);
    assert_eq!(
        agent.receive(),
        RouterMessage::Shortcut(ShortcutAction::NextLayout)
    );

    // An Alt code hides its digits, masks Alt once and arrives after the Alt release.
    source.press(KeyCode::LEFT_ALT);
    source.tap(KeyCode::KP1);
    source.tap(KeyCode::KP3);
    source.tap(KeyCode::KP0);
    source.release(KeyCode::LEFT_ALT);
    desktop.expect(&[
        press(KeyCode::LEFT_ALT),
        press(KeyCode::MASK_KEY),
        release(KeyCode::MASK_KEY),
        release(KeyCode::LEFT_ALT),
    ]);
    assert_eq!(
        agent.receive(),
        RouterMessage::AltCode(
            AltCodeEntry::new(AltCodeRadix::Decimal, "130".to_string()).unwrap()
        )
    );

    // LEDs set by the desktop reach the physical keyboard.
    desktop.set_led(LED_CAPSL, true);
    let mut leds = Vec::new();
    wait_until("the LED change", || {
        leds.extend(source.device.read_led_changes().unwrap());
        leds.contains(&LedChange {
            led: LED_CAPSL,
            on: true,
        })
    });

    // A keyboard plugged in later is grabbed, and unplugging it releases its keys.
    let mut second = SourceKeyboard::create(SECOND_KEYBOARD_NAME);
    second.wait_until_grabbed();
    second.press(KeyCode::D);
    desktop.expect(&[press(KeyCode::D)]);
    drop(second);
    desktop.expect(&[release(KeyCode::D)]);

    // Reconfiguring can exclude a keyboard and take it back.
    agent.configure(&[FIRST_KEYBOARD_NAME]);
    source.wait_until_released();
    source.tap(KeyCode::E);
    desktop.expect_silence();
    agent.configure(&[]);
    source.wait_until_grabbed();

    // A second agent is refused while the first is connected.
    let mut intruder = TestAgent::connect();
    assert!(intruder.was_closed_by_router());

    // Losing the agent releases held keys and gives the keyboard back.
    source.press(KeyCode::F);
    desktop.expect(&[press(KeyCode::F)]);
    drop(agent);
    desktop.expect(&[release(KeyCode::F)]);
    source.wait_until_released();
    source.release(KeyCode::F);

    // A stop signal ends the router cleanly and removes its socket and device.
    kill(router.pid(), Signal::SIGTERM).unwrap();
    let mut router = router;
    let status = router.child.wait().unwrap();

    assert!(status.success());
    assert!(!Path::new(SOCKET_PATH).exists());
    wait_until("the virtual keyboard to go", || {
        find_node(ROUTER_DEVICE_NAME).is_none()
    });
}

fn run_crash_recovery(source: &mut SourceKeyboard) {
    let router = RouterProcess::start(ROOT_UID);
    let desktop = DesktopView::open();
    let mut agent = TestAgent::connect();
    agent.greet();
    agent.configure(&[]);
    source.wait_until_grabbed();
    source.press(KeyCode::A);
    desktop.expect(&[press(KeyCode::A)]);

    // The kernel alone must give the keyboard back when the router dies. A key held through
    // the router then depends on the desktop handling its device's removal, not tested here.
    kill(router.pid(), Signal::SIGKILL).unwrap();
    source.wait_until_released();
    source.release(KeyCode::A);
    wait_until("the virtual keyboard to go", || {
        find_node(ROUTER_DEVICE_NAME).is_none()
    });
}

fn run_refused_user(source: &SourceKeyboard) {
    let _router = RouterProcess::start(OTHER_UID);
    let mut agent = TestAgent::connect();

    assert!(agent.was_closed_by_router());
    assert!(!is_grabbed(&source.node));
}

#[test]
#[ignore = "needs real devices; run through tools/run_vm_tests.sh"]
fn the_router_works_on_real_kernel_devices() {
    require_isolated_vm();

    let mut source = SourceKeyboard::create(FIRST_KEYBOARD_NAME);

    run_against_one_router(&mut source);
    run_crash_recovery(&mut source);
    run_refused_user(&source);
}
