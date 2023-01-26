use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Archive)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct Manifest {
    plugins: Vec<ManifestPlugin>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Archive)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ManifestPlugin {
    /// The plugin's name
    name: String,
    /// Rename the plugin to this name when loading
    rename: Option<String>,
    /// The plugin's version
    version: ManifestVersion,
    /// The plugin's resolved version
    /// e.g. if the version is a git branch, this will be the commit hash
    resolved_version: String,
    /// If the plugin is optional
    optional: bool,
    /// The plugin's dependencies, as a list of plugin names
    dependencies: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Archive)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug))]
pub enum ManifestVersion {
    /// The version is a git tag
    GitTag(String),
    /// The version is a git commit
    GitCommit(String),
    /// The version is a git branch
    GitBranch(String),
    /// The version is a custom version to be handled by its loader
    /// as an example, a loader could use this to load a specific file
    Custom(String),
}
