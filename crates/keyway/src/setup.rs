//! Module: keyway::setup
//! Purpose: Install and remove the runtime files embedded in this binary.
//! Created: 2026-11-08
//! Architecture: Embedding them lets bin/ work alone once clean_source.sh has run. The file
//! lists take a root directory, so the logic can be tested without touching the system.

use keyway_core::layout_labels::{FLAG_FILES, FLAGS_LICENSE, LABELS_FILE};
use keyway_desktop::command::run_command;
use keyway_desktop::desktop_shell::EXTENSION_UUID;
use keyway_desktop::restart::{AGENT_UNIT_NAME, ROUTER_UNIT_NAME};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const BINARY_PLACEHOLDER: &str = "@KEYWAY_BIN@";
const SETTINGS_PLACEHOLDER: &str = "@KEYWAY_SETTINGS_BIN@";
const ROUTER_PLACEHOLDER: &str = "@KEYWAY_ROUTER_BIN@";
const SYSTEM_ROUTER_BINARY: &str = "/usr/local/bin/keyway-router";

/// Installed by the .deb packages, which own the files that setup would otherwise write. It is
/// in a system data directory, as the package's extension and labels are.
const PACKAGED_MARKER: &str = "keyway/packaged";

/// The XDG default for system data directories, when XDG_DATA_DIRS is unset or empty.
const DEFAULT_SYSTEM_DATA_DIRS: &str = "/usr/local/share:/usr/share";
const SETTINGS_ENTRY_NAME: &str = "keyway-settings.desktop";
const SETTINGS_BINARY_NAME: &str = "keyway-settings";
const ROUTER_BINARY_NAME: &str = "keyway-router";
const AGENT_BINARY_NAME: &str = "keyway";
const INSTALLED_BINARIES: [&str; 3] = [ROUTER_BINARY_NAME, AGENT_BINARY_NAME, SETTINGS_BINARY_NAME];
const SYSTEM_BINARY_DIRECTORY: &str = "usr/local/bin";

/// A package's own session entry has this name too, so one here takes its place for this user.
const AUTOSTART_ENTRY_NAME: &str = "keyway.desktop";
const TURNED_ON_MARKER: &str = "keyway/turned-on";

/// The taskbar's layout labels and flags, where the extensions look for them.
const LABELS_DIRECTORY: &str = "keyway";
const LABELS_FILE_NAME: &str = "layout_labels.json";
const FLAGS_DIRECTORY: &str = "flags";
const FLAGS_LICENSE_NAME: &str = "LICENSE";
const FLAG_EXTENSION: &str = "svg";

/// The standard way to turn off a system-wide session entry for one user.
const AUTOSTART_OPT_OUT: &str = "[Desktop Entry]\nType=Application\nName=Keyway\nHidden=true\n";

/// The desktops' extension settings. The same user may log in to either desktop.
const EXTENSION_SCHEMAS: [&str; 2] = ["org.gnome.shell", "org.cinnamon"];
const ENABLED_EXTENSIONS_KEY: &str = "enabled-extensions";

/// GNOME's Extensions app lists an extension here when it is turned off, and that wins.
const DISABLED_EXTENSIONS_KEY: &str = "disabled-extensions";
const EXECUTABLE_MODE: u32 = 0o755;

/// The on-screen keyboard's look, which both extensions load from beside their own files.
const KEYBOARD_STYLESHEET: (&str, &str) = (
    "keyboard.css",
    include_str!("../../../extension/keyboard.css"),
);

