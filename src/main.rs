#![warn(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]

use anyhow::Context;
use std::{collections::HashMap, io::Write, path::Path, sync::mpsc};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

mod git;
mod loader;
mod semver;

#[macro_use]
extern crate serde;

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
    // let config_dir = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| "~/.config/".into());
    // let config_path = Path::new(&config_dir).join("nvim/packages.json");
    // println!("config-dir {}", config_dir);
    // println!("config-path {}", config_path.display());
    let config_path = "packages.json";
    println!("config-path {}", config_path);
    let config = loader::read(&config_path).context("failed to read config file")?;

    // let data_dir = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| "~/.local/share/".into());
    // let data_path = Path::new(&data_dir).join("nvim/site/pack/nyoom");
    // Pretty print installing packages
    // Get list of already installed packages
    // Parse packages file & create generation
    // Print current generation
    // Check given list against currently installed packages
    // Install packages that need installing
    // Update packages that need updating
    // Remove packages that need removing
    Ok(())
}
