#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use alpacka::{
    config::Config,
    manifest::{GenerationsFile, Manifest, Plugin},
    smith::{DynSmith, Git},
};
use error_stack::{Context, IntoReport, Result, ResultExt};
use rayon::prelude::*;
use rkyv::from_bytes;
use std::{
    collections::hash_map::DefaultHasher,
    fmt::{Display, Formatter},
    hash::{Hash, Hasher},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};
use tracing::{error, info, warn};

#[derive(Debug)]
struct MainError;

impl Display for MainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to run alpacka!")
    }
}

impl Context for MainError {}

fn main() -> error_stack::Result<(), MainError> {
    tracing_subscriber::fmt::init();

    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .and_then(dirs_sys::is_absolute_path)
        .or_else(|| dirs_sys::home_dir().map(|h| h.join(".config")));
    let config_path = config_dir.map(|cd| cd.join("nvim/packages.json")).unwrap();
    let data_dir = std::env::var_os("XDG_DATA_HOME")
        .and_then(dirs_sys::is_absolute_path)
        .or_else(|| dirs_sys::home_dir().map(|h| h.join(".local/share")));
    let data_path = data_dir
        .map(|dd| dd.join("nvim/site/pack/alpacka/"))
        .unwrap();

    if !data_path.exists() {
        std::fs::create_dir_all(&data_path)
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "Failed to create alpacka directory. Alpacka directory path: {}",
                    data_path.display()
                )
            })
            .change_context(MainError)?;
    }

    load_alpacka(&data_path, config_path)?;
    Ok(())
}

fn load_alpacka(data_path: &PathBuf, config_path: PathBuf) -> Result<(), MainError> {
    let config_file = std::fs::File::open(config_path)
        .into_report()
        .attach_printable_lazy(|| "Failed to open config file".to_string())
        .change_context(MainError)?;
    let config: Config = serde_json::from_reader(config_file)
        .into_report()
        .attach_printable_lazy(|| "Failed to parse config file".to_string())
        .change_context(MainError)?;

    info!("Config loaded, checking for existing manifest");

    let config_hash = {
        let mut hasher = DefaultHasher::new();
        config.hash(&mut hasher);
        hasher.finish()
    };

    let smiths: Vec<Box<dyn DynSmith>> = vec![Box::new(Git::new())];
    let generation_path = data_path.join("generations.rkyv");
    let manifest = if generation_path.exists() {
        let generations_file = std::fs::read(&generation_path)
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "Failed to read generations file. Generations file path: {}",
                    generation_path.display()
                )
            })
            .change_context(MainError)?;

        let mut generations: GenerationsFile = from_bytes(&generations_file)
            .map_err(|_| MainError) // TODO: better error
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "Failed to parse generations file. Generations file path: {}",
                    generation_path.display()
                )
            })?;

        // find generation that have the same hash as the current config, and the highest generation
        match generations.get_latest_generation(config_hash) {
            Some(manifest) => {
                info!(
                    "Found generation with the same hash as the current config, loading manifest"
                );

                let manifest_file = std::fs::read(&*manifest.path)
                    .into_report()
                    .attach_printable_lazy(|| {
                        format!(
                            "Failed to open manifest file. Manifest file path: {}",
                            manifest.path.display()
                        )
                    })
                    .change_context(MainError)?;

                let manifest: Manifest = from_bytes(&manifest_file)
                    .map_err(|e| {
                        error!("Failed to parse manifest file: {}", e);
                        MainError
                    }) // TODO: better error
                    .into_report()
                    .attach_printable_lazy(|| {
                        format!(
                            "Failed to parse manifest file. Manifest file path: {}",
                            manifest.path.display()
                        )
                    })
                    .change_context(MainError)?;

                Ok(manifest)
            }
            None => generate_manifest(
                &smiths,
                &mut generations,
                &config,
                data_path,
                &generation_path,
                config_hash,
            ),
        }
    } else {
        generate_manifest(
            &smiths,
            &mut GenerationsFile::new(),
            &config,
            data_path,
            &generation_path,
            config_hash,
        )
    }?;

    info!("Manifest loaded, creating packages");

    manifest
        .plugins
        .par_iter()
        .map(|plugin| load_plugin(&smiths, plugin, &manifest, data_path))
        .collect::<Result<_, _>>()?;

    Ok(())
}