/// Each desktop's extension files, by name and contents.
const GNOME_EXTENSION_FILES: [(&str, &str); 4] = [
    KEYBOARD_STYLESHEET,
    (
        "extension.js",
        include_str!("../../../extension/gnome/keyway@local/extension.js"),
    ),
    (
        "adapter.js",
        include_str!("../../../extension/gnome/keyway@local/adapter.js"),
    ),
    (
        "metadata.json",
        include_str!("../../../extension/gnome/keyway@local/metadata.json"),
    ),
];
const CINNAMON_EXTENSION_FILES: [(&str, &str); 3] = [
    KEYBOARD_STYLESHEET,
    (
        "extension.js",
        include_str!("../../../extension/cinnamon/keyway@local/extension.js"),
    ),
    (
        "metadata.json",
        include_str!("../../../extension/cinnamon/keyway@local/metadata.json"),
    ),
];
const AGENT_UNIT: &str = include_str!("../../../packaging/keyway.service");
const AUTOSTART_ENTRY: &str = include_str!("../../../packaging/keyway-autostart.desktop");
const SETTINGS_ENTRY: &str = include_str!("../../../packaging/keyway-settings.desktop");
const ROUTER_UNIT: &str = include_str!("../../../packaging/keyway-router.service");
const UDEV_RULE: &str = include_str!("../../../packaging/70-keyway.rules");
const LIBINPUT_QUIRK: &str = include_str!("../../../packaging/30-keyway.quirks");

/// Where the current user's files go.
#[derive(Clone, Debug)]
pub struct UserPaths {
    pub gnome_extension_directory: PathBuf,
    pub cinnamon_extension_directory: PathBuf,
    pub agent_unit: PathBuf,
    pub autostart_entry: PathBuf,
    pub settings_entry: PathBuf,
    pub labels_directory: PathBuf,
    pub turned_on_marker: PathBuf,
}

impl UserPaths {
    /// Follows the XDG variables, falling back to their defaults under HOME.
    pub fn from_environment() -> io::Result<Self> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
        let from_variable = |name: &str, fallback: &str| {
            env::var_os(name).map_or_else(|| home.join(fallback), PathBuf::from)
        };

        let data_home = from_variable("XDG_DATA_HOME", ".local/share");
        let config_home = from_variable("XDG_CONFIG_HOME", ".config");
        let state_home = from_variable("XDG_STATE_HOME", ".local/state");

        Ok(Self {
            gnome_extension_directory: data_home
                .join("gnome-shell/extensions")
                .join(EXTENSION_UUID),
            cinnamon_extension_directory: data_home
                .join("cinnamon/extensions")
                .join(EXTENSION_UUID),
            agent_unit: config_home.join("systemd/user").join(AGENT_UNIT_NAME),
            autostart_entry: config_home.join("autostart").join(AUTOSTART_ENTRY_NAME),
            settings_entry: data_home.join("applications").join(SETTINGS_ENTRY_NAME),
            labels_directory: data_home.join(LABELS_DIRECTORY),
            turned_on_marker: state_home.join(TURNED_ON_MARKER),
        })
    }
}

/// Writes through a temporary file, so nothing ever reads a half-written file.
fn write_file(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no directory"))?;
    fs::create_dir_all(directory)?;

    let temporary = path.with_extension("keyway-new");
    fs::write(&temporary, contents)?;

    fs::rename(&temporary, path)
}

fn remove_file_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Writes this user's files. The settings binary is expected beside the agent's.
pub fn write_user_files(paths: &UserPaths, agent_binary: &Path) -> io::Result<()> {
    let unit = AGENT_UNIT.replace(BINARY_PLACEHOLDER, &agent_binary.to_string_lossy());
    let settings_binary = agent_binary.with_file_name(SETTINGS_BINARY_NAME);
    let settings_entry =
        SETTINGS_ENTRY.replace(SETTINGS_PLACEHOLDER, &settings_binary.to_string_lossy());

    write_file(&paths.settings_entry, &settings_entry)?;

    write_extension(&paths.gnome_extension_directory, &GNOME_EXTENSION_FILES)?;
    write_extension(
        &paths.cinnamon_extension_directory,
        &CINNAMON_EXTENSION_FILES,
    )?;
    write_labels(&paths.labels_directory)?;

    write_file(&paths.agent_unit, &unit)
}

fn flag_path(flags: &Path, name: &str) -> PathBuf {
    flags.join(name).with_extension(FLAG_EXTENSION)
}

