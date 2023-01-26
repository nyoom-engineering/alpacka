use crate::package::Package;
use error_stack::{bail, Context, IntoReport, Result, ResultExt};
use git2::Repository;
use std::{fmt::Display, path::PathBuf};
use tracing::info;

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
pub fn update_package(
    package: &Package,
    remote_path: &String,
    package_path: &PathBuf,
) -> Result<(), CloneError> {
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
    } else {
        Repository::clone_recurse(remote_path, package_path)
            .into_report()
            .attach_printable_lazy(|| format!("Failed to clone {}", remote_path))
            .change_context(CloneError::GitError)?;
    }

    Ok(())
}
