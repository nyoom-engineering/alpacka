use bytecheck::CheckBytes;
use rkyv::{to_bytes, Archive, Deserialize, Infallible};
use rkyv_typename::TypeName;
use std::{collections::BTreeMap, io::Write, path::PathBuf};

use crate::smith::SerializeLoaderInput;

#[derive(Archive, rkyv::Serialize, rkyv::Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[archive_attr(derive(TypeName, CheckBytes, Eq, PartialEq, PartialOrd, Ord))]
/// A hash of the config file
///
/// The first value is the hash of the config file
/// The second value is the generation number
pub struct GenerationHash(pub u64, pub u64);

/// A file which contains a list of all the generations
/// The key is the config hash and the generation number
/// The value is the path to the generation
#[derive(Archive, rkyv::Serialize, rkyv::Deserialize, Debug)]
#[archive_attr(derive(TypeName, CheckBytes))]
pub struct GenerationsFile(pub BTreeMap<GenerationHash, Manifest>);

impl GenerationsFile {
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
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

    pub fn add_to_generations(&mut self, config_hash: u64, manifest: Manifest) {
        let generation_number = self
            .0
            .keys()
            .filter(|GenerationHash(hash, _)| *hash == config_hash)
            .max_by_key(|GenerationHash(_, generation)| generation)
            .map(|GenerationHash(_, generation)| generation)
            .copied()
            .unwrap_or(0)
            + 1;

        self.0
            .insert(GenerationHash(config_hash, generation_number), manifest);
    }

    pub fn get_latest_manifest(&self, config_hash: u64) -> Option<&Manifest> {
        self.0
            .iter()
            .find_map(|(GenerationHash(hash, _), manifest)| {
                if *hash == config_hash {
                    Some(manifest)
                } else {
                    None
                }
            })
    }
}

impl Default for GenerationsFile {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
pub fn get_latest_manifest(
    generation_file: &ArchivedGenerationsFile,
    config_hash: u64,
) -> Option<&ArchivedManifest> {
    generation_file
        .0
        .iter()
        .find_map(|(ArchivedGenerationHash(hash, _), manifest)| {
            if *hash == config_hash {
                Some(manifest)
            } else {
                None
            }
        })
}

#[must_use]
pub fn get_latest_generation_number(
    generation_file: &ArchivedGenerationsFile,
    config_hash: u64,
) -> Option<u64> {
    generation_file
        .0
        .keys()
        .filter(|ArchivedGenerationHash(hash, _)| *hash == config_hash)
        .max_by_key(|ArchivedGenerationHash(_, generation)| generation)
        .map(|ArchivedGenerationHash(_, generation)| generation)
        .copied()
}

#[must_use]
pub fn get_next_generation_number(
    generation_file: &ArchivedGenerationsFile,
    config_hash: u64,
) -> u64 {
    get_latest_generation_number(generation_file, config_hash).unwrap_or(0) + 1
}

pub fn add_to_generations(
    generation_file: &ArchivedGenerationsFile,
    config_hash: u64,
    manifest: Manifest,
) -> GenerationsFile {
    let mut generations: GenerationsFile = generation_file.deserialize(&mut Infallible).unwrap();

    let generation = get_next_generation_number(generation_file, config_hash);
    generations
        .0
        .insert(GenerationHash(config_hash, generation), manifest);

    generations
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
    /// The plugin's unresolved name
    /// This is the name that is used in the config file
    /// This is used to resolve dependencies
    pub unresolved_name: String,
    /// Rename the plugin to this name when loading
    pub rename: Option<String>,
    /// If the plugin is optional
    pub optional: bool,
    /// The plugin's dependencies, as a list of plugin names. These are the non-resolved names
    pub dependencies: Vec<String>,
    /// the name of the loader this plugin is loaded by
    pub smith: String,
    /// A command which is run in the plugin's directory after loading
    pub build: String,
    /// The data which is used for the loader
    pub loader_data: Box<dyn SerializeLoaderInput>,
}

#[cfg(test)]
mod tests {
    // use rkyv::to_bytes;
    // use std::path::PathBuf;
    // use super::*;
    // #[test]
    // fn test_generation_serialize_deserialize() {
    //     let generation = Generation {
    //         generation: 1,
    //         path: StringPathBuf::new(PathBuf::from("file1.txt")),
    //     };

    //     let bytes = to_bytes::<_, 1024>(&generation).unwrap();
    //     let deserialized = rkyv::from_bytes::<Generation>(&bytes).unwrap();
    //     assert_eq!(generation, deserialized);
    // }

    // #[test]
    // fn test_generations_file_serialize_deserialize() {
    //     let mut generations_file = GenerationsFile::new();
    //     generations_file.add_to_generation(1, PathBuf::from("file1.txt"));

    //     let bytes = to_bytes::<_, 1024>(&generations_file).unwrap();
    //     let deserialized = rkyv::from_bytes::<GenerationsFile>(&bytes).unwrap();
    //     assert_eq!(generations_file, deserialized);
    // }

    // #[test]
    // fn test_get_next_generation_number() {
    //     let mut generations_file = GenerationsFile::new();
    //     assert_eq!(generations_file.get_next_generation_number(0), 1);
    //     generations_file.add_to_generation(0, PathBuf::from("manifest_path"));
    //     assert_eq!(generations_file.get_next_generation_number(0), 2);
    // }

    // #[test]
    // fn test_get_latest_generation() {
    //     let mut generations_file = GenerationsFile::new();
    //     let latest_generation = generations_file.get_latest_generation(0);
    //     assert!(latest_generation.is_none());

    //     generations_file.add_to_generation(0, PathBuf::from("manifest_path"));
    //     let latest_generation = generations_file.get_latest_generation(0).unwrap();
    //     assert_eq!(latest_generation.generation, 1);
    //     assert_eq!(latest_generation.path.to_str(), Some("manifest_path"));

    //     generations_file.add_to_generation(0, PathBuf::from("manifest_path2"));
    //     let latest_generation = generations_file.get_latest_generation(0).unwrap();
    //     assert_eq!(latest_generation.generation, 2);
    //     assert_eq!(latest_generation.path.to_str(), Some("manifest_path2"));
    // }

    // #[test]
    // fn test_get_latest_generation_number() {
    //     let mut generations_file = GenerationsFile::new();
    //     let latest_generation = generations_file.get_latest_generation_number(0);
    //     assert!(latest_generation.is_none());

    //     generations_file.add_to_generation(0, PathBuf::from("manifest_path"));
    //     let latest_generation = generations_file.get_latest_generation_number(0).unwrap();
    //     assert_eq!(latest_generation, 1);

    //     generations_file.add_to_generation(0, PathBuf::from("manifest_path2"));
    //     let latest_generation = generations_file.get_latest_generation_number(0).unwrap();
    //     assert_eq!(latest_generation, 2);
    // }
}