fn write_labels(directory: &Path) -> io::Result<()> {
    let flags = directory.join(FLAGS_DIRECTORY);

    for (name, svg) in FLAG_FILES {
        write_file(&flag_path(&flags, name), svg)?;
    }

    write_file(&flags.join(FLAGS_LICENSE_NAME), FLAGS_LICENSE)?;

    write_file(&directory.join(LABELS_FILE_NAME), LABELS_FILE)
}

/// Removes only the files setup writes, then each directory if that left it empty.
fn remove_labels(directory: &Path) -> io::Result<()> {
    let flags = directory.join(FLAGS_DIRECTORY);

    remove_file_if_present(&directory.join(LABELS_FILE_NAME))?;
    remove_file_if_present(&flags.join(FLAGS_LICENSE_NAME))?;

    for (name, _svg) in FLAG_FILES {
        remove_file_if_present(&flag_path(&flags, name))?;
    }

    remove_directory_if_empty(&flags)?;

    remove_directory_if_empty(directory)
}

fn remove_directory_if_empty(directory: &Path) -> io::Result<()> {
    match fs::remove_dir(directory) {
        Err(error)
            if !matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Err(error)
        }
        _ => Ok(()),
    }
}

fn write_extension(directory: &Path, files: &[(&str, &str)]) -> io::Result<()> {
    for (name, contents) in files {
        write_file(&directory.join(name), contents)?;
    }

    Ok(())
}

fn remove_extension(directory: &Path, files: &[(&str, &str)]) -> io::Result<()> {
    for (name, _contents) in files {
        remove_file_if_present(&directory.join(name))?;
    }

    match fs::remove_dir(directory) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

pub fn remove_user_files(paths: &UserPaths) -> io::Result<()> {
    remove_file_if_present(&paths.agent_unit)?;
    remove_file_if_present(&paths.settings_entry)?;
    remove_extension(&paths.gnome_extension_directory, &GNOME_EXTENSION_FILES)?;
    remove_extension(
        &paths.cinnamon_extension_directory,
        &CINNAMON_EXTENSION_FILES,
    )?;

    remove_labels(&paths.labels_directory)
}

/// Starts the agent with Cinnamon sessions. Separate from the files above because this user
/// opts in here even when a package owns those.
pub fn write_session_start(paths: &UserPaths) -> io::Result<()> {
    write_file(&paths.autostart_entry, AUTOSTART_ENTRY)
}

pub fn remove_session_start(paths: &UserPaths) -> io::Result<()> {
    remove_file_if_present(&paths.autostart_entry)
}

/// Stops a package's session entry for this user, which keeps Keyway off until setup runs again.
pub fn write_session_opt_out(paths: &UserPaths) -> io::Result<()> {
    write_file(&paths.autostart_entry, AUTOSTART_OPT_OUT)
}

/// Records that Keyway has been turned on here, so that a user who later turns the extension off
/// with the desktop's own tools is not overruled at the next login.
pub fn mark_turned_on(paths: &UserPaths) -> io::Result<()> {
    write_file(&paths.turned_on_marker, "")
}

pub fn clear_turned_on(paths: &UserPaths) -> io::Result<()> {
    remove_file_if_present(&paths.turned_on_marker)
}

/// System paths relative to a root directory, and what each contains.
fn system_files() -> [(&'static str, String); 3] {
    [
        ("etc/udev/rules.d/70-keyway.rules", UDEV_RULE.to_string()),
        (
            "etc/systemd/system/keyway-router.service",
            ROUTER_UNIT.replace(ROUTER_PLACEHOLDER, SYSTEM_ROUTER_BINARY),
        ),
        (
            "usr/share/libinput/30-keyway.quirks",
            LIBINPUT_QUIRK.to_string(),
        ),
    ]
}

/// True when a package owns Keyway's files, so setup must not write or remove them. Looks where
/// the desktop looks for the package's extension, so a desktop shown other data directories,
/// as in a test, gets its files from setup instead.
pub fn is_packaged() -> bool {
    let directories = env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SYSTEM_DATA_DIRS.into());

    env::split_paths(&directories).any(|directory| directory.join(PACKAGED_MARKER).exists())
}

/// Copies a binary into place by renaming, so a running service keeps its old file intact.
fn install_binary(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let directory = destination
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no directory"))?;
    fs::create_dir_all(directory)?;

    let temporary = destination.with_extension("keyway-new");
    fs::copy(source, &temporary)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(EXECUTABLE_MODE))?;

    fs::rename(&temporary, destination)
}

