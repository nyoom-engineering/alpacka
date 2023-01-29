mod git;
pub use git::Git;
use std::{
    any::Any,
    fmt::{Debug as FmtDebug, Display},
    path::Path,
};

use crate::package::Package;
use error_stack::{Context, Result};

#[derive(Debug)]
pub struct ResolveError;

impl Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to resolve package")
    }
}

impl Context for ResolveError {}

#[derive(Debug)]
pub struct LoadError;

impl Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to load package")
    }
}

impl Context for LoadError {}

/// A marker trait for loader inputs.
/// This trait is used to allow the loader input to be serialized and deserialized.
#[typetag::serde(tag = "loader")]
pub trait LoaderInput: FmtDebug + Send + Sync + Any {
    fn any(&self) -> Box<dyn Any>;
}

#[typetag::serde(name = "boxed_loader")]
impl LoaderInput for Box<dyn LoaderInput> {
    fn any(&self) -> Box<dyn Any> {
        self.as_ref().any()
    }
}

/// A smith that can be used to resolve and load a package.
///
/// There are 2 parts to a smith:
/// 1. A resolver that can resolve a config package to a loader package, which has all the necessary information to load the package. This is cached inside of the generation file.
/// 2. A loader that can download and install the package, and run the build script.
pub trait Smith: FmtDebug + Send + Sync {
    fn name(&self) -> String;

    /// Check if this smith can load the given package. If it can, it will return the name of the package.
    /// This is used to find the correct smith for a package
    fn get_package_name(&self, name: &str) -> Option<String>;

    /// Resolve a package to a loader package, which has all the necessary information to load the package.
    /// This is cached inside of the generation file.
    ///
    /// # Errors
    /// This function will return an error if the package cannot be resolved.
    fn resolve(&self, package: &Package) -> Result<Box<dyn LoaderInput>, ResolveError>;

    /// Loads a package.
    /// This downloads and installs the package to the given directory.
    ///
    /// # Errors
    /// This function will return an error if the package cannot be loaded.
    fn load(&self, input: &dyn LoaderInput, package_path: &Path) -> Result<(), LoadError>;
}

// implement smith for Box<dyn Smith>
impl<T> Smith for Box<T>
where
    T: Smith + Send + Sync,
{
    fn name(&self) -> String {
        self.as_ref().name()
    }

    fn get_package_name(&self, name: &str) -> Option<String> {
        self.as_ref().get_package_name(name)
    }

    fn resolve(&self, package: &Package) -> Result<Box<dyn LoaderInput>, ResolveError> {
        self.as_ref().resolve(package)
    }

    fn load(&self, input: &dyn LoaderInput, package_path: &Path) -> Result<(), LoadError> {
        self.as_ref().load(input, package_path)
    }
}
