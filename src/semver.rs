//! get "latest" semver tag that fits tag
//! determine how to clone? pass that to git ig

use git2::{Direction, Repository};
use rand::Rng;
use semver::{Version, VersionReq};
use std::{collections::HashMap, fs::remove_dir_all};

#[derive(Debug)]
pub enum VersionError {
    SemverError(semver::Error),
    GitError(git2::Error),
    IoError(std::io::Error),
    InvalidVersionError,
}

impl From<semver::Error> for VersionError {
    fn from(value: semver::Error) -> Self {
        VersionError::SemverError(value)
    }
}

impl From<git2::Error> for VersionError {
    fn from(value: git2::Error) -> Self {
        VersionError::GitError(value)
    }
}

impl From<std::io::Error> for VersionError {
    fn from(value: std::io::Error) -> Self {
        VersionError::IoError(value)
    }
}

/// Get Latest commit for given semver
///
/// I couldn't find a way to do it without initializing a tmp repositiory :/
/// ```
/// use alpacka::semver::get_latest_commit;
///
/// assert_eq!(get_latest_commit("https://github.com/semver/semver".to_string(), ">=2.0.0".to_string()).unwrap(), "7c834b3f3a4940d77ab593bc32583004d6a426a9");
/// assert_eq!(get_latest_commit("https://github.com/semver/semver".to_string(), "<2.0.0".to_string()).unwrap(), "ec80195ed310aab3ae1f1ce797b7ba88b4246d27");
/// ```
pub fn get_latest_commit(git_url: String, tag: String) -> Result<String, VersionError> {
    let requirement = VersionReq::parse(&tag)?;

    let dir = format!("/tmp/alpacka_tmp_{}", random_string(10));

    let repo = Repository::init(&dir)?;
    let mut remote = repo.remote("origin", &git_url)?;
    remote.connect(Direction::Fetch)?;
    let refs = remote.list()?;

    let mut versions = HashMap::new();

    for ref_ in refs {
        if ref_.name().starts_with("refs/tags") {
            let v = match ref_.name().strip_prefix("refs/tags/") {
                Some(result) => result,
                None => return Err(VersionError::InvalidVersionError),
            };

            let ref_version = Version::parse(v.trim_start_matches("v"))?;
            if requirement.matches(&ref_version) {
                versions.insert(ref_version, ref_.oid().to_string());
            };
        }
    }

    let mut tmp: Vec<&Version> = versions.keys().collect();

    if tmp.len() == 0 {
        return Err(VersionError::InvalidVersionError);
    }

    tmp.sort();
    let sha = match versions.get(tmp[0]) {
        Some(v) => v,
        None => return Err(VersionError::InvalidVersionError),
    };

    remove_dir_all(&dir)?;
    Ok(sha.to_string())
}

fn random_string(len: usize) -> String {
    let rng = rand::thread_rng();
    let chars = rng
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(len)
        .collect();

    String::from_utf8(chars).unwrap()
}