/// Installs the binaries and the system files under root. The binaries are expected in
/// binary_directory, which is where build.sh publishes them.
pub fn write_system_files(root: &Path, binary_directory: &Path) -> io::Result<()> {
    let installed = root.join(SYSTEM_BINARY_DIRECTORY);

    for binary in INSTALLED_BINARIES {
        install_binary(&binary_directory.join(binary), &installed.join(binary))?;
    }

    for (path, contents) in system_files() {
        write_file(&root.join(path), &contents)?;
    }

    Ok(())
}

pub fn remove_system_files(root: &Path) -> io::Result<()> {
    let installed = root.join(SYSTEM_BINARY_DIRECTORY);

    for (path, _contents) in system_files() {
        remove_file_if_present(&root.join(path))?;
    }

    for binary in INSTALLED_BINARIES {
        remove_file_if_present(&installed.join(binary))?;
    }

    Ok(())
}

/// Returns the extensions list with the UUID added, keeping existing entries and order.
/// The input and output use GVariant text, as printed and read by gsettings.
fn with_extension_enabled(list: &str) -> String {
    let quoted = format!("'{EXTENSION_UUID}'");

    if list.contains(&quoted) {
        return list.to_string();
    }

    match list.trim().strip_suffix(']') {
        Some(entries) if entries.contains('\'') => format!("{entries}, {quoted}]"),
        _ => format!("[{quoted}]"),
    }
}

/// Returns the extensions list without the UUID, keeping all other entries and order.
fn with_extension_disabled(list: &str) -> String {
    let quoted = format!("'{EXTENSION_UUID}'");
    let body = list.trim().trim_start_matches('[').trim_end_matches(']');
    let others: Vec<&str> = body
        .split(',')
        .map(str::trim)
        .filter(|entry| entry.starts_with('\'') && *entry != quoted)
        .collect();

    if others.is_empty() {
        "@as []".to_string()
    } else {
        format!("[{}]", others.join(", "))
    }
}

/// Updates a list of each desktop that is installed. A missing schema means that desktop is not,
/// so it is only an error when none of them are.
fn update_extension_list(key: &str, update: fn(&str) -> String) -> io::Result<()> {
    let mut found_a_desktop = false;

    for schema in EXTENSION_SCHEMAS {
        let output = Command::new("gsettings")
            .args(["get", schema, key])
            .output()?;

        if !output.status.success() {
            continue;
        }

        found_a_desktop = true;

        let current = String::from_utf8_lossy(&output.stdout);
        let updated = update(current.trim());

        run_command("gsettings", &["set", schema, key, &updated])?;
    }

    if found_a_desktop {
        Ok(())
    } else {
        Err(io::Error::other(
            "could not read the enabled extensions; Keyway needs GNOME Shell or Cinnamon",
        ))
    }
}

/// Without touching other extensions. Cinnamon has no list of disabled ones, nor needs it.
fn enable_extension() -> io::Result<()> {
    let _ = update_extension_list(DISABLED_EXTENSIONS_KEY, with_extension_disabled);

    update_extension_list(ENABLED_EXTENSIONS_KEY, with_extension_enabled)
}

/// Enables the extension and the agent's user service.
pub fn activate_user_setup() -> io::Result<()> {
    enable_extension()?;
    run_command("systemctl", &["--user", "daemon-reload"])?;

    run_command("systemctl", &["--user", "enable", AGENT_UNIT_NAME])
}

pub fn deactivate_user_setup() -> io::Result<()> {
    let _ = run_command(
        "systemctl",
        &["--user", "disable", "--now", AGENT_UNIT_NAME],
    );
    update_extension_list(ENABLED_EXTENSIONS_KEY, with_extension_disabled)?;

    run_command("systemctl", &["--user", "daemon-reload"])
}

