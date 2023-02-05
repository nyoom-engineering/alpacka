//! A module which contains structs and types for packages

use crate::smith::{DynSmith, ResolveError, SerializeLoaderInput};
use error_stack::{IntoReport, Result, ResultExt};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Serialize, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
/// A package declaration, as found in a config file
pub struct Config {
    /// Don't load the package on startup
    pub optional: Option<bool>,
    /// The package version. Internally uses git tags when using git, else the resolver decides
    pub version: Option<String>,
    /// rename the package to something else
    pub rename: Option<String>,
    /// A command to build the package. This is run in the package directory
    pub build: Option<String>,
    /// A list of dependencies
    #[serde(default)]
    pub dependencies: BTreeMap<String, Config>,
}

#[derive(Debug, Clone)]
/// A package declaration, as found in a config file plus some additional information
pub struct Package {
    /// The name of the package
    pub name: String,
    /// The package's config, as found in the config file
    pub config_package: Config,
}

#[derive(Debug, Clone)]
/// A package declaration and a smith name used to handle said package
pub struct WithSmith {
    /// The name of the smith used to handle this package
    pub smith: String,
    /// The package to be handled
    pub package: Package,
}

/// A type alias for a package loader input and a package with a smith
pub type WithLoaderInput = (Box<dyn SerializeLoaderInput>, WithSmith);

impl WithSmith {
    /// Check if this package is optional
    #[must_use]
    pub fn is_optional(&self) -> bool {
        self.package.config_package.optional.unwrap_or(false)
    }

    /// Resolve a package to a loader package, which has all the necessary information to load the package.
    ///
    /// # Errors
    /// This function will return an error if the package cannot be resolved.
    pub fn resolve(
        &self,
        smiths: &[Box<dyn DynSmith>],
    ) -> Result<Box<dyn SerializeLoaderInput>, ResolveError> {
        let smith = smiths
            .iter()
            .find(|smith| smith.name() == self.smith)
            .ok_or(ResolveError)
            .into_report()
            .attach_printable_lazy(|| format!("Smith {} not found", self.smith))?;

        smith.resolve_dyn(&self.package)
    }

    /// Recursively resolve a package to a loader package, which has all the necessary information to load the package.
    /// This function will also resolve all dependencies of the package.
    ///
    /// See [`WithSmith::resolve`] for more information.
    ///
    /// # Errors
    /// This function will return an error if the package or one of its dependencies cannot be resolved.
    pub fn resolve_recurse(
        self,
        smiths: &[Box<dyn DynSmith>],
    ) -> Result<Vec<WithLoaderInput>, ResolveError> {
        let mut deps = self
            .package
            .config_package
            .dependencies
            .par_iter()
            .map(|dep| {
                let pkg = Package {
                    name: dep.0.clone(),
                    config_package: dep.1.clone(),
                };

                let smith_to_use = smiths
                    .iter()
                    .find(|s| s.get_package_name(&pkg.name).is_some())
                    .ok_or(ResolveError)
                    .into_report()
                    .attach_printable_lazy(|| {
                        format!("Failed to find smith. Package name: {}", pkg.name)
                    })?;

                let package = Self {
                    smith: smith_to_use.name(),
                    package: pkg,
                };

                package.resolve_recurse(smiths)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let smith_to_use = smiths
            .iter()
            .find(|s| s.name() == self.smith)
            .ok_or(ResolveError)
            .into_report()
            .attach_printable_lazy(|| {
                format!("Failed to find smith. Smith name: {}", self.smith)
            })?;

        let loader_data = smith_to_use
            .resolve_dyn(&self.package)
            .attach_printable_lazy(|| {
                format!(
                    "Failed to resolve package. Package name: {}",
                    self.package.name
                )
            })
            .change_context(ResolveError)?;

        let mut final_package = vec![(loader_data, self)];
        deps.append(&mut final_package);

        Ok(deps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_smith_is_optional() {
        let with_smith = WithSmith {
            smith: "test".to_string(),
            package: Package {
                name: "test".to_string(),
                config_package: Config {
                    optional: Some(true),
                    version: None,
                    rename: None,
                    build: None,
                    dependencies: BTreeMap::new(),
                },
            },
        };

        assert!(with_smith.is_optional());
    }
}
