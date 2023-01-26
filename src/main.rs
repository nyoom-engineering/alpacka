//! WIP impl of alpacka

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use alpacka::config::Config;
use error_stack::{Context, IntoReport, ResultExt};
use std::fmt::{Display, Formatter};
use tracing::info;

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
    let data_path = std::env::current_dir()
        .into_report()
        .attach_printable_lazy(|| "Failed to get current directory. Current directory")
        .change_context(MainError)?
        .join("pack");

    let config_file = std::fs::File::open(config_path)
        .into_report()
        .attach_printable_lazy(|| {
            format!(
                "Failed to open config file. Config file path: {}",
                config_path
            )
        })
        .change_context(MainError)?;

    let config: Config = serde_json::from_reader(config_file)
        .into_report()
        .attach_printable_lazy(|| {
            format!(
                "Failed to parse config file. Config file path: {}",
                config_path
            )
        })
        .change_context(MainError)?;

    info!("config loaded, checking for existing manifest");

    todo!("check for existing manifest");

    Ok(())
}
