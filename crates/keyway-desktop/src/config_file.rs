//! Module: keyway_desktop::config_file
//! Purpose: Find, read and atomically save the user's configuration file.
//! Created: 2026-11-08
//! Architecture: The agent reloads this file when it changes; the settings window writes it.

use keyway_core::config::{Config, MAX_CONFIG_BYTES};
use std::env;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const CONFIG_DIRECTORY_NAME: &str = "keyway";
const TEMPORARY_EXTENSION: &str = "toml.new";
const INVALID_BACKUP_EXTENSION: &str = "toml.invalid";

pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Returns $XDG_CONFIG_HOME/keyway, falling back to ~/.config/keyway.
pub fn config_directory() -> io::Result<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));

    base.map(|directory| directory.join(CONFIG_DIRECTORY_NAME))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

/// Reads the configuration; a missing file means the defaults.
pub fn load_config(path: &Path) -> Result<Config, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => return Err(error.to_string()),
    };

    // One byte over the limit is enough for the parser to reject the file as too large.
    let mut text = String::new();
    file.take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;

    Config::parse(&text).map_err(|error| error.to_string())
}

/// Saves by renaming a finished temporary file, so the agent never reads half of one.
pub fn save_config(path: &Path, config: &Config) -> io::Result<()> {
    let text = config.to_toml().map_err(io::Error::other)?;
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no directory"))?;
    fs::create_dir_all(directory)?;

    let temporary = path.with_extension(TEMPORARY_EXTENSION);
    fs::write(&temporary, text)?;

    fs::rename(&temporary, path)
}

/// Sets aside a file that does not parse, so saving over it cannot lose the user's work.
/// Returns where it went.
pub fn back_up_invalid_config(path: &Path) -> io::Result<PathBuf> {
    let backup = path.with_extension(INVALID_BACKUP_EXTENSION);
    fs::rename(path, &backup)?;

    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyway_core::hotkey::Chord;

    fn temporary_directory(name: &str) -> PathBuf {
        let directory =
            env::temp_dir().join(format!("keyway-config-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();

        directory
    }

    #[test]
    fn a_missing_file_means_the_defaults() {
        let directory = temporary_directory("missing");

        assert_eq!(
            load_config(&directory.join(CONFIG_FILE_NAME)),
            Ok(Config::default())
        );

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_saved_configuration_loads_back_and_leaves_no_temporary_file() {
        let directory = temporary_directory("saved");
        let path = directory.join("nested").join(CONFIG_FILE_NAME);
        let mut config = Config::default();
        config.switching.next = Chord::parse("Ctrl+Shift").unwrap();

        save_config(&path, &config).unwrap();

        assert_eq!(load_config(&path), Ok(config));
        assert!(!path.with_extension(TEMPORARY_EXTENSION).exists());

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn an_invalid_file_is_reported_and_can_be_set_aside() {
        let directory = temporary_directory("invalid");
        let path = directory.join(CONFIG_FILE_NAME);
        fs::write(&path, "not valid").unwrap();

        assert!(load_config(&path).is_err());

        let backup = back_up_invalid_config(&path).unwrap();

        assert_eq!(fs::read_to_string(&backup).unwrap(), "not valid");
        assert!(!path.exists());

        fs::remove_dir_all(&directory).unwrap();
    }
}
