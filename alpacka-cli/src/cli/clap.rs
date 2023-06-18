use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// Alpacka: the next-generation package manager for Neovim.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub enum Cli {
    Install {
        /// The path to the config file
        /// Defaults to `$XDG_CONFIG_HOME/nvim/packages.json`
        #[arg(short, long)]
        path: Option<PathBuf>,
        /// The data directory
        /// Defaults to `$XDG_DATA_HOME/nvim/site/pack/alpacka`
        #[arg(short, long)]
        data_dir: Option<PathBuf>,
    },
    ListGenerations {
        /// The data directory containing the generations.rkyv file
        /// Defaults to `$XDG_DATA_HOME/nvim/site/pack/alpacka`
        #[arg(short, long)]
        data_dir: Option<PathBuf>,
        /// The output format
        /// Defaults to `ListGenerationsFormatMethod::Human`
        #[arg(short, long)]
        format_style: Option<ListGenerationsFormatMethod>,
    },
}

#[derive(Debug, ValueEnum, Clone)]
pub enum ListGenerationsFormatMethod {
    /// Human-readable output
    Human,
    /// JSON output, to be parsed by another program
    Json,
}
