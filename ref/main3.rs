#![warn(clippy::pedantic)]

use anyhow::Context;
use installer::Installer;
use std::{collections::HashMap, io::Write, path::PathBuf, sync::mpsc};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

#[macro_use]
extern crate serde;
#[macro_use]
extern crate clap;

mod config;
mod installer;

pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug)]
pub enum Message {
    Close,
    StateEvent(StateEvent),
}

#[derive(Debug)]
pub struct StateEvent {
    pub name: String,
    pub kind: StateEventKind,
}

impl StateEvent {
    #[must_use]
    pub fn new(name: &str, kind: StateEventKind) -> Self {
        Self {
            name: name.to_string(),
            kind,
        }
    }
}

#[derive(Debug)]
pub enum StateEventKind {
    Installing,
    Installed,
    Updating,
    Updated,
    UpToDate,
    Removed,
    Failed(anyhow::Error),
}

impl std::fmt::Display for StateEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Self::Installing => "  Installing".to_string(),
                Self::Installed => "   Installed".to_string(),
                Self::Updating => "    Updating".to_string(),
                Self::Updated => "     Updated".to_string(),
                Self::UpToDate => "  Up to date".to_string(),
                Self::Removed => "     Removed".to_string(),
                Self::Failed(e) => format!("Error occured: {:?}", e),
            }
        )
    }
}

fn main() -> anyhow::Result<()> {
    let matches = command!()
        .propagate_version(true)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .args(&[arg!(
            -c --config <CONFIG_FILE> "path to configuration file [default: $HOME/.config/nyoom/pkg.yml]"
        )
        .allow_invalid_utf8(true)
        .required(false)])
        .subcommands(vec![
            clap::Command::new("install")
                .about("installs all new packages")
                .visible_alias("i")
                .args(&[arg!(-u --upgrade "upgrade existing packages").required(false)]),
            clap::Command::new("upgrade")
                .about("updates all existing packages")
                .visible_alias("u"),
            clap::Command::new("clean")
                .about("removes all unused packages")
                .visible_alias("c"),
        ])
        .get_matches();

    let config_path = matches.value_of_os("config").map_or_else(
        || home::home_dir().unwrap().join(".config/nyoom/pkg.yml"),
        PathBuf::from,
    );

    let config = config::read(&config_path).context("failed to read config file")?;

    let (s, r) = mpsc::sync_channel::<Message>(8);

    std::thread::spawn(move || {
        let mut installer = Installer::new(config, s);
        if let Some(install) = matches.subcommand_matches("install") {
            let upgrade = install.is_present("upgrade");
            installer.set_upgrade_during_install(upgrade);
            installer.all_repos(Installer::clone_repo)?;
        }
        if let Some(_upgrade) = matches.subcommand_matches("upgrade") {
            installer.all_repos(Installer::pull_repo)?;
        }
        if let Some(_upgrade) = matches.subcommand_matches("clean") {
            installer.remove_unused()?;
        }

        Ok::<(), anyhow::Error>(())
    });

    let clear_screen = || print!("\x1B[2J\x1B[1;1H");

    let mut state: HashMap<String, StateEventKind> = HashMap::new();

    let print_state = |mut stdout: &mut StandardStream,
                       map: &HashMap<String, StateEventKind>|
     -> anyhow::Result<()> {
        clear_screen();
        for (pkg, state) in map {
            match state {
                StateEventKind::Updating | StateEventKind::Installing => {
                    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Blue)))?;
                }
                StateEventKind::Installed | StateEventKind::Updated | StateEventKind::UpToDate => {
                    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
                }
                StateEventKind::Removed => {
                    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
                }
                StateEventKind::Failed(_) => {
                    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
                }
            };
            write!(&mut stdout, "{}", state)?;
            stdout.set_color(ColorSpec::new().set_fg(None))?;
            writeln!(&mut stdout, " {}", pkg)?;
        }
        Ok(())
    };
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    loop {
        let event = r.recv()?;

        match event {
            Message::Close => break,
            Message::StateEvent(event) => state.insert(event.name, event.kind),
        };

        print_state(&mut stdout, &state)?;
    }

    Ok(())
}
