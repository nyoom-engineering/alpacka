//! WIP impl of alpacka

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    env,
    fmt::{Display, Formatter},
};

use alpacka::loader;
use error_stack::{Context, ResultExt};
use tracing::{debug, info};

#[derive(Debug)]
struct MainError;

impl Display for MainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to run alpacka!")
    }
}

impl Context for MainError {}

fn main() -> error_stack::Result<(), MainError> {
    tracing_subscriber::fmt::init();

    // let config_dir = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| "~/.config/".into());
    // let config_path = Path::new(&config_dir).join("nvim/packages.json");
    // println!("config-dir {}", config_dir);
    // println!("config-path {}", config_path.display());
    let config_path = "packages.json";
    debug!("config-path {}", config_path);
    let (generation, config) = loader::read(&config_path).change_context(MainError)?;
    debug!("config {:#?} generation {}", config, generation);

    let data_path = env::current_dir().unwrap().join("pack");

    info!("Installing packages...");
    config
        .install_all_packages(&data_path)
        .change_context(MainError)?;
    info!("Installed all packages!");

    Ok(())
}
