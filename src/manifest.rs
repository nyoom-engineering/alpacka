use bytecheck::CheckBytes;
use rkyv::{to_bytes, Archive};
use rkyv_typename::TypeName;
use std::{collections::BTreeMap, io::Write, path::PathBuf};

use crate::{rkyv::StringPathBuf, smith::SerializeLoaderInput};

#[derive(Archive, rkyv::Serialize, rkyv::Deserialize, Debug, PartialEq, Eq)]
#[archive_attr(derive(Debug, CheckBytes))]
pub struct Generation {
    pub generation: u64,
    pub path: StringPathBuf,
}

impl PartialOrd for Generation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Generation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.generation.cmp(&other.generation)
    }
}

/// A file which contains a list of all the generations
/// The key is the config hash and the generation number
/// The value is the path to the generation
#[derive(Archive, rkyv::Serialize, rkyv::Deserialize, Debug)]
#[archive_attr(derive(TypeName, Debug, CheckBytes))]
pub struct GenerationsFile(pub BTreeMap<u64, Generation>);

impl GenerationsFile {
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn add_to_generation(&mut self, config_hash: u64, manifest_path: PathBuf) {
        let generation = self
            .0
            .iter()
            .filter(|(hash, _)| **hash == config_hash)
            .map(|(_, generation)| generation.generation)
            .max()
            .unwrap_or(0)
            + 1;

        self.0.insert(
            config_hash,
            Generation {
                generation,
                path: StringPathBuf::new(manifest_path),
            },
        );
    }

    #[must_use]
    pub fn get_latest_generation(&self, config_hash: u64) -> Option<&Generation> {
        self.0
            .iter()
            .filter(|(hash, _)| **hash == config_hash)
            .map(|(_, gen)| gen)
            .max()
    }

    #[must_use]
    pub fn get_latest_generation_number(&self, config_hash: u64) -> Option<u64> {
        self.0
            .iter()
            .filter(|(hash, _)| **hash == config_hash)
            .map(|(_, gen)| gen.generation)
            .max()
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
        let bytes = to_bytes::<_, 1024>(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let mut writer = std::io::BufWriter::new(file);
        writer
            .write_all(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        Ok(())
    }
}

impl Default for GenerationsFile {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive_attr(derive(CheckBytes))]
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
        let bytes = to_bytes::<_, 1024>(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let mut writer = std::io::BufWriter::new(file);

        writer
            .write_all(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        Ok(())
    }
}

#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive_attr(derive(CheckBytes))]
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
    pub loader_data: Box<dyn SerializeLoaderInput>,
}
