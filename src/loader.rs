use error_stack::{report, Context, Result, ResultExt};
use std::{
    fmt::{Debug as DebugTrait, Display},
    path::Path,
};
use tracing::debug;

use crate::{config::ConfigPackage, git::update_package, package::Package};

#[derive(Debug)]
pub struct LoaderError;

impl Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Loader error")
    }
}

impl Context for LoaderError {}

pub trait Loader: DebugTrait {
    /// Installs and updates the package given to it
    fn load(&self, package: &Package, data_path: &Path) -> Result<(), LoaderError>;
    /// Returns true if the loader can load the package
    fn loads_package(&self, name: &str, package: &ConfigPackage) -> bool;
}

#[derive(Debug)]
pub enum GitLoaderError {
    InvalidLocator,
    CloneError,
}

impl Display for GitLoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLocator => f.write_str(
                "Invalid locator. Valid options are currently: github, gitlab, and https",
            ),
            Self::CloneError => f.write_str("Error cloning repository"),
        }
    }
}

impl Context for GitLoaderError {}

#[derive(Debug)]
pub struct GitLoader;

impl GitLoader {
    /// Get the package directory name
    pub fn get_repo_name(package: &ConfigPackage, remote_path: &str) -> String {
        if let Some(renamed) = &package.rename {
            return renamed.to_owned();
        }
        let mut package_dirname = remote_path
            .split('/')
            .last()
            .unwrap_or_else(|| panic!("Invalid remote path: {}", remote_path))
            .to_owned();

        if package_dirname.ends_with(".git") {
            package_dirname.truncate(package_dirname.len() - 4);
        }

        package_dirname
    }
}

impl Loader for GitLoader {
    fn load(&self, package: &Package, data_path: &Path) -> Result<(), LoaderError> {
        let remote_path = match package.name.split_once(':') {
            None => package.name.to_owned(),
            Some(("github", path)) => format!("https://github.com/{}.git", path),
            Some(("gitlab", path)) => format!("https://gitlab.com/{}.git", path),
            Some(("https", path)) => format!("https:{}", path),
            Some((_, _)) => {
                return Err(report!(GitLoaderError::InvalidLocator).change_context(LoaderError))
            }
        };

        let mut data_path = data_path.to_path_buf();
        debug!("data_path {}", data_path.display());

        if package.package.opt.unwrap_or(false) {
            data_path.push("opt");
        } else {
            data_path.push("start");
        };

        let repo = GitLoader::get_repo_name(&package.package, &remote_path);
        let package_path = data_path.join(repo);

        update_package(package, &remote_path, &package_path)
            .change_context(GitLoaderError::CloneError)
            .change_context(LoaderError)?;

        Ok(())
    }

    fn loads_package(&self, name: &str, _package: &ConfigPackage) -> bool {
        matches!(
            name.split_once(':'),
            Some(("github", _)) | Some(("gitlab", _)) | Some(("https", _))
        )
    }
}
