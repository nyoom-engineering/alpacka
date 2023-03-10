mod git;
pub use git::Git;
use rkyv_dyn::archive_dyn;
use std::{
    any::Any,
    fmt::{Debug as FmtDebug, Display},
    path::Path,
};

use crate::package::Package;
use error_stack::{Context, IntoReport, Result as ErrorStackResult, ResultExt};

#[derive(Debug)]
/// An error that can occur when resolving a package
pub struct ResolveError;

impl Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to resolve package")
    }
}

impl Context for ResolveError {}

#[derive(Debug)]
/// An error that can occur when loading a package
pub struct LoadError;

impl Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to load package")
    }
}

impl Context for LoadError {}

/// A marker trait for loader inputs.
/// This trait is used to allow the loader input to be serialized and deserialized.
#[archive_dyn(deserialize)]
pub trait LoaderInput: FmtDebug + Send + Sync + UpcastAny {}

/// A trait that allows a loader input to be upcasted to a dyn [Any]
pub trait UpcastAny {
    fn upcast_any_ref(&self) -> &dyn Any;
}
/// A smith that can be used to resolve and load a package.
///
/// There are 2 main parts to a smith:
/// 1. A resolver that can resolve a config package to a loader package, which has all the necessary information to load the package. This is cached inside of the generation file.
/// 2. A loader that can download and install the package, and run the build script.
pub trait Smith: FmtDebug + Send + Sync {
    type Input: LoaderInput + SerializeLoaderInput;

    /// Gets the name of the smith
    fn name(&self) -> String;

    /// Check if this smith can load the given package. If it can, it will return the name of the package.
    /// This is used to find the correct smith for a package
    fn get_package_name(&self, name: &str) -> Option<String>;

    /// Resolve a package to a loader package, which has all the necessary information to load the package.
    /// This is cached inside of the generation file.
    ///
    /// # Errors
    /// This function will return an error if the package cannot be resolved.
    fn resolve(&self, package: &Package) -> ErrorStackResult<Self::Input, ResolveError>;

    /// Get latest commits for a git repo.
    ///
    /// # Errors
    /// This function will return an error if it cannot find the changes.
    fn get_change_log(
        &self,
        old_sha: Option<git2::Oid>,
        path: &Path,
    ) -> ErrorStackResult<Vec<String>, LoadError>;

    /// Loads a package.
    /// This downloads and installs the package to the given directory.
    ///
    /// # Errors
    /// This function will return an error if the package cannot be loaded.
    fn load(&self, input: &Self::Input, package_path: &Path) -> ErrorStackResult<(), LoadError>;
}

/// "dyn friendly" version of the smith trait, which removes the concrete associated type.
///
/// See the [Smith] trait for more information.
#[allow(clippy::module_name_repetitions)]
pub trait DynSmith: Send + Sync + FmtDebug {
    /// Gets the name of the smith
    fn name(&self) -> String;

    /// Check if this smith can load the given package. If it can, it will return the name of the package.
    /// This is used to find the correct smith for a package
    fn get_package_name(&self, name: &str) -> Option<String>;

    /// Get latest commits for a git repo.
    ///
    /// # Errors
    /// This function will return an error if it cannot find the changes.
    fn get_change_log(
        &self,
        old_sha: Option<git2::Oid>,
        path: &Path,
    ) -> ErrorStackResult<Vec<String>, LoadError>;

    /// Resolve a package to a loader package, which has all the necessary information to load the package.
    /// This is cached inside of the generation file.
    ///
    /// # Errors
    /// This function will return an error if the package cannot be resolved.
    fn resolve_dyn(
        &self,
        package: &Package,
    ) -> ErrorStackResult<Box<dyn SerializeLoaderInput>, ResolveError>;

    /// Loads a package.
    /// This downloads and installs the package to the given directory.
    ///
    /// # Errors
    /// This function will return an error if the package cannot be loaded or if the loader input is the wrong type.
    fn load_dyn(
        &self,
        input: &dyn SerializeLoaderInput,
        package_path: &Path,
    ) -> ErrorStackResult<(), LoadError>;
}

impl<T: Smith> DynSmith for T
where
    T::Input: 'static,
{
    #[tracing::instrument]
    fn name(&self) -> String {
        Smith::name(self)
    }

    #[tracing::instrument]
    fn get_package_name(&self, package: &str) -> Option<String> {
        Smith::get_package_name(self, package)
    }

    #[tracing::instrument]
    fn get_change_log(
        &self,
        old_sha: Option<git2::Oid>,
        path: &Path,
    ) -> ErrorStackResult<Vec<String>, LoadError> {
        Smith::get_change_log(self, old_sha, path)
    }

    #[tracing::instrument]
    fn resolve_dyn(
        &self,
        name: &Package,
    ) -> ErrorStackResult<Box<dyn SerializeLoaderInput>, ResolveError> {
        let input = Smith::resolve(self, name)?;
        Ok(Box::new(input))
    }

    #[tracing::instrument]
    fn load_dyn(
        &self,
        input: &dyn SerializeLoaderInput,
        package_path: &Path,
    ) -> ErrorStackResult<(), LoadError> {
        input.upcast_any_ref().downcast_ref().map_or_else(
            || {
                Err(LoadError)
                    .into_report()
                    .attach_printable_lazy(|| "Failed to downcast loader input. Wrong input type")
            },
            |input| Smith::load(self, input, package_path),
        )
    }
}
