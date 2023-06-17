#![allow(clippy::multiple_crate_versions)]

mod cli;

use cli::{
    clap::{Cli, ListGenerationsFormatMethod},
    install::install,
    list_generations::list_generations,
};

use clap::Parser;
use error_stack::{Context, Report, ResultExt};

use std::{
    fmt::{Display, Formatter},
    path::PathBuf,
};
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
    #[cfg(feature = "vendor")]
    {
        openssl_probe::init_ssl_cert_env_vars();
    }

    Report::set_color_mode(error_stack::fmt::ColorMode::Color);

    let error_handler = tracing_error::ErrorLayer::new(PrettyFields::new());

    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::Level::INFO.into())
                .from_env_lossy(),
        )
        .finish()
        .with(error_handler)
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
    data_dir: Option<PathBuf>,
    format_style: Option<ListGenerationsFormatMethod>,
) -> Result<(), Report<MainError>> {
    let data_path = get_data_path(data_dir);

    list_generations(
        &data_path,
        &format_style.unwrap_or(ListGenerationsFormatMethod::Human),
    )
    .change_context(MainError)
}

fn cli_install(path: Option<PathBuf>, data_dir: Option<PathBuf>) -> Result<(), Report<MainError>> {
    let data_path = get_data_path(data_dir);

    let config_path = path.unwrap_or_else(|| {
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .and_then(dirs_sys::is_absolute_path)
            .or_else(|| dirs_sys::home_dir().map(|h| h.join(".config")));

        config_dir.map(|cd| cd.join("nvim/packages.json")).unwrap()
    });

    install(config_path, &data_path).change_context(MainError)
}

fn get_data_path(data_dir: Option<PathBuf>) -> PathBuf {
    data_dir.unwrap_or_else(|| {
        let data_dir = std::env::var_os("XDG_DATA_HOME")
            .and_then(dirs_sys::is_absolute_path)
            .or_else(|| dirs_sys::home_dir().map(|h| h.join(".local/share")));

        data_dir
            .map(|dd| dd.join("nvim/site/pack/alpacka/"))
            .unwrap()
    })
}
