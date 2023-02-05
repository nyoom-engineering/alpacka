use crate::{
    package::Package,
    smith::{DeserializeLoaderInput, SerializeLoaderInput},
};
use bytecheck::CheckBytes;
use error_stack::{Context, IntoReport, Result as ErrorStackResult, ResultExt};
use rkyv::Archive;
use rkyv_dyn::archive_dyn;
use rkyv_typename::TypeName;
use serde::{Deserialize, Serialize};
use std::{any::Any, fmt::Display, path::Path};
use tracing::debug;

use super::{LoadError, LoaderInput, ResolveError, Smith, UpcastAny};

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
    pub fn new() -> Self {
        Self::new_with_type(CloneType::default())
    }

    #[must_use]
    pub const fn new_with_type(clone_type: CloneType) -> Self {
        Self { clone_type }
    }
}

impl Default for Git {
    fn default() -> Self {
        Self::new()
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

#[derive(Debug, rkyv::Serialize, rkyv::Deserialize, Archive, Clone)]
#[archive_attr(derive(CheckBytes, Debug, TypeName))]
pub struct LoaderType {
    commit_hash: String,
    remote: String,
}

#[archive_dyn(deserialize)]
impl LoaderInput for LoaderType {}

impl UpcastAny for LoaderType {
    fn upcast_any_ref(&self) -> &dyn Any {
        self as &dyn Any
    }
}

impl Smith for Git {
    type Input = LoaderType;

    fn name(&self) -> String {
        "git".to_string()
    }

    fn resolve(&self, package: &Package) -> ErrorStackResult<Self::Input, ResolveError> {
        let Some((repo_type, repo_url)) = package
                .name
                .split_once(':') else {
                    unreachable!("should be handled by handles_package")
                };

        let repo_type = repo_type.to_owned();
        let repo_url = repo_url.to_owned();

        let url = match (repo_type.as_str(), repo_url.as_str()) {
            // repo format: git:host:path
            ("git", repo) => match self.clone_type {
                CloneType::Ssh => format!("git@{repo}.git"),
                CloneType::Https => {
                    let (host, path) = repo
                        .split_once(':')
                        .ok_or(ResolveError)
                        .into_report()
                        .attach_printable_lazy(|| format!("Failed to parse git repo: {repo}"))?;

                    format!("https://{host}/{path}.git")
                }
            },
            ("github", repo_url) => match self.clone_type {
                CloneType::Ssh => format!("git@github.com:{repo_url}.git"),
                CloneType::Https => format!("https://github.com/{repo_url}.git"),
            },
            ("gitlab", repo_url) => match self.clone_type {
                CloneType::Ssh => format!("git@gitlab.com:{repo_url}.git"),
                CloneType::Https => format!("https://gitlab.com/{repo_url}.git"),
            },
            ("srht", repo_url) => match self.clone_type {
                CloneType::Ssh => format!("git@git.sr.ht:{repo_url}"),
                CloneType::Https => format!("https://git.sr.ht/{repo_url}"),
            },
            _ => unreachable!("should be handled by handles_package"),
        };

        debug!("url: {url}");

        let lock_type = match package
            .package
            .version
            .as_ref()
            .and_then(|v| v.split_once(':'))
        {
            Some(("tag", tag)) => LockType::Tag(tag.to_string()),
            Some(("commit", commit)) => LockType::Commit(commit.to_string()),
            Some(("branch", branch)) => LockType::Branch(branch.to_string()),
            _ => LockType::Default,
        };

        debug!("lock_type: {lock_type:?}");

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
            .attach_printable_lazy(|| format!("Failed to find FETCH_HEAD: {url}. Check if the specified commit, tag, or branch exists."))
            .change_context(ResolveError)?;

        let commit_hash = fetch_head
            .peel_to_commit()
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to peel FETCH_HEAD to commit: {url}"))
            .change_context(ResolveError)?
            .id()
            .to_string();

        Ok(LoaderType {
            commit_hash,
            remote: url,
        })
    }

    /// Gets the commit messages between a sha and the current HEAD, if not sha is provided, it
    /// takes up to 5 of the latest commits on the current branch, provided they exist.
    ///
    /// ```
    /// use alpacka::{package::{Package, Config}, smith::{Smith, Git}};
    /// use std::path::Path;
    /// use std::collections::BTreeMap;
    ///
    /// let curr_dir = Path::new("testing");
    ///
    /// let smith = Git::new();
    ///
    /// let pkg = smith.resolve(&Package {
    ///     name: "github:zackartz/testing_repo".to_string(),
    ///     package: Config {
    ///         version: Some("tag:0.1.1".to_string()),
    ///         build: None,
    ///         dependencies: BTreeMap::new(),
    ///         optional: None,
    ///         rename: None,
    ///     }
    /// }).unwrap();
    ///
    /// smith.load(&pkg, &curr_dir);
    ///
    /// // sha of 0.1.0
    /// let oid = git2::Oid::from_str("90600fc317747afad28add17705199fc1eead17c").unwrap();
    ///
    /// let commits = smith.get_change_log(Some(oid), &curr_dir).unwrap();
    ///
    /// assert_eq!(commits[0], "Update README.md");
    ///
    /// // cleanup
    /// std::fs::remove_dir_all(&curr_dir);
    /// ```
    fn get_change_log(
        &self,
        old_sha: Option<git2::Oid>,
        path: &Path,
    ) -> ErrorStackResult<Vec<String>, LoadError> {
        let repo = match git2::Repository::open(path) {
            Ok(repo) => repo,
            Err(e) => {
                return Err(e)
                    .into_report()
                    .change_context(GitError::GitError)
                    .attach_printable_lazy(|| format!("Failed to open repo: {path:?}"))
                    .change_context(LoadError)
            }
        };

        let mut revwalk = repo
            .revwalk()
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| "Failed to get revwalk".to_string())
            .change_context(LoadError)?;

        revwalk
            .set_sorting(git2::Sort::TOPOLOGICAL)
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| "Failed to set revwalk sorting".to_string())
            .change_context(LoadError)?;

        let head = repo
            .head()
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| {
                format!("Failed to get current HEAD for repository: {path:?}")
            })
            .change_context(LoadError)?;

