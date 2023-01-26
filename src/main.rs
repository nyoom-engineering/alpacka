//! WIP impl of alpacka

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use alpacka::loader;
use error_stack::{Context, ResultExt};
use std::fmt::{Display, Formatter};
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

    //    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
    //        .and_then(dirs_sys::is_absolute_path)
    //        .or_else(|| dirs_sys::home_dir().map(|h| h.join(".config")));
    //    let data_dir = std::env::var_os("XDG_DATA_HOME")
    //        .and_then(dirs_sys::is_absolute_path)
    //        .or_else(|| dirs_sys::home_dir().map(|h| h.join(".local/share")));
    //    let cache_dir = std::env::var_os("XDG_CACHE_HOME")
    //        .and_then(dirs_sys::is_absolute_path)
    //        .or_else(|| dirs_sys::home_dir().map(|h| h.join(".cache")));
    //
    //    let alpacka_config = Path::new(&config_dir).join("nvim/packages.json");
    //    let alpacka_data = Path::new(&data_dir).join("nvim/site/pack/alpacka");
    //    let alpacka_cache = Path::new(&cache_dir).join("nvim");
    //
    //    println!("config-path {}", config_dir);
    //    println!("data-path {}", data_dir);
    //    println!("cache-path {}", cache_dir);

    let config_path = "packages.json";
    let data_path = std::env::current_dir().unwrap().join("pack");

    debug!("config-path {}", config_path);
    let (generation, config) = loader::read(&config_path).change_context(MainError)?;
    debug!("config {:#?} generation {}", config, generation);

    info!("Installing packages...");
    config
        .install_all_packages(&data_path)
        .change_context(MainError)?;
    info!("Installed all packages!");

    Ok(())
}
