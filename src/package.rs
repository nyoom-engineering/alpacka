//! A module which contains structs and types for packages

use std::collections::BTreeMap;

use error_stack::{IntoReport, Result, ResultExt};
use serde::{Deserialize, Serialize};

use crate::smith::{DynSmith, LoaderInput, ResolveError};

#[derive(Debug, Deserialize, Serialize, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
/// A package declaration, as found in a config file
pub struct Config {
    /// Don't load the package on startup
    pub opt: Option<bool>,
    /// The package version. Internally uses git tags when using git, else the resolver decides
    pub version: Option<String>,
    /// rename the package to something else
    pub rename: Option<String>,
    /// A command to build the package. This is run in the package directory
    pub build: Option<String>,
    /// A list of dependencies
    pub dependencies: Option<BTreeMap<String, Config>>,
}

#[derive(Debug, Clone)]
/// A package declaration, as found in a config file plus some additional information
pub struct Package {
    pub name: String,
    pub package: Config,
}

#[derive(Debug, Clone)]
/// A package declaration and a smith name used to handle said package
pub struct WithSmith {
    pub smith: String,
    pub package: Package,
}

impl WithSmith {
    /// Check if this package is optional
    #[must_use]
    pub fn is_optional(&self) -> bool {
        self.package.package.opt.unwrap_or(false)
    }

    /// Resolve a package to a loader package, which has all the necessary information to load the package.
    ///
    /// # Errors
    /// This function will return an error if the package cannot be resolved.
    pub fn resolve(
        &self,
        smiths: &[Box<dyn DynSmith>],
    ) -> Result<Box<dyn LoaderInput>, ResolveError> {
        let smith = smiths
            .iter()
            .find(|smith| smith.name() == self.smith)
            .ok_or(ResolveError)
            .into_report()
            .attach_printable_lazy(|| format!("Smith {} not found", self.smith))?;

        smith.resolve_dyn(&self.package)
    }
}
