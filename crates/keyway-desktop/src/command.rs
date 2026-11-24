//! Module: keyway_desktop::command
//! Purpose: Run a system tool, such as systemctl, and report its failure as an error.
//! Created: 2026-11-09
//! Architecture: The tool keeps this process's terminal, so it can ask its own questions there.

use std::io;
use std::process::Command;

pub fn run_command(program: &str, arguments: &[&str]) -> io::Result<()> {
    let status = Command::new(program).args(arguments).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{program} {} failed: {status}",
            arguments.join(" ")
        )))
    }
}
