use serde_json::{Value};
use git2::{Repository, RemoteCallbacks, FetchOptions};
use std::path::{Path, PathBuf};
use std::env;
use structopt::StructOpt;
use std::thread;
use std::process::Command;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::fs;

#[derive(StructOpt)]
struct Cli {
    #[structopt(short = "i", long = "install")]
    install: bool,
}

fn clone_and_update(name: &str, details: &Value, data_path: &Path, pb: &ProgressBar) {
    let url = if name.starts_with("http") {
        name.to_string()
    } else {
        format!("https://github.com/{}.git", name)
    };
    let path = if details["opt"].as_str().unwrap_or("false") == "true" {
        data_path.join("opt").join(name.split('/').last().unwrap())
    } else {
        data_path.join("start").join(name.split('/').last().unwrap())
    };
    if !path.exists() {
        pb.set_message(&format!("Cloning {}", name));
        let mut cb = RemoteCallbacks::new();
        let mut fo = FetchOptions::new();
        let repo = match Repository::clone_with_options(url.as_str(), path.as_path(), &fo, Some(&mut cb)) {
            Ok(repo) => repo,
            Err(e) => panic!("failed to clone: {}", e),
        };
        if let Some(ver) = details["ver"].as_str() {
            repo.set_head_detached(ver).unwrap();
        } else if let Some

            let build_bar = MultiProgress.add(ProgressBar::new_spinner());
            build_bar.set_message(&format!("Building {}", name));
            let output = Command::new("sh")
                .arg("-c")
                .arg(build)
                .current_dir(path)
                .output()
                .expect("Failed to execute build command");
            build_bar.finish();

            if output.status.success() {
                println!("{} {}", name, "built successfully".green());
            } else {
                println!("{} {}", name, "build failed".red());
                println!("{}", String::from_utf8_lossy(&output.stderr));
            }
        }
    }
}

fn main() {
    let args = Cli::from_args();
    if args.install {
        let config_dir = env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| "~/.config/".into());
        let config_path = Path::new(&config_dir).join("nvim/packages.json");
        let json_string = fs::read_to_string(config_path).unwrap();
        let json_data: Value = serde_json::from_str(json_string.as_str()).unwrap();
        let data_dir = env::var("XDG_DATA_HOME").unwrap_or_else(|_| "~/.local/share/".into());
        let data_path = Path::new(&data_dir).join("nvim/site/pack/nyoom");

        let multi_progress = MultiProgress::new();
        let total_repos = json_data.as_object().unwrap().len();

        for (name, details) in json_data.as_object().unwrap() {
            let data_path = data_path.clone();
            let pb = multi_progress.add(ProgressBar::new_spinner());
            let handle = thread::spawn(move || {
                clone_and_update(name, details, &data_path, &pb);
                if let Some(deps) = details["dependencies"].as_object() {
                    for (dep_name, dep_details) in deps {
                        clone_and_update(dep_name, dep_details, &data_path, &pb);
                    }
                }
            });
            multi_progress.join().unwrap();
        }
        println!("Installation complete!");
    } else {
        println!("Please use the flag --install or -i to install packages")
    }
}
