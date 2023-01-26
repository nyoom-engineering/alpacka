use error_stack::{Context, Result};
use std::{
    fmt::{Debug as DebugTrait, Display},
    path::Path,
};

use crate::{config::ConfigPackage, manifest::ManifestVersion, package::Package};

#[derive(Debug)]
pub struct LoaderError;

impl Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Loader error")
    }
}

impl Context for LoaderError {}

#[derive(Debug)]
pub struct LoaderResult {
    pub version: ManifestVersion,
    pub resolved_version: String,
}

pub trait Loader: DebugTrait {
    /// Installs and updates the package given to it
    fn load(&self, package: &Package, data_path: &Path) -> Result<LoaderResult, LoaderError>;
    /// Returns true if the loader can load the package
    fn loads_package(&self, name: &str, package: &ConfigPackage) -> bool;
}