fn load_plugin(
    smiths: &[Box<dyn DynSmith>],
    plugin: &Plugin,
    manifest: &Manifest,
    data_path: &PathBuf,
) -> Result<(), MainError> {
    for dep in &plugin.dependencies {
        let dep = manifest
            .plugins
            .iter()
            .find(|p| &p.name == dep)
            .ok_or(MainError)
            .into_report()
            .attach_printable_lazy(|| {
                format!("Failed to find dependency. Dependency name: {dep}")
            })?;

        load_plugin(smiths, dep, manifest, data_path)?;
    }

    let smith = smiths
        .iter()
        .find(|s| s.name() == plugin.smith)
        .ok_or(MainError)
        .into_report()
        .attach_printable_lazy(|| format!("Failed to find smith. Smith name: {}", plugin.smith))?;
    let mut package_path = data_path.clone();

    if plugin.optional {
        package_path = package_path.join("opt");
    } else {
        package_path = package_path.join("start");
    }

    if let Some(rename) = &plugin.rename {
        package_path = package_path.join(rename);
    } else {
        package_path = package_path.join(&plugin.name);
    }

    smith
        .load_dyn(plugin.loader_data.as_ref(), &package_path)
        .attach_printable_lazy(|| {
            format!(
                "Failed to load package. Package name: {}, Package path: {}",
                plugin.name,
                package_path.display()
            )
        })
        .change_context(MainError)?;

    // run the build script if it exists
    if !plugin.build.is_empty() {
        let mut split = plugin.build.split_whitespace().collect::<Vec<_>>();

        let command = Command::new(split.remove(0))
            .args(split)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&package_path)
            .spawn()
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "Failed to run build script. Package name: {}, Package path: {}",
                    plugin.name,
                    package_path.display()
                )
            })
            .change_context(MainError)?;

        let stdout = command
            .stdout
            .ok_or(MainError)
            .into_report()
            .change_context(MainError)?;

        let stderr = command
            .stderr
            .ok_or(MainError)
            .into_report()
            .change_context(MainError)?;

        let stdout = BufReader::new(stdout);
        let stderr = BufReader::new(stderr);

        thread::scope(move |threads| -> Result<(), MainError> {
            let stdout = threads.spawn(move || -> Result<(), MainError> {
                for line in stdout.lines() {
                    let line = line.into_report().change_context(MainError)?;
                    info!("STDOUT {}: {}", plugin.name, line);
                }

                Ok(())
            });

            let stderr = threads.spawn(move || -> Result<(), MainError> {
                for line in stderr.lines() {
                    let line = line.into_report().change_context(MainError)?;
                    warn!("STDOUT {}: {}", plugin.name, line);
                }

                Ok(())
            });

            stdout.join().unwrap()?;
            stderr.join().unwrap()?;

            Ok(())
        })?;
    }

    Ok(())
}

fn generate_manifest(
    smiths: &[Box<dyn DynSmith>],
    generations: &mut GenerationsFile,
    config: &Config,
    data_path: &Path,
    generation_path: &Path,
    config_hash: u64,
) -> Result<Manifest, MainError> {
    info!("No generation found with the same hash as the current config, creating new generation");

    // get the latest generation number, and increment it
    let generation = generations.get_next_generation_number(config_hash);

    // compute the hash of the generation
    let mut generation_hash = DefaultHasher::new();
    config.hash(&mut generation_hash);
    generation.hash(&mut generation_hash);
    let generation_hash = generation_hash.finish();

    // create the manifest
    let manifest = create_manifest_from_config(smiths, config)?;

    info!("Saving generation file");
    let manifest_path = data_path.join(format!("manifest-{}.rkyv", &generation_hash));
    manifest
        .save_to_file(&manifest_path)
        .into_report()
        .attach_printable_lazy(|| {
            format!(
                "Failed to save manifest file. Manifest file path: {}",
                manifest_path.display()
            )
        })
        .change_context(MainError)?;

    info!("Saving generations file");

    generations.add_to_generation(config_hash, manifest_path);
    generations
        .save_to_file(&generation_path.to_path_buf())
        .into_report()
        .attach_printable_lazy(|| {
            format!(
                "Failed to save generations file. Generations file path: {}",
                generation_path.display()
            )
        })
        .change_context(MainError)?;

    Ok(manifest)
}

fn create_manifest_from_config(
    smiths: &[Box<dyn DynSmith>],
    config: &Config,
) -> Result<Manifest, MainError> {
    let packages = config
        .create_package_list(smiths)
        .attach_printable_lazy(|| "Failed to create package list")
        .change_context(MainError)?;

    info!("packages created, resolving");
    let resolved_packages = packages
        .par_iter()
        .map(|package| {
            let loader_data = package.resolve(smiths).change_context(MainError)?;

            Ok((loader_data, package))
        })
        .collect::<Result<Vec<_>, _>>()?;

    info!("resolved, saving manifest");

    let plugins = resolved_packages
        .into_iter()
        .map(|(loader_data, package)| {
            let smith = smiths
                .iter()
                .find(|s| s.name() == package.smith)
                .ok_or(MainError)
                .into_report()
                .attach_printable_lazy(|| {
                    format!("Failed to find smith. Smith name: {}", package.smith)
                })?;

            let plugin = Plugin {
                name: smith
                    .get_package_name(&package.package.name)
                    .ok_or(MainError)
                    .into_report()
                    .attach_printable_lazy(|| {
                        format!(
                            "Failed to get package name. Package name: {}",
                            package.package.name
                        )
                    })?,
                rename: package.package.package.rename.clone(),
                optional: package.package.package.optional.unwrap_or(false),
                dependencies: package
                    .package
                    .package
                    .dependencies
                    .clone()
                    .unwrap_or_default()
                    .keys()
                    .cloned()
                    .collect(),
                build: package.package.package.build.clone().unwrap_or_default(),
                smith: package.smith.clone(),
                loader_data,
            };

            Ok(plugin)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let manifest = Manifest {
        neovim_version: "0.9.0".to_string(),
        plugins,
    };

    Ok(manifest)
}