        let branch = head
            .peel_to_commit()
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| {
                format!("Failed to get current branch for repository: {path:?}")
            })
            .change_context(LoadError)?;

        revwalk
            .push(branch.id())
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to push commit to revwalk: {path:?}"))
            .change_context(LoadError)?;

        if let Some(sha) = old_sha {
            let commit = repo
                .find_commit(sha)
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| "Failed to get commit".to_string())
                .change_context(LoadError)?;

            revwalk
                .hide(commit.id())
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| format!("Failed to push commit to revwalk {path:?}"))
                .change_context(LoadError)?;
        }

        let mut messages = vec![];
        let mut count = 0;
        for id in revwalk.take(5) {
            let commit_id = id
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| "Failed to get commit_id".to_string())
                .change_context(LoadError)?;

            let commit = repo
                .find_commit(commit_id)
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| format!("Failed to find commit {commit_id}"))
                .change_context(LoadError)?;
            messages.push(commit.message().unwrap_or("").to_string());
            count += 1;
            if old_sha.is_none() && count == 5 {
                break;
            }
        }

        Ok(messages.iter().map(|m| m.trim().to_string()).collect())
    }

    fn load(&self, input: &Self::Input, path: &Path) -> ErrorStackResult<(), LoadError> {
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

        repo.remote_anonymous(&input.remote)
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to add remote: {}", input.remote))
            .change_context(LoadError)?;

        let commit_hash = git2::Oid::from_str(&input.commit_hash)
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to parse commit hash: {}", input.commit_hash))
            .change_context(LoadError)?;

        let commit = repo
            .find_commit(commit_hash)
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to find commit: {}", input.commit_hash))
            .change_context(LoadError)?;

        debug!("Resetting {} to commit: {:?}", input.remote, commit);

        repo.reset(&commit.into_object(), git2::ResetType::Hard, None)
            .into_report()
            .change_context(GitError::GitError)
            .attach_printable_lazy(|| format!("Failed to reset to FETCH_HEAD: {}", input.remote))
            .change_context(LoadError)?;

        Ok(())
    }

    fn get_package_name(&self, name: &str) -> Option<String> {
        match name.split_once(':') {
            Some(("git", name)) => name.rsplit_once('/').map(|(_, name)| name.to_string()),
            Some(("github", name)) => name.split_once('/').map(|(_, name)| name.to_string()),
            Some(("gitlab", name)) => name.split_once('/').map(|(_, name)| name.to_string()),
            Some(("srht", name)) => name.split_once('/').map(|(_, name)| name.to_string()),
            _ => None,
        }
    }
}

fn fetch_remote(
    url: &String,
    lock_type: &LockType,
    remote: &mut git2::Remote,
) -> ErrorStackResult<(), ResolveError> {
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
            remote
                .connect(git2::Direction::Fetch)
                .into_report()
                .change_context(GitError::GitError)
                .attach_printable_lazy(|| format!("Failed to connect to remote: {url}"))
                .change_context(ResolveError)?;

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
                    &[&format!("{default_branch_name}:{default_branch_name}")],
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
