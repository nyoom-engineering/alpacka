use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use crate::config::{Config, Package};
use crate::{Message, StateEvent, StateEventKind, PKG_NAME};

#[derive(Debug)]
pub struct Installer {
    config: Config,
    pack_dir: PathBuf,
    upgrade_during_install: bool,
    sender: mpsc::SyncSender<Message>,
}

impl Installer {
    pub fn new(config: Config, sender: mpsc::SyncSender<Message>) -> Self {
        let pack_dir = home::home_dir()
            .map(|d| d.join(".local/share/nvim/site/pack"))
            .unwrap();

        Self {
            config,
            pack_dir,
            sender,
            upgrade_during_install: false,
        }
    }
    pub fn set_upgrade_during_install(&mut self, b: bool) {
        self.upgrade_during_install = b;
    }
    pub fn remove_unused(&self) -> Result<()> {
        let repo_dir_path = self.pack_dir.join(PKG_NAME).join("start");
        if !repo_dir_path.exists() {
            std::fs::create_dir_all(&repo_dir_path)?;
        }
        let repo_dir = repo_dir_path.read_dir()?;
        thread::scope(move |s| {
            for entry in repo_dir {
                s.spawn(move || {
                    let entry = entry.unwrap();
                    let meta = entry.metadata().unwrap();
                    let entry_name = entry.file_name();

                    let f = || -> anyhow::Result<()> {
                        if meta.is_dir()
                            && self.config.packages.iter().any(|(remote_path, cfg)| {
                                OsString::from(cfg.get_package_dirname(remote_path)) == entry_name
                            })
                        {
                            std::fs::remove_dir_all(&entry.path())?;
                            self.sender.send(Message::StateEvent(StateEvent::new(
                                &entry_name.to_string_lossy(),
                                StateEventKind::Removed,
                            )))?;
                        }
                        Ok(())
                    };

                    if let Err(e) = f() {
                        self.sender.send(Message::StateEvent(StateEvent::new(
                            &entry_name.to_string_lossy(),
                            StateEventKind::Failed(e),
                        )))?;
                    }

                    anyhow::Ok(())
                });
            }
        });

        self.sender.send(Message::Close)?;
        Ok(())
    }
    pub fn clone_repo(&self, remote_path: &str, cfg: &Package) -> Result<()> {
        let repo_path = self
            .pack_dir
            .join(PKG_NAME)
            .join("start")
            .join(cfg.get_package_dirname(remote_path));
        match git2::Repository::open(&repo_path) {
            Err(e) if e.code() == git2::ErrorCode::NotFound => {}
            Err(e) => return Err(e.into()),
            Ok(_) => {
                if self.upgrade_during_install {
                    self.pull_repo(remote_path, cfg)?;
                }
                return Ok(());
            }
        }

        self.sender.send(Message::StateEvent(StateEvent::new(
            remote_path,
            StateEventKind::Installing,
        )))?;

        let remote_url = cfg
            .host
            .as_ref()
            .map(|s| format!("{}/{}", s.trim_matches('/'), &remote_path))
            .unwrap_or(format!("https://github.com/{}", &remote_path));
        git2::build::RepoBuilder::new()
            .clone(&remote_url, &repo_path)
            .context("failed to clone repository")?;

        self.sender.send(Message::StateEvent(StateEvent::new(
            remote_path,
            StateEventKind::Installed,
        )))?;

        Ok(())
    }

    pub fn pull_repo(&self, remote_path: &str, cfg: &Package) -> Result<()> {
        let repo_path = self
            .pack_dir
            .join(PKG_NAME)
            .join("start")
            .join(cfg.get_package_dirname(remote_path));
        let repo = match git2::Repository::open(&repo_path) {
            Err(e) if e.code() == git2::ErrorCode::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
            Ok(repo) => repo,
        };

        let mut remote = repo.find_remote("origin")?;

        self.sender.send(Message::StateEvent(StateEvent::new(
            remote_path,
            StateEventKind::Updating,
        )))?;
        for branch in repo.branches(None)? {
            let (branch, branch_type) = branch?;

            if let git2::BranchType::Local = branch_type {
                let branch_name = branch.name()?.unwrap();

                remote.fetch(&[&branch_name], None, None)?;
                let fetch_head_ref = repo.find_reference("FETCH_HEAD")?;
                let fetch_commit = repo.reference_to_annotated_commit(&fetch_head_ref)?;

                let mut branch_head_ref =
                    repo.find_reference(&format!("refs/heads/{}", &branch_name))?;
                let (analysis, _pref) =
                    repo.merge_analysis_for_ref(&branch_head_ref, &[&fetch_commit])?;

                if analysis.is_fast_forward() {
                    branch_head_ref.set_target(fetch_commit.id(), "fast forwarding")?;
                    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
                    self.sender.send(Message::StateEvent(StateEvent::new(
                        remote_path,
                        StateEventKind::Updated,
                    )))?;
                } else if analysis.is_up_to_date() {
                    // println!("{} Already up to date", &remote_path);
                    self.sender.send(Message::StateEvent(StateEvent::new(
                        remote_path,
                        StateEventKind::UpToDate,
                    )))?;
                } else {
                    unimplemented!()
                }
            }
        }

        Ok(())
    }

    pub fn all_repos<F>(&self, f: F) -> Result<()>
    where
        F: Send + Copy + Fn(&Self, &str, &Package) -> Result<()>,
    {
        thread::scope(|s| {
            for (remote_path, pkg) in &self.config.packages {
                let sender = self.sender.clone();
                s.spawn(move || {
                    if let Err(e) = f(self, remote_path, pkg) {
                        sender.send(Message::StateEvent(StateEvent::new(
                            remote_path,
                            StateEventKind::Failed(e),
                        )))?;
                    }
                    Ok::<(), anyhow::Error>(())
                });
            }
        });

        self.sender.send(Message::Close)?;

        Ok(())
    }
}
