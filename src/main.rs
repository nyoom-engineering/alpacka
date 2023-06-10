#![allow(clippy::multiple_crate_versions)]

use alpacka::cli::{
    clap::{Cli, ListGenerationsFormatMethod},
    install::install,
    list_generations::list_generations,
};

use clap::Parser;
use error_stack::{Context, Report, ResultExt};

use std::fmt::{Display, Formatter};
use tracing_subscriber::{fmt::format::PrettyFields, prelude::*};

#[derive(Debug)]
struct MainError;

impl Display for MainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to run alpacka!")
    }
}

impl Context for MainError {}

fn main() -> error_stack::Result<(), MainError> {
    Report::set_color_mode(error_stack::fmt::ColorMode::Color);

    let error = tracing_error::ErrorLayer::new(PrettyFields::new());

    // Setup logging, with pretty printing
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::Level::INFO.into())
                .from_env_lossy(),
        )
        .finish()
        .with(error)
        .init();

    match Cli::parse() {
        Cli::Install { path, data_dir } => cli_install(path, data_dir),
        Cli::ListGenerations {
            data_dir,
            format_style,
        } => cli_list_generations(data_dir, format_style),
    }?;

    Ok(())
}

fn cli_list_generations(
    data_dir: Option<std::path::PathBuf>,
    format_style: Option<ListGenerationsFormatMethod>,
) -> Result<(), Report<MainError>> {
    let data_path = data_dir.unwrap_or_else(|| {
        let data_dir = std::env::var_os("XDG_DATA_HOME")
            .and_then(dirs_sys::is_absolute_path)
            .or_else(|| dirs_sys::home_dir().map(|h| h.join(".local/share")));

        data_dir
            .map(|dd| dd.join("nvim/site/pack/alpacka/"))
            .unwrap()
    });

    list_generations(
        &data_path,
        &format_style.unwrap_or(ListGenerationsFormatMethod::Human),
    )
    .change_context(MainError)
}

fn cli_install(
    path: Option<std::path::PathBuf>,
    data_dir: Option<std::path::PathBuf>,
) -> Result<(), Report<MainError>> {
    let config_path = path.unwrap_or_else(|| {
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .and_then(dirs_sys::is_absolute_path)
            .or_else(|| dirs_sys::home_dir().map(|h| h.join(".config")));

        config_dir.map(|cd| cd.join("nvim/packages.json")).unwrap()
    });

    let data_path = data_dir.unwrap_or_else(|| {
        let data_dir = std::env::var_os("XDG_DATA_HOME")
            .and_then(dirs_sys::is_absolute_path)
            .or_else(|| dirs_sys::home_dir().map(|h| h.join(".local/share")));

        data_dir
            .map(|dd| dd.join("nvim/site/pack/alpacka/"))
            .unwrap()
    });

    install(config_path, &data_path).change_context(MainError)
}
