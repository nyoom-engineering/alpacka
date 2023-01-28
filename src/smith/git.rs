use crate::package::Package;
use error_stack::{Context, IntoReport, Result, ResultExt};
use serde::{Deserialize, Serialize};
use std::{fmt::Display, path::Path};
use tracing::debug;

use super::{LoadError, LoaderInput, ResolveError, Smith};

#[derive(Debug)]
enum GitError {
    GitError,
    IoError,
}

impl Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError => f.write_str("IO error"),
            Self::GitError => f.write_str("Git error"),
        }
    }
}

impl Context for GitError {}

#[derive(Debug, Default, Clone)]
pub enum CloneType {
    Ssh,
    #[default]
    Https,
}

#[derive(Debug, Clone)]
pub struct Git {
    pub clone_type: CloneType,
}

impl Git {
    #[must_use]
    /// Create a new git smith with the default clone type
    pub fn create() -> Box<dyn Smith<Input = Box<dyn LoaderInput>>> {
        Self::create_with_type(CloneType::default())
    }

    #[must_use]
    pub fn create_with_type(clone_type: CloneType) -> Box<dyn Smith<Input = Box<dyn LoaderInput>>> {
        Box::new(Self { clone_type })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum LockType {
    /// Lock to a specific tag
    Tag(String),
    /// Lock to a specific commit
    Commit(String),
    /// Lock to a specific branch
    Branch(String),
    /// Lock to the default branch
    Default,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoaderType {
    commit_hash: String,
    remote: String,
}

#[typetag::serde]
impl LoaderInput for LoaderType {}

#[typetag::serde]
impl LoaderInput for Box<LoaderType> {}

impl Smith for Git {
    type Input = Box<dyn LoaderInput>;

    fn name(&self) -> String {
        "git".to_string()
    }

    fn handles_package(&self, package: &crate::package::Package) -> bool {
        debug!("{:?}", package.name.split_once(':'));
        matches!(package.name.split_once(':'), Some(("git" | "github", _)))
    }

    fn resolve(&self, package: &Package) -> Result<Self::Input, ResolveError> {
        let Some((repo_type, repo_url)) = package
                .name
                .split_once(':') else {
                    unreachable!("should be handled by handles_package")
                };

        let repo_type = repo_type.to_owned();
        let repo_url = repo_url.to_owned();

        let url = match (repo_type.as_str(), repo_url.as_str()) {
            ("git", repo_url) => match self.clone_type {
                CloneType::Ssh => format!("git@{repo_url}"),
                CloneType::Https => repo_url.to_string(),
            },
            ("github", repo_url) => match self.clone_type {
                CloneType::Ssh => format!("git@github.com:{repo_url}"),
                CloneType::Https => format!("https://github.com/{repo_url}.git"),
            },
            _ => unreachable!("should be handled by handles_package"),
        };

        let lock_type = package.package.ver.as_ref().map_or_else(
            || {
                package.package.branch.as_ref().map_or_else(
                    || {
                        package
                            .package
                            .commit
                            .as_ref()
                            .cloned()
                            .map_or(LockType::Default, LockType::Commit)
                    },
                    |branch| LockType::Branch(branch.clone()),
                )
            },
            |ver| LockType::Tag(ver.clone()),
        );

        let temp_git_dir = tempfile::tempdir()
            .into_report()
            .change_context(GitError::IoError)
            .attach_printable_lazy(|| format!("Failed to create temp dir for git repo: {url}"))
            .change_context(ResolveError)?;

        // init git repo and add remote
        let repo = git2::Repository::init(temp_git_dir.path())
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to init git repo: {url}"))
            .change_context(ResolveError)?;

        let mut remote = repo
            .remote_anonymous(&url)
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to add remote: {url}"))
            .change_context(ResolveError)?;

        fetch_remote(&url, &lock_type, &mut remote)?;

        let fetch_head = repo
            .find_reference("FETCH_HEAD")
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to find FETCH_HEAD: {url}"))
            .change_context(ResolveError)?;

        let commit_hash = fetch_head
            .peel_to_commit()
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to peel FETCH_HEAD to commit: {url}"))
            .change_context(ResolveError)?
            .id()
            .to_string();

        Ok(Box::new(LoaderType {
            commit_hash,
            remote: url,
        }))
    }

    fn load(&self, input: Self::Input, path: &Path) -> Result<(), LoadError> {
        let input = (input).any();
        dbg!(input.is::<Box<dyn LoaderInput>>());
        let input = input.downcast_ref::<LoaderType>().unwrap();

        let repo = match git2::Repository::open(path) {
            Ok(repo) => repo,
            Err(e) => match e.code() {
                git2::ErrorCode::NotFound => git2::Repository::clone(&input.remote, path)
                    .into_report()
                    .change_context(GitError::GitError)
                    .attach_printable_lazy(|| format!("Failed to clone repo: {}", input.remote))
                    .change_context(LoadError)?,
                _ => {
                    return Err(e)
                        .into_report()
                        .change_context(GitError::GitError)
                        .attach_printable_lazy(|| format!("Failed to open repo: {}", input.remote))
                        .change_context(LoadError)
                }
            },
        };

        let mut remote = repo
            .remote_anonymous(&input.remote)
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to add remote: {}", input.remote))
            .change_context(LoadError)?;

        fetch_remote(
            &input.remote,
            &LockType::Commit(input.commit_hash.clone()),
            &mut remote,
        )
        .change_context(LoadError)?;

        let fetch_head = repo
            .find_reference("FETCH_HEAD")
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to find FETCH_HEAD: {}", input.remote))
            .change_context(LoadError)?;

        let commit = fetch_head
            .peel_to_commit()
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| {
                format!("Failed to peel FETCH_HEAD to commit: {}", input.remote)
            })
            .change_context(LoadError)?;

        let commit_hash = commit.id().to_string();

        if commit_hash != input.commit_hash {
            repo.reset(&commit.into_object(), git2::ResetType::Hard, None)
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| {
                    format!("Failed to reset to FETCH_HEAD: {}", input.remote)
                })
                .change_context(LoadError)?;
        }

        Ok(())
    }
}

fn fetch_remote(
    url: &String,
    lock_type: &LockType,
    remote: &mut git2::Remote,
) -> Result<(), ResolveError> {
    match lock_type {
        LockType::Tag(tag) => {
            remote
                .fetch(&[&format!("refs/tags/{tag}:refs/tags/{tag}")], None, None)
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| format!("Failed to fetch tag: {tag}"))
                .change_context(ResolveError)?;
        }
        LockType::Commit(commit) => {
            remote
                .fetch(
                    &[&format!("refs/heads/{commit}:refs/heads/{commit}")],
                    None,
                    None,
                )
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| format!("Failed to fetch commit: {commit}"))
                .change_context(ResolveError)?;
        }
        LockType::Branch(branch) => {
            remote
                .fetch(
                    &[&format!("refs/heads/{branch}:refs/heads/{branch}")],
                    None,
                    None,
                )
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| format!("Failed to fetch branch: {branch}"))
                .change_context(ResolveError)?;
        }
        LockType::Default => {
            let default_branch = remote
                .default_branch()
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| format!("Failed to fetch default branch: {url}"))
                .change_context(ResolveError)?;

            let default_branch_name = default_branch
                .as_str()
                .ok_or(GitError::GitError)
                .into_report()
                .attach_printable_lazy(|| format!("Failed to find default branch: {url}"))
                .change_context(ResolveError)?;

            remote
                .fetch(
                    &[&format!(
                        "refs/heads/{default_branch_name}:refs/heads/{default_branch_name}"
                    )],
                    None,
                    None,
                )
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| {
                    format!("Failed to fetch default branch: {default_branch_name}")
                })
                .change_context(ResolveError)?;
        }
    };

    Ok(())
}
