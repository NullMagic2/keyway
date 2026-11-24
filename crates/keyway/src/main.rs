//! Module: keyway::main
//! Purpose: Command line of the per-user agent: run it, switch layouts, set up the system.
//! Created: 2026-11-08
//! Architecture: Thin dispatch to run.rs, setup.rs and the desktop client.

mod agent;
mod run;
mod setup;

use agent::neighbour_in_list;
use keyway_core::layout_id::LayoutId;
use keyway_core::protocol::DEFAULT_SOCKET_PATH;
use keyway_desktop::config_file::{CONFIG_FILE_NAME, config_directory, load_config};
use keyway_desktop::desktop_shell::DesktopShell;
use keyway_desktop::restart::{self, Part};
use setup::UserPaths;
use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const EXIT_USAGE: u8 = 2;
const USAGE: &str = "usage: keyway COMMAND

  run [--socket PATH]       Run the agent
  status                    List layouts and show the active one
  next                      Switch to the next layout without its own shortcut
  previous                  Switch to the previous layout without its own shortcut
  select ID                 Switch to a layout, such as gr+simple
  setup [--no-activate]     Install for this user (extension and agent service)
  setup --system            As root: install the router service and system files
  session-start             Turn on for this login; a package's session entry runs this
  restart                   Restart Keyway in this session, as after an upgrade
  remove                    Undo setup, and keep Keyway off, for this user
  remove --system           As root: undo the system setup";

const SYSTEM_ROOT: &str = "/";

type CommandResult = Result<(), Box<dyn Error>>;

fn print_status() -> CommandResult {
    let snapshot = DesktopShell::connect()?.snapshot()?;

    for source in &snapshot.sources {
        let marker = if source.id == snapshot.active {
            '*'
        } else {
            ' '
        };
        println!("{marker} {}\t{}", source.id, source.display_name);
    }

    Ok(())
}

/// Follows the list as the switching shortcuts do, so it too passes over a layout with its
/// own shortcut.
fn switch_layout(forward: bool) -> CommandResult {
    let switching = load_config(&config_directory()?.join(CONFIG_FILE_NAME))?.switching;
    let shell = DesktopShell::connect()?;
    let snapshot = shell.snapshot()?;
    let target = neighbour_in_list(&snapshot.sources, &snapshot.active, forward, |id| {
        switching.cycles_through(id)
    });

    if let Some(target) = target {
        shell.activate(&target)?;
    }

    Ok(())
}

fn select_layout(id: String) -> CommandResult {
    let id = LayoutId::try_from(id)?;

    Ok(DesktopShell::connect()?.activate(&id)?)
}

fn binary_directory() -> Result<PathBuf, Box<dyn Error>> {
    let binary = env::current_exe()?;
    let directory = binary.parent().ok_or("this binary has no directory")?;

    Ok(directory.to_path_buf())
}

const PACKAGED_MESSAGE: &str =
    "Keyway was installed by a package; use apt to change the system files.";

fn setup_user(activate: bool) -> CommandResult {
    let paths = UserPaths::from_environment()?;
    let packaged = setup::is_packaged();

    if !packaged {
        setup::write_user_files(&paths, &env::current_exe()?)?;
    }

    if activate {
        // The package's own session entry takes over once this user's is gone.
        if packaged {
            setup::remove_session_start(&paths)?;
            setup::mark_turned_on(&paths)?;
        } else {
            setup::write_session_start(&paths)?;
        }

        setup::activate_user_setup()?;
        println!("Installed. If your desktop does not show Keyway yet, run: keyway restart");
    } else {
        println!("Files written; nothing was activated.");
    }

    Ok(())
}

fn setup_system() -> CommandResult {
    if setup::is_packaged() {
        return Err(PACKAGED_MESSAGE.into());
    }

    setup::write_system_files(Path::new(SYSTEM_ROOT), &binary_directory()?)?;
    setup::activate_system_setup()?;
    println!("Router installed. It stays idle until an agent connects.");

    Ok(())
}

fn start_for_session() -> CommandResult {
    Ok(setup::start_for_session(&UserPaths::from_environment()?)?)
}

const LOOKING_GLASS_ADVICE: &str =
    "To load the extension into the running session: press Alt+F2, type lg and press Enter,
paste the line below at the prompt, press Enter, then Esc.";

fn part_name(part: Part) -> &'static str {
    match part {
        Part::Extension => "Desktop extension",
        Part::Agent => "Agent",
        Part::Router => "Router",
    }
}

fn restart() -> CommandResult {
    let report = restart::restart();

    for (part, failure) in &report.parts {
        let name = part_name(*part);

        match failure {
            None => println!("{name} restarted"),
            Some(error) => eprintln!("keyway: {name} not restarted: {error}"),
        }
    }

    if report.needs_looking_glass {
        eprintln!(
            "\n{LOOKING_GLASS_ADVICE}\n\n{}\n",
            restart::looking_glass_line()
        );
    }

    if !report.succeeded() {
        return Err("not everything was restarted".into());
    }

    Ok(())
}

fn remove_user() -> CommandResult {
    let paths = UserPaths::from_environment()?;

    setup::deactivate_user_setup()?;

    if setup::is_packaged() {
        setup::write_session_opt_out(&paths)?;
        setup::clear_turned_on(&paths)?;
    } else {
        setup::remove_session_start(&paths)?;
        setup::remove_user_files(&paths)?;
    }

    println!("Removed for this user. The desktop's own switcher is back.");

    Ok(())
}

fn remove_system() -> CommandResult {
    if setup::is_packaged() {
        return Err(PACKAGED_MESSAGE.into());
    }

    setup::deactivate_system_setup()?;
    setup::remove_system_files(Path::new(SYSTEM_ROOT))?;
    println!("Router removed.");

    Ok(())
}

fn run_with_options(options: &[String]) -> Option<CommandResult> {
    match options {
        [] => Some(run::run_agent(PathBuf::from(DEFAULT_SOCKET_PATH))),
        [flag, path] if flag == "--socket" => Some(run::run_agent(PathBuf::from(path))),
        _ => None,
    }
}

/// Runs a command, or returns None when the arguments are not a valid command.
fn dispatch(arguments: &[String]) -> Option<CommandResult> {
    let (command, options) = arguments.split_first()?;
    let options_text: Vec<&str> = options.iter().map(String::as_str).collect();

    match (command.as_str(), options_text.as_slice()) {
        ("run", _) => run_with_options(options),
        ("status", []) => Some(print_status()),
        ("next", []) => Some(switch_layout(true)),
        ("previous", []) => Some(switch_layout(false)),
        ("select", [id]) => Some(select_layout((*id).to_string())),
        ("setup", []) => Some(setup_user(true)),
        ("setup", ["--no-activate"]) => Some(setup_user(false)),
        ("setup", ["--system"]) => Some(setup_system()),
        ("session-start", []) => Some(start_for_session()),
        ("restart", []) => Some(restart()),
        ("remove", []) => Some(remove_user()),
        ("remove", ["--system"]) => Some(remove_system()),
        _ => None,
    }
}

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();

    match dispatch(&arguments) {
        Some(Ok(())) => ExitCode::SUCCESS,
        Some(Err(error)) => {
            eprintln!("keyway: {error}");

            ExitCode::FAILURE
        }
        None => {
            eprintln!("{USAGE}");

            ExitCode::from(EXIT_USAGE)
        }
    }
}
