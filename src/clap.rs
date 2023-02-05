use std::path::PathBuf;

use clap::Parser;

/// Alpacka: the next-generation package manager for neovim.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub enum Cli {
    Install {
        /// The path to the config file
        /// Defaults to `$XDG_CONFIG_HOME/nvim/packages.jsob`
        path: Option<PathBuf>,
        /// The data directory
        /// Defaults to `$XDG_DATA_HOME/nvim/site/pack/alpacka`
        data_dir: Option<PathBuf>,
    },
}
