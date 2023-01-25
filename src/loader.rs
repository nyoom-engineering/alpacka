//! parses a json file and creates a bincode file for it

use bincode2::{deserialize, deserialize_from, serialize};
use error_stack::{bail, Context, IntoReport, Result, ResultExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt::Display;
use std::fs::{self, create_dir_all};
use std::path::Path;
use std::process::{Command, Stdio};
use tracing::{debug, info};

use crate::git::update_package;

#[derive(Debug, Deserialize, Serialize)]
/// The alpacka config format
pub struct Config {
    /// All the packages
    pub packages: HashMap<String, Package>,
}

impl Config {
    pub fn install_all_packages(&self, data_path: &Path) -> Result<(), InstallPackageError> {
        for (name, package) in self.packages.iter() {
            package.install_plugin(name, data_path)?;
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
/// A package declaration
pub struct Package {
    /// Don't load the package on startup
    pub opt: Option<bool>,
    /// The package version. Internally uses git tags
    pub ver: Option<String>,
    /// rename the package to something else
    pub rename: Option<String>,
    /// the remote branch
    pub branch: Option<String>,
    /// the remote commit
    pub commit: Option<String>,
    /// A command to build the package. This is run in the package directory
    pub build: Option<String>,
    /// A list of dependencies
    pub dependencies: Option<HashMap<String, Package>>,
}

#[derive(Debug)]
pub enum InstallPackageError {
    InvalidLocator,
    PackageError,
    IoError,
}

impl Display for InstallPackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLocator => f.write_str(
                "Invalid locator. Valid options are currently: github, gitlab, and https",
            ),
            Self::IoError => f.write_str("IO Error"),
            Self::PackageError => f.write_str("Package Installation Error"),
        }
    }
}

impl Context for InstallPackageError {}

impl Package {
    /// Get the package directory name
    pub fn get_repo_name(&self, remote_path: &str) -> String {
        if let Some(renamed) = &self.rename {
            return renamed.to_owned();
        }

        let mut package_dirname = remote_path
            .split('/')
            .last()
            .unwrap_or_else(|| panic!("Invalid remote path: {}", remote_path))
            .to_owned();
        // remove the .git extension
        if package_dirname.ends_with(".git") {
            package_dirname.truncate(package_dirname.len() - 4);
        }
        package_dirname
    }

    pub fn install_plugin(
        &self,
        name: &String,
        data_path: &Path,
    ) -> Result<(), InstallPackageError> {
        if let Some(dependencies) = &self.dependencies {
            for (dep_name, dep) in dependencies {
                dep.install_plugin(dep_name, data_path)?;
            }
        }

        let remote_path = match name.split_once(':') {
            None => name.to_owned(),
            Some(("github", path)) => format!("https://github.com/{}.git", path),
            Some(("gitlab", path)) => format!("https://gitlab.com/{}.git", path),
            Some(("https", path)) => format!("https:{}", path),
            Some((_, _)) => bail!(InstallPackageError::InvalidLocator),
        };

        let mut data_path = data_path.to_path_buf();
        debug!("data_path {}", data_path.display());

        if self.opt.unwrap_or(false) {
            data_path.push("opt");
        }

        let repo = self.get_repo_name(&remote_path);
        let package_path = data_path.join(repo);

        if !data_path.exists() {
            create_dir_all(&data_path)
                .into_report()
                .attach_printable_lazy(|| {
                    format!("Failed to create directory at {}", data_path.display())
                })
                .change_context(InstallPackageError::IoError)?;
        }

        update_package(self, &remote_path, &package_path)
            .change_context(InstallPackageError::PackageError)?;

        let Some(build) = &self.build else {
            return Ok(());
        };
        let mut build_iter = build.split_whitespace();
        let mut build_command = Command::new(build_iter.next().unwrap());
        for arg in build_iter {
            build_command.arg(arg);
        }

        build_command.current_dir(&package_path);
        build_command.stdout(Stdio::piped());
        build_command.stderr(Stdio::piped());

        build_command
            .spawn()
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "An error occured when installing package {}",
                    package_path.display()
                )
            })
            .change_context(InstallPackageError::PackageError)?;

        Ok(())
    }
}

#[derive(Debug)]
pub enum ReadError {
    IoError,
    SerdeError,
}

impl Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError => f.write_str("Failed to read file"),
            Self::SerdeError => f.write_str("Failed to parse json"),
        }
    }
}

impl Context for ReadError {}

/// Read the package manifest and return the config along with its generation
pub fn read<P: AsRef<Path> + Copy>(path: &P) -> Result<(usize, Config), ReadError> {
    let json_bytes = fs::read(path)
        .into_report()
        .change_context(ReadError::IoError)?;
    let json_hash = hash_json(&json_bytes);
    let index_path = Path::new("index.bin");
    let bincode_path = format!("{json_hash}.bin");
    let (generation, config) = if fs::metadata(&bincode_path).is_ok() {
        let file_handle = fs::File::open(index_path)
            .into_report()
            .change_context(ReadError::IoError)?;
        let index_data: HashMap<String, usize> = deserialize_from(file_handle)
            .into_report()
            .change_context(ReadError::SerdeError)?;

        if let Some(gen) = index_data.get(&json_hash) {
            info!(
                "Loading bincode for generation {} for hash {}",
                gen, json_hash
            );
            let file = fs::read(&bincode_path)
                .into_report()
                .change_context(ReadError::IoError)?;

            let config = deserialize(&file)
                .into_report()
                .change_context(ReadError::SerdeError)?;

            (*gen, config)
        } else {
            let new_gen = index_data.len();
            info!(
                "Creating new bincode for generation {} for hash {}",
                new_gen, json_hash
            );
            let config = serde_json::from_slice(&json_bytes)
                .into_report()
                .change_context(ReadError::SerdeError)?;

            write_to_index(index_path, json_hash.clone(), new_gen)
                .change_context(ReadError::IoError)?;
            serialize_to_file(&config, &bincode_path).change_context(ReadError::IoError)?;

            (new_gen, config)
        }
    } else {
        info!(
            "Creating new bincode for generation 0 for hash {}",
            json_hash
        );

        let config = serde_json::from_slice(&json_bytes)
            .into_report()
            .change_context(ReadError::SerdeError)?;
        write_to_index(index_path, json_hash, 0)?;
        serialize_to_file(&config, &bincode_path)?;

        (0, config)
    };
    Ok((generation, config))
}

/// Hash a json file into a sha256 string
fn hash_json(json: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(json);

    format!("{:x}", hasher.finalize())
}

/// Write the index file
fn write_to_index<P: AsRef<Path> + Copy>(
    index_path: P,
    hash: String,
    generation: usize,
) -> Result<(), ReadError> {
    let index_data = match fs::metadata(index_path) {
        Ok(_) => deserialize_from(
            fs::File::open(index_path)
                .into_report()
                .change_context(ReadError::IoError)?,
        )
        .into_report()
        .change_context(ReadError::SerdeError)?,

        Err(_) => HashMap::new(),
    };
    let mut index_data = index_data;
    index_data.insert(hash, generation);
    let encoded = serialize(&index_data)
        .into_report()
        .change_context(ReadError::SerdeError)?;

    fs::write(index_path, encoded)
        .into_report()
        .change_context(ReadError::IoError)?;
    Ok(())
}

fn serialize_to_file<P: AsRef<Path>>(config: &Config, file_path: P) -> Result<(), ReadError> {
    let encoded = serialize(config)
        .into_report()
        .change_context(ReadError::SerdeError)?;

    fs::write(file_path, encoded)
        .into_report()
        .change_context(ReadError::IoError)?;
    Ok(())
}
