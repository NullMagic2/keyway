//! Module: keyway_router::main
//! Purpose: Parse the command line and run the router until it is told to stop.
//! Created: 2026-11-08
//! Architecture: Thin entry point for the service in packaging/keyway-router.service.

use keyway_core::protocol::DEFAULT_SOCKET_PATH;
use keyway_router::router::{Router, RouterOptions};
use std::path::PathBuf;
use std::process::ExitCode;

const EXIT_USAGE: u8 = 2;
const USAGE: &str = "usage: keyway-router [--socket PATH] [--only-device NAME]... [--trust-uid UID]

  --socket PATH       Agent socket path
  --only-device NAME  Grab only keyboards with this exact name (for testing)
  --trust-uid UID     Accept this user instead of asking logind (for testing)";

fn parse_options(mut arguments: impl Iterator<Item = String>) -> Result<RouterOptions, String> {
    let mut options = RouterOptions {
        socket_path: PathBuf::from(DEFAULT_SOCKET_PATH),
        only_devices: Vec::new(),
        trusted_uid: None,
    };

    while let Some(option) = arguments.next() {
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| format!("{option} needs a value"))
        };

        match option.as_str() {
            "--socket" => options.socket_path = PathBuf::from(value()?),
            "--only-device" => options.only_devices.push(value()?),
            "--trust-uid" => {
                let uid = value()?;
                let parsed = uid.parse().map_err(|_| format!("invalid uid: {uid}"))?;
                options.trusted_uid = Some(parsed);
            }
            _ => return Err(format!("unknown option: {option}")),
        }
    }

    Ok(options)
}

fn main() -> ExitCode {
    let options = match parse_options(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");

            return ExitCode::from(EXIT_USAGE);
        }
    };

    let result = Router::new(options).and_then(Router::run);

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("keyway-router: {error}");

            ExitCode::FAILURE
        }
    }
}
