extern crate directories;
extern crate git2;
extern crate semver;
extern crate serde;

use directories::ProjectDirs;
use git2::Repository;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

#[derive(Deserialize)]
struct Plugin {
    opt: Option<bool>,
    ver: Option<String>,
    branch: Option<String>,
    commit: Option<String>,
    dependencies: Option<serde_json::Value>,
    build: Option<String>,
}

fn main() {
    let project_dirs = ProjectDirs::from("", "", "nyoom").unwrap();
    let json_string = fs::read_to_string(project_dirs.config_dir().join("plugins.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_string).unwrap();
    let nyoom_path = project_dirs.data_dir().join("nvim/site/pack/nyoom-cli/");
    let installed_plugins = get_installed_plugins(nyoom_path.to_str().unwrap());

    println!("Installing packages...");
    for (name, plugin) in json.as_object().unwrap().iter() {
        let plugin: Plugin = serde_json::from_value(plugin.clone()).unwrap();
        let opt = plugin.opt.unwrap_or(false);
        let repo;
        if name.starts_with("https") {
            repo = name.to_string();
        } else {
            repo = format!("https://github.com/{}.git", name);
        }
        let dest = nyoom_path
            .join(if opt { "opt" } else { "start" })
            .join(name);
        let mut options = CloneOptions::new();
        options.depth(1);
        if installed_plugins.contains(name) {
            println!(" > Updating {}...", name);
            let repo = Repository::open(&dest).unwrap();
            let branch = plugin
                .branch
                .unwrap_or(repo.head().unwrap().shorthand().unwrap());
            options.branch(&branch);

            if let Some(c) = plugin.commit {
                options.reference(&c);
            }

            repo.pull(&options).unwrap();
            installed_plugins.remove(name);
        } else {
            println!(" > Cloning {}...", name);
            let handle = thread::spawn(move || {
                let output = Repoistory::clone(&repo, &dest, &options).unwrap();
                println!(
                    " - Checked out {}: {}",
                    name,
                    output.reference().unwrap().name().unwrap()
                );
                if let Some(deps) = plugin.dependencies {
                    handle_dependencies(deps, nyoom_path);
                }
                if let Some(build) = plugin.build {
                    println!(" > Building {}...", name);
                    let output = Command::new("sh")
                        .arg("-c")
                        .arg(build)
                        .current_dir(dest)
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit())
                        .output()
                        .unwrap();
                    if !output.status.success() {
                        println!(
                            "Failed to build {} with error: {}",
                            name,
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                }
            });
            handle.join().unwrap();
        }
    }

    // delete plugin not in json file
    for plugin in installed_plugins {
        println!(" > Deleting {}...", plugin);
        let plugin_path = nyoom_path
            .join(if opt { "opt" } else { "start" })
            .join(plugin);
        fs::remove_dir_all(plugin_path).unwrap();
    }

    println!("Installation completed!");
}

fn get_installed_plugins(path: &str) -> HashSet<String> {
    let mut plugins = HashSet::new();
    for entry in WalkDir::new(path) {
        let entry = entry.unwrap();
        if entry.file_type().is_dir() {
            let dir_name = entry.file_name().to_str().unwrap();
            if !dir_name.starts_with(".") {
                plugins.insert(dir_name.to_string());
            }
        }
    }
    return plugins;
}

fn handle_plugin(deps: serde_json::Value, nyoom_path: PathBuf) {
    for (name, plugin) in deps.as_object().unwrap().iter() {
        let plugin: Plugin = serde_json::from_value(plugin.clone()).unwrap();
        let opt = plugin.opt.unwrap_or(false);
        let repo;
        if name.starts_with("https") {
            repo = name.to_string();
        } else {
            repo = format!("https://github.com/{}.git", name);
        }

        let dest = nyoom_path
            .join(if opt { "opt" } else { "start" })
            .join(name);
        let mut options = CloneOptions::new();
        options.depth(1); // this line will enable partial clone with a depth of 1

        if Path::new(&dest).exists() {
            println!(" > Updating dependency {}...", name);
            let repo = Repository::open(&dest).unwrap();
            let branch = plugin
                .branch
                .unwrap_or(repo.head().unwrap().shorthand().unwrap());
            options.branch(&branch);

            if let Some(c) = plugin.commit {
                options.reference(&c);
            }
            repo.pull(&options).unwrap();
        } else {
            println!(" > Cloning dependency {}...", name);
            let output = Repository::clone(&repo, &dest, &options).unwrap();
            println!(
                " - Checked out {}: {}",
                name,
                output.reference().unwrap().name().unwrap()
            );
        }
    }
}
