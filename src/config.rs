use crate::{loader::Loader, package::Package};
use error_stack::{Context, IntoReport, Result};
use hashbrown::HashMap;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, sync::Arc};

#[derive(Debug, Deserialize, Serialize)]
/// The alpacka config format
pub struct Config {
    /// All the packages
    pub packages: HashMap<String, ConfigPackage>,
}

#[derive(Debug)]
pub enum CreatePackageListError {
    NoLoaderFound(String),
}

impl Display for CreatePackageListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLoaderFound(name) => {
                f.write_str(&format!("No loader found for package {}", name))
            }
        }
    }
}

impl Context for CreatePackageListError {}

impl Config {
    pub fn create_package_list(
        &self,
        loaders: Vec<Arc<dyn Loader>>,
    ) -> Result<Vec<Package>, CreatePackageListError> {
        let mut packages = Vec::with_capacity(self.packages.len());

        for (name, package) in &self.packages {
            let loader = loaders
                .iter()
                .find(|loader| loader.loads_package(name, package))
                .ok_or_else(|| CreatePackageListError::NoLoaderFound(name.to_owned()))
                .into_report()?
                .clone();

            let pkg = Package {
                loader,
                name: name.to_owned(),
                package: package.clone(),
            };

            packages.push(pkg);
        }

        Ok(packages)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConfigPackage {
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
    pub dependencies: Option<HashMap<String, ConfigPackage>>,
}
