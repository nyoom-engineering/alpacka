use crate::{
    config::ConfigPackage,
    loader::{Loader, LoaderError, LoaderResult},
    manifest::ManifestVersion,
    package::Package,
};
use error_stack::{bail, report, Context, IntoReport, Result, ResultExt};
use git2::Repository;
use std::{
    fmt::Display,
    path::{Path, PathBuf},
};
use tracing::{debug, info};

#[derive(Debug)]
#[non_exhaustive]
pub enum CloneError {
    GitError,
    MultipleLock,
}

impl Display for CloneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::GitError => f.write_str("Failed to update/install package"),
            Self::MultipleLock => f.write_str("Multiple lock types set"),
        }
    }
}

impl Context for CloneError {}

/// clones or updates a package using git
/// outputs the resolved commit hash
fn update_package(
    package: &Package,
    remote_path: &String,
    package_path: &PathBuf,
) -> Result<String, CloneError> {
    let tag = package
        .package
        .ver
        .as_ref()
        .map(|v| format!("refs/tags/{v}"));
    let branch = package
        .package
        .branch
        .as_ref()
        .map(|v| format!("refs/heads/{v}"));
    let commit = &package.package.commit;

    // make sure only one of these is set
    if (tag.is_some() && branch.is_some())
        || (tag.is_some() && commit.is_some())
        || (branch.is_some() && commit.is_some())
    {
        bail!(CloneError::MultipleLock)
    }

    if package_path.exists() {
        let mut remote_callbacks = git2::RemoteCallbacks::new();

        remote_callbacks.transfer_progress(|stats| {
            if stats.received_objects() == stats.total_objects() {
                info!(
                    "Resolving deltas {}/{}",
                    stats.indexed_deltas(),
                    stats.total_deltas()
                );
            } else if stats.total_objects() > 0 {
                info!(
                    "Received {}/{} objects ({}) in {} bytes",
                    stats.received_objects(),
                    stats.total_objects(),
                    stats.indexed_objects(),
                    stats.received_bytes()
                );
            }
            true
        });

        let repo = git2::Repository::open(package_path)
            .into_report()
            .attach_printable_lazy(|| format!("Failed to open repo at {}", package_path.display()))
            .change_context(CloneError::GitError)?;

        let mut remote = repo
            .find_remote("origin")
            .into_report()
            .attach_printable_lazy(|| {
                format!("Failed to find remote at {}", package_path.display())
            })
            .change_context(CloneError::GitError)?;

        let mut fetch_options = git2::FetchOptions::new();
        fetch_options.remote_callbacks(remote_callbacks);
        fetch_options.download_tags(git2::AutotagOption::All);
        info!("Updating {}", package_path.display());

        let refspecs = tag.map_or_else(
            || {
                if let Some(branch) = branch {
                    branch
                } else {
                    match commit {
                        Some(commit) => commit.to_string(),
                        None => "HEAD".to_string(),
                    }
                }
            },
            |tag| tag,
        );

        remote
            .fetch(&[&refspecs], Some(&mut fetch_options), None)
            .into_report()
            .attach_printable_lazy(|| {
                format!("Failed to fetch remote at {}", package_path.display())
            })
            .change_context(CloneError::GitError)?;

        let good_ref = if refspecs == "HEAD" {
            let head = repo
                .head()
                .into_report()
                .attach_printable_lazy(|| {
                    format!("Failed to get HEAD at {}", package_path.display())
                })
                .change_context(CloneError::GitError)?;
            head.name().unwrap().to_string()
        } else {
            refspecs
        };

        let mut reference = repo
            .find_reference(&good_ref)
            .into_report()
            .attach_printable_lazy(|| {
                format!("Failed to find reference at {}", package_path.display())
            })
            .change_context(CloneError::GitError)?;

        let name = match reference.name() {
            Some(name) => name.to_string(),
            None => String::from_utf8_lossy(reference.name_bytes()).to_string(),
        };

        let reference_commit = repo
            .reference_to_annotated_commit(&reference)
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "Failed to find reference commit at {}",
                    package_path.display()
                )
            })
            .change_context(CloneError::GitError)?;

        let msg = format!(
            "Fast-Forward: Setting {} to id: {}",
            name,
            reference_commit.id()
        );
        info!("{}", msg);

        reference
            .set_target(reference_commit.id(), &msg)
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "Failed to set reference target at {}",
                    package_path.display()
                )
            })
            .change_context(CloneError::GitError)?;

        let mut checkout_options = git2::build::CheckoutBuilder::default();
        checkout_options.force();

        repo.checkout_head(Some(&mut checkout_options))
            .into_report()
            .attach_printable_lazy(|| {
                format!("Failed to checkout head at {}", package_path.display())
            })
            .change_context(CloneError::GitError)?;

        // get the commit hash
        let commit = repo
            .head()
            .into_report()
            .attach_printable_lazy(|| format!("Failed to get HEAD at {}", package_path.display()))
            .change_context(CloneError::GitError)?
            .peel_to_commit()
            .into_report()
            .attach_printable_lazy(|| {
                format!("Failed to get HEAD commit at {}", package_path.display())
            })
            .change_context(CloneError::GitError)?;

        Ok(commit.id().to_string())
    } else {
        let repo = Repository::clone_recurse(remote_path, package_path)
            .into_report()
            .attach_printable_lazy(|| format!("Failed to clone {}", remote_path))
            .change_context(CloneError::GitError)?;

        // get the commit hash
        let commit = repo
            .head()
            .into_report()
            .attach_printable_lazy(|| format!("Failed to get HEAD at {}", package_path.display()))
            .change_context(CloneError::GitError)?
            .peel_to_commit()
            .into_report()
            .attach_printable_lazy(|| {
                format!("Failed to get HEAD commit at {}", package_path.display())
            })
            .change_context(CloneError::GitError)?;

        Ok(commit.id().to_string())
    }
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

#[derive(Debug, Default)]
pub enum GitLoaderFormat {
    #[default]
    Https,
    Ssh,
}
#[derive(Debug)]
pub struct GitLoader {
    format: GitLoaderFormat,
}

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
    fn load(&self, package: &Package, data_path: &Path) -> Result<LoaderResult, LoaderError> {
        let remote_path = match package.name.split_once(':') {
            None => package.name.to_owned(),
            Some(("github", path)) => match self.format {
                GitLoaderFormat::Https => format!("https://github.com/{}.git", path),
                GitLoaderFormat::Ssh => format!("git@github.com:{}.git", path),
            },
            Some(("git", path)) => match self.format {
                GitLoaderFormat::Https => format!("https://{}.git", path),
                GitLoaderFormat::Ssh => format!("git@{}.git", path),
            },
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

        let res = update_package(package, &remote_path, &package_path)
            .change_context(GitLoaderError::CloneError)
            .change_context(LoaderError)?;

        let version = if package.package.commit.is_some() {
            ManifestVersion::GitCommit(package.package.commit.clone().unwrap())
        } else if package.package.branch.is_some() {
            ManifestVersion::GitBranch(package.package.branch.clone().unwrap())
        } else if package.package.ver.is_some() {
            ManifestVersion::GitTag(package.package.ver.clone().unwrap())
        } else {
            ManifestVersion::GitCommit(res.clone())
        };

        Ok(LoaderResult {
            version,
            resolved_version: res,
        })
    }

    fn loads_package(&self, name: &str, _package: &ConfigPackage) -> bool {
        matches!(
            name.split_once(':'),
            Some(("github", _)) | Some(("gitlab", _)) | Some(("https", _))
        )
    }
}
