//! WIP impl of alpacka

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    fmt::{Display, Formatter},
    path::Path,
};

use alpacka::loader;
use error_stack::{Context, ResultExt};
use tracing::debug;

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
}

impl std::fmt::Display for StateEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Self::Installing => "Installing",
                Self::Installed => "Installed",
                Self::Updating => "Updating",
                Self::Updated => "Updated",
                Self::UpToDate => "Up to date",
                Self::Removed => "Removed",
            }
        )
    }
}

#[derive(Debug)]
struct MainError;

impl Display for MainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to run alpacka!")
    }
}

impl Context for MainError {}

fn main() {
    tracing_subscriber::fmt::init();

    // let config_dir = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| "~/.config/".into());
    // let config_path = Path::new(&config_dir).join("nvim/packages.json");
    // println!("config-dir {}", config_dir);
    // println!("config-path {}", config_path.display());
    let config_path = "packages.json";
    debug!("config-path {}", config_path);
    let config = loader::read(&config_path).change_context(MainError);
    debug!("config {:#?}", config);

    let data_dir = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| "~/.local/share/".into());
    let data_path = Path::new(&data_dir).join("nvim/site/pack");
    // Pretty print installing packages
    // Get list of already installed packages
    // Parse packages file & create generation
    // Print current generation
    // Check given list against currently installed packages
    // Install packages that need installing
    // Update packages that need updating
    // Remove packages that need removing
}
