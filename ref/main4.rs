extern crate clap;
extern crate dirs;
extern crate git2;
extern crate serde;
extern crate serde_json;

use clap::{App, Arg};
use dirs::config_dir;
use dirs::data_dir;
use git2::{build::RepoBuilder, Error, Repository};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

const PACKAGE_NAME: &str = "nyoom";

#[derive(Deserialize, Serialize)]
struct Plugin {
    opt: Option<bool>,
    ver: Option<String>,
    branch: Option<String>,
    commit: Option<String>,
    build: Option<String>,
    dependencies: Vec<Plugin>,
}

impl Plugin {
    fn from_json(json: &Value) -> Self {
        let opt = json["opt"].as_bool();
        let ver = json["ver"].as_str().map(|s| s.to_string());
        let branch = json["branch"].as_str().map(|s| s.to_string());
        let commit = json["commit"].as_str().map(|s| s.to_string());
        let build = json["build"].as_str().map(|s| s.to_string());
        let dependencies = json["dependencies"].as_array();

        let dependencies_vec: Vec<Plugin> = dependencies
            .unwrap_or(&vec![])
            .iter()
            .map(|dep| Plugin::from_json(dep))
            .collect();

        Self {
            opt,
            ver,
            branch,
            commit,
            build,
            dependencies: dependencies_vec,
        }
    }

    fn clone(&self, repo_path: &Path) -> Result<(), Error> {
        let mut builder = RepoBuilder::new();
        builder.branch(&self.branch.unwrap_or_default());
        builder.clone_into(self.url(), repo_path);
        Ok(())
    }

    fn update(&self, repo_path: &Path) -> Result<(), Error> {
        let mut repo = Repository::open(repo_path)?;
        let remote = repo.find_remote("origin")?;
        remote.fetch(&["refs/heads/:refs/heads/"], None, None)?;

        if let Some(branch) = &self.branch {
            let branch = repo.find_branch(branch, git2::BranchType::Local)?;
            repo.set_head(branch.get().name().unwrap())?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        } else if let Some(commit) = &self.commit {
            let oid = git2::Oid::from_str(commit)?;
            repo.set_head_detached(oid)?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        } else if let Some(ver) = &self.ver {
            let semver = semver::Version::parse(ver)?;
            let tags = repo.tag_names(None)?;
            let tag = tags
                .into_iter()
                .filter_map(|t| semver::Version::parse(&t).ok())
                .filter(|t| t >= &semver)
                .max();
            if let Some(t) = tag {
                let tag = repo.find_tag(&t.to_string())?;
                let object = tag.peel_to_commit()?;
                repo.set_head_detached(object.id())?;
                repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
            }
        }
        Ok(())
    }

    fn delete(&self, repo_path: &Path) -> Result<(), std::io::Error> {
        std::fs::remove_dir_all(repo_path)
    }
    fn url(&self) -> String {
        format!("https://github.com/{}.git", self.name())
    }
    fn name(&self) -> String {
        self.branch.unwrap().split("/").collect::<Vec<&str>>()[1].to_string()
    }
}

fn main() {
    let matches = App::new("nyoom")
        .arg(Arg::with_name("install").short("i").long("install"))
        .get_matches();

    if matches.is_present("install") {
        let config_dir = config_dir().unwrap();
        let packages_file = config_dir.join(PACKAGE_NAME).join("packages.json");
        let packages_json = std::fs::read_to_string(packages_file).unwrap();
        let packages: HashMap<String, Value> = serde_json::from_str(&packages_json).unwrap();
        let data_dir = data_dir().unwrap();
        let cache_dir = data_dir
            .join("nvim")
            .join("site")
            .join("pack")
            .join(PACKAGE_NAME);

        std::fs::create_dir_all(cache_dir.clone()).unwrap();

        for (repo, json) in packages {
            let plugin = Plugin::from_json(&json);
            let repo_path = cache_dir.join(&repo);
            if repo_path.exists() {
                plugin.update(&repo_path).unwrap();
            } else {
                plugin.clone(&repo_path).unwrap();
            }
            if plugin.opt.unwrap_or(false) {
                std::fs::create_dir_all(
                    data_dir
                        .join("nvim")
                        .join("site")
                        .join("pack")
                        .join(PACKAGE_NAME)
                        .join("opt"),
                )
                .unwrap();
                std::fs::rename(
                    repo_path,
                    data_dir
                        .join("nvim")
                        .join("site")
                        .join("pack")
                        .join(PACKAGE_NAME)
                        .join("opt")
                        .join(&repo),
                )
                .unwrap();
            } else {
                std::fs::create_dir_all(
                    data_dir
                        .join("nvim")
                        .join("site")
                        .join("pack")
                        .join(PACKAGE_NAME)
                        .join("start"),
                )
                .unwrap();
                std::fs::rename(
                    repo_path,
                    data_dir
                        .join("nvim")
                        .join("site")
                        .join("pack")
                        .join(PACKAGE_NAME)
                        .join("start")
                        .join(&repo),
                )
                .unwrap();
            }
        }
    }
}
