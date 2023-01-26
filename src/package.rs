//! parses a json file and creates a bincode file for it

use error_stack::Result;
use std::path::Path;
use std::sync::Arc;

use crate::config::ConfigPackage;
use crate::loader::{Loader, LoaderError, LoaderResult};
#[derive(Debug)]
/// A package declaration
pub struct Package {
    pub name: String,
    pub loader: Arc<dyn Loader>,
    pub package: ConfigPackage,
}

impl Package {
    pub fn load(&self, data_path: &Path) -> Result<LoaderResult, LoaderError> {
        self.loader.load(self, data_path)
    }
}
