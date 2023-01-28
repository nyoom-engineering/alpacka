use std::{collections::BTreeMap, path::PathBuf};

use crate::smith::LoaderInput;
use serde::{Deserialize, Serialize};
use serde_cbor::to_writer;

/// A file which contains a list of all the generations
/// The key is the config hash and the generation number
/// The value is the path to the generation
pub struct GenerationsFile(pub BTreeMap<(u64, u64), PathBuf>);

impl GenerationsFile {
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn add_to_generation(&mut self, config_hash: u64, manifest_path: PathBuf) {
        let generation = self
            .0
            .iter()
            .filter(|((hash, _), _)| *hash == config_hash)
            .map(|((_, gen), _)| gen)
            .max()
            .unwrap_or(&0)
            + 1;

        self.0.insert((config_hash, generation), manifest_path);
    }

    #[must_use]
    pub fn get_latest_generation(&self, config_hash: u64) -> Option<&PathBuf> {
        self.0
            .iter()
            .filter(|((hash, _), _)| *hash == config_hash)
            .map(|((_, _), path)| path)
            .max()
    }

    #[must_use]
    pub fn get_latest_generation_number(&self, config_hash: u64) -> Option<u64> {
        self.0
            .iter()
            .filter(|((hash, _), _)| *hash == config_hash)
            .map(|((_, gen), _)| gen)
            .max()
            .copied()
    }

    #[must_use]
    pub fn get_next_generation_number(&self, config_hash: u64) -> u64 {
        self.get_latest_generation_number(config_hash).unwrap_or(0) + 1
    }

    /// Save the generations file to a file
    ///
    /// # Errors
    /// This function will return an error if the file can't be created, or if the generations can't be serialized
    pub fn save_to_file(&self, generation_path: &PathBuf) -> Result<(), std::io::Error> {
        let file = std::fs::File::create(generation_path)?;
        let writer = std::io::BufWriter::new(file);
        to_writer(writer, &self.0)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        Ok(())
    }
}

impl Default for GenerationsFile {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
    /// The neovim version this manifest was built for
    pub neovim_version: String,
    pub plugins: Vec<Plugin>,
}

impl Manifest {
    #[must_use]
    pub fn new(neovim_version: String, plugins: Vec<Plugin>) -> Self {
        Self {
            neovim_version,
            plugins,
        }
    }

    /// Save the manifest to a file
    ///
    /// # Errors
    /// This function will return an error if the file can't be created, or if the manifest can't be serialized
    pub fn save_to_file(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        to_writer(writer, self).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Plugin {
    /// The plugin's name
    pub name: String,
    /// Rename the plugin to this name when loading
    pub rename: Option<String>,
    /// If the plugin is optional
    pub optional: bool,
    /// The plugin's dependencies, as a list of plugin names
    pub dependencies: Vec<String>,
    /// the name of the loader this plugin is loaded by
    pub smith: String,
    /// A command which is run in the plugin's directory after loading
    pub build: String,
    /// The data which is used for the loader
    pub loader_data: Box<dyn LoaderInput>,
}