/// What a package's session entry runs at each login: the first time turns Keyway on, with no
/// setup needed, and every time starts the agent. The Shell loads an enabled extension at once
/// if it was installed before this login, so the switcher is replaced from this login on.
pub fn start_for_session(paths: &UserPaths) -> io::Result<()> {
    if !paths.turned_on_marker.exists() {
        enable_extension()?;
        mark_turned_on(paths)?;
    }

    run_command("systemctl", &["--user", "daemon-reload"])?;

    run_command("systemctl", &["--user", "start", AGENT_UNIT_NAME])
}

/// Starts the router. It stays idle, with keyboards closed, until an agent configures it.
pub fn activate_system_setup() -> io::Result<()> {
    run_command("udevadm", &["control", "--reload-rules"])?;
    run_command("udevadm", &["trigger", "--name-match=uinput"])?;
    run_command("systemctl", &["daemon-reload"])?;

    run_command("systemctl", &["enable", "--now", ROUTER_UNIT_NAME])
}

pub fn deactivate_system_setup() -> io::Result<()> {
    let _ = run_command("systemctl", &["disable", "--now", ROUTER_UNIT_NAME]);
    run_command("systemctl", &["daemon-reload"])?;
    run_command("udevadm", &["control", "--reload-rules"])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!("keyway-setup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        root
    }

    fn user_paths(root: &Path) -> UserPaths {
        UserPaths {
            gnome_extension_directory: root.join("gnome").join(EXTENSION_UUID),
            cinnamon_extension_directory: root.join("cinnamon").join(EXTENSION_UUID),
            agent_unit: root.join("user/keyway.service"),
            autostart_entry: root.join("autostart/keyway.desktop"),
            settings_entry: root.join("applications/keyway-settings.desktop"),
            labels_directory: root.join("data/keyway"),
            turned_on_marker: root.join("state/keyway/turned-on"),
        }
    }

    #[test]
    fn user_files_are_written_and_removed() {
        let root = temporary_root("user");
        let paths = user_paths(&root);

        write_user_files(&paths, Path::new("/opt/keyway/bin/keyway")).unwrap();

        let unit = fs::read_to_string(&paths.agent_unit).unwrap();

        let entry = fs::read_to_string(&paths.settings_entry).unwrap();

        assert!(unit.contains("ExecStart=/opt/keyway/bin/keyway run"));
        assert!(entry.contains("Exec=/opt/keyway/bin/keyway-settings"));
        assert!(!unit.contains(BINARY_PLACEHOLDER));

        let gnome_metadata =
            fs::read_to_string(paths.gnome_extension_directory.join("metadata.json")).unwrap();
        let cinnamon_metadata =
            fs::read_to_string(paths.cinnamon_extension_directory.join("metadata.json")).unwrap();

        assert!(gnome_metadata.contains("shell-version"));
        assert!(cinnamon_metadata.contains("cinnamon-version"));
        assert!(
            paths
                .gnome_extension_directory
                .join("extension.js")
                .exists()
        );
        assert!(paths.gnome_extension_directory.join("adapter.js").exists());

        for extension in [
            &paths.gnome_extension_directory,
            &paths.cinnamon_extension_directory,
        ] {
            assert!(extension.join("keyboard.css").exists());
        }
        assert!(
            paths
                .cinnamon_extension_directory
                .join("extension.js")
                .exists()
        );

        let labels = fs::read_to_string(paths.labels_directory.join(LABELS_FILE_NAME)).unwrap();

        assert!(labels.contains(r#""br": {"code": "PT-BR", "flag": "br"}"#));
        assert!(paths.labels_directory.join("flags/br.svg").exists());

        remove_user_files(&paths).unwrap();

        assert!(!paths.agent_unit.exists());
        assert!(!paths.settings_entry.exists());
        assert!(!paths.gnome_extension_directory.exists());
        assert!(!paths.cinnamon_extension_directory.exists());
        assert!(!paths.labels_directory.exists());

        remove_user_files(&paths).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_cinnamon_session_start_is_written_and_removed() {
        let root = temporary_root("autostart");
        let paths = user_paths(&root);

        write_session_start(&paths).unwrap();

        let entry = fs::read_to_string(&paths.autostart_entry).unwrap();

        assert!(entry.contains("Exec=systemctl --user start keyway.service"));
        assert!(entry.contains("OnlyShowIn=X-Cinnamon;"));

        remove_session_start(&paths).unwrap();

        assert!(!paths.autostart_entry.exists());

        remove_session_start(&paths).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_session_opt_out_hides_a_system_entry_of_the_same_name() {
        let root = temporary_root("opt-out");
        let paths = user_paths(&root);

        write_session_opt_out(&paths).unwrap();

        let entry = fs::read_to_string(&paths.autostart_entry).unwrap();

        assert!(entry.contains("Hidden=true"));
        assert_eq!(
            paths.autostart_entry.file_name().unwrap(),
            AUTOSTART_ENTRY_NAME
        );

        remove_session_start(&paths).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_turned_on_marker_is_written_and_cleared() {
        let root = temporary_root("marker");
        let paths = user_paths(&root);

        mark_turned_on(&paths).unwrap();
        assert!(paths.turned_on_marker.exists());

        clear_turned_on(&paths).unwrap();
        assert!(!paths.turned_on_marker.exists());

        clear_turned_on(&paths).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn user_removal_keeps_files_it_did_not_install() {
        let root = temporary_root("foreign");
        let paths = user_paths(&root);
        write_user_files(&paths, Path::new("/keyway")).unwrap();

        let foreign_file = paths.gnome_extension_directory.join("notes.txt");
        fs::write(&foreign_file, "keep me").unwrap();

        assert!(remove_user_files(&paths).is_err());
        assert!(foreign_file.exists());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn system_files_are_written_under_the_root_and_removed() {
        let root = temporary_root("system");
        let binaries = root.join("bin");
        fs::create_dir_all(&binaries).unwrap();
        fs::write(binaries.join(ROUTER_BINARY_NAME), "router").unwrap();
        fs::write(binaries.join(AGENT_BINARY_NAME), "agent").unwrap();
        fs::write(binaries.join(SETTINGS_BINARY_NAME), "settings").unwrap();

        write_system_files(&root, &binaries).unwrap();

        let installed_router = root.join("usr/local/bin/keyway-router");

        assert_eq!(fs::read_to_string(&installed_router).unwrap(), "router");
        assert!(root.join("etc/udev/rules.d/70-keyway.rules").exists());
        assert!(
            root.join("etc/systemd/system/keyway-router.service")
                .exists()
        );
        assert!(root.join("usr/share/libinput/30-keyway.quirks").exists());

        let unit =
            fs::read_to_string(root.join("etc/systemd/system/keyway-router.service")).unwrap();

        assert!(unit.contains("ExecStart=/usr/local/bin/keyway-router"));

        remove_system_files(&root).unwrap();

        assert!(!installed_router.exists());
        assert!(!root.join("etc/udev/rules.d/70-keyway.rules").exists());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_missing_router_binary_stops_the_system_install() {
        let root = temporary_root("missing");

        assert!(write_system_files(&root, &root.join("nowhere")).is_err());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn enabling_keeps_the_users_other_extensions() {
        assert_eq!(
            with_extension_enabled("['snappy@snappy.local', 'magic-hotspot@local']"),
            "['snappy@snappy.local', 'magic-hotspot@local', 'keyway@local']"
        );
        assert_eq!(with_extension_enabled("@as []"), "['keyway@local']");
        assert_eq!(
            with_extension_enabled("['keyway@local']"),
            "['keyway@local']"
        );
    }

    #[test]
    fn disabling_keeps_the_users_other_extensions() {
        assert_eq!(
            with_extension_disabled(
                "['snappy@snappy.local', 'keyway@local', 'magic-hotspot@local']"
            ),
            "['snappy@snappy.local', 'magic-hotspot@local']"
        );
        assert_eq!(with_extension_disabled("['keyway@local']"), "@as []");
        assert_eq!(with_extension_disabled("@as []"), "@as []");
    }
}
