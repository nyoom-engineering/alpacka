//! Git related functions for alpacka

use std::{fmt::Display, fs::create_dir_all, path::Path};

use crate::loader::Package;
use error_stack::{bail, Context, IntoReport, Result, ResultExt};
use git2::Repository;
use tracing::info;

#[derive(Debug)]
pub enum CloneError {
    GitError,
    MultipleLock,
}

impl Display for CloneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitError => f.write_str("Failed to update/install package"),
            Self::MultipleLock => f.write_str("Multiple lock types set"),
        }
    }
}

impl Context for CloneError {}

/// clones or updates a package
pub fn update_package(name: String, package: Package, data_path: &Path) -> Result<(), CloneError> {
    let mut data_path = data_path.to_path_buf();
    if package.opt.unwrap_or(false) {
        data_path.push("opt");
    }

    let remote_path = name;
    let package_dirname = package.get_package_dirname(&remote_path);
    let package_path = data_path.join(package_dirname);

    if !package_path.exists() {
        create_dir_all(&data_path)
            .into_report()
            .attach_printable_lazy(|| {
                format!("Failed to create directory at {}", data_path.display())
            })
            .change_context(CloneError::GitError)?;
    }

    let tag = package.ver.map(|v| format!("refs/tags/{}", v));
    let branch = package.branch.map(|v| format!("refs/heads/{}", v));
    let commit = package.commit;

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

        let repo = git2::Repository::open(&package_path)
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

        let refspecs = if let Some(tag) = tag {
            format!("{}:{}", tag, tag)
        } else if let Some(branch) = branch {
            format!("{}:{}", branch, branch)
        } else if let Some(commit) = commit {
            format!("{}:{}", commit, commit)
        } else {
            "".to_string()
        };

        remote
            .fetch(&[&refspecs], Some(&mut fetch_options), None)
            .into_report()
            .attach_printable_lazy(|| {
                format!("Failed to fetch remote at {}", package_path.display())
            })
            .change_context(CloneError::GitError)?;

        let mut reference = repo
            .find_reference("FETCH_HEAD")
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
    } else {
        Repository::clone_recurse(&remote_path, &package_path)
            .into_report()
            .attach_printable_lazy(|| format!("Failed to clone {}", remote_path))
            .change_context(CloneError::GitError)?;
    }

    Ok(())
}
