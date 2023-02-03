#![allow(clippy::multiple_crate_versions)]

use alpacka::{
    config::Config,
    manifest::{
        add_to_generations, get_latest_manifest, ArchivedGenerationsFile, GenerationsFile,
        Manifest, Plugin,
    },
    package::{Package, WithSmith},
    smith::{DynSmith, Git, SerializeLoaderInput},
};
use error_stack::{Context, IntoReport, Result, ResultExt};
use rayon::prelude::*;
use rkyv::{check_archived_root, to_bytes, Deserialize, Infallible};
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    fmt::{Display, Formatter},
    fs::File,
    hash::{Hash, Hasher},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};
use tracing::{debug, error, info, warn};

#[derive(Debug)]
struct MainError;

impl Display for MainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to run alpacka!")
    }
}

impl Context for MainError {}

fn main() -> error_stack::Result<(), MainError> {
    // Setup logging, with pretty printing
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::Level::INFO.into())
                .from_env_lossy(),
        )
        .init();

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

fn load_alpacka(data_path: &Path, config_path: PathBuf) -> Result<(), MainError> {
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

        let generations = check_archived_root::<GenerationsFile>(&generations_file)
            .map_err(|e| {
                error!("Failed to check generations file: {}", e);
                MainError
            })
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "Failed to check generations file. Generations file path: {}",
                    generation_path.display()
                )
            })?;

        // find generation that have the same hash as the current config, and the highest generation
        get_latest_manifest(generations, config_hash).map_or_else(
            || create_manifest_from_config(&smiths, &config, &generation_path, Some(generations)),
            |manifest| {
                info!(
                    "Found generation with the same hash as the current config, loading manifest"
                );
                let manifest: Manifest = manifest.deserialize(&mut Infallible).unwrap();
                Ok(manifest)
            },
        )
    } else {
        create_manifest_from_config(&smiths, &config, &generation_path, None)
    }?;

    info!("Manifest loaded, creating packages");

    manifest
        .plugins
        .par_iter()
        .map(|plugin| load_plugin(&smiths, plugin, data_path))
        .collect::<Result<_, _>>()?;

    Ok(())
}

fn load_plugin(
    smiths: &[Box<dyn DynSmith>],
    plugin: &Plugin,
    data_path: &Path,
) -> Result<(), MainError> {
    let smith = smiths
        .iter()
        .find(|s| s.name() == plugin.smith)
        .ok_or(MainError)
        .into_report()
        .attach_printable_lazy(|| format!("Failed to find smith. Smith name: {}", plugin.smith))?;

    let package_path = data_path
        .join(if plugin.optional { "opt" } else { "start" })
        .join(plugin.rename.as_ref().unwrap_or(&plugin.name));

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

fn create_manifest_from_config(
    smiths: &[Box<dyn DynSmith>],
    config: &Config,
    generations_path: &Path,
    generations: Option<&ArchivedGenerationsFile>,
) -> Result<Manifest, MainError> {
    let packages = config
        .create_package_list(smiths)
        .attach_printable_lazy(|| "Failed to create package list")
        .change_context(MainError)?;

    info!("packages created, resolving");
    let resolved_packages = packages
        .into_par_iter()
        .map(|package| resolve_package(smiths, package))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    debug!("Resolved packages: {:#?}", resolved_packages);

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
                unresolved_name: package.package.name,
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

    info!("resolved manifest, saving");

    let mut hasher = DefaultHasher::new();
    config.hash(&mut hasher);
    let hash = hasher.finish();

    let new_generations_file = if let Some(generations) = generations {
        add_to_generations(generations, hash, manifest)
    } else {
        GenerationsFile(BTreeMap::new())
    };

    // overwrite the generations file
    let mut file = File::create(generations_path)
        .into_report()
        .attach_printable_lazy(|| {
            format!(
                "Failed to create generations file. Path: {}",
                generations_path.display()
            )
        })
        .change_context(MainError)?;

    let bytes = to_bytes::<_, 1024>(&new_generations_file)
        .into_report()
        .attach_printable_lazy(|| "Failed to serialize generations file")
        .change_context(MainError)?;

    file.write_all(&bytes)
        .into_report()
        .attach_printable_lazy(|| {
            format!(
                "Failed to write to generations file. Path: {}",
                generations_path.display()
            )
        })
        .change_context(MainError)?;

    let manifest = new_generations_file
        .0
        .into_iter()
        .last()
        .ok_or(MainError)
        .into_report()
        .attach_printable_lazy(|| "Failed to get latest manifest")
        .change_context(MainError)?;

    Ok(manifest.1)
}

type PackageWithSerializer = (Box<dyn SerializeLoaderInput>, WithSmith);

fn resolve_package(
    smiths: &[Box<dyn DynSmith>],
    package: WithSmith,
) -> Result<Vec<PackageWithSerializer>, MainError> {
    let mut deps = package.package.package.dependencies.as_ref();
    let none_map = BTreeMap::new();
    if deps.is_none() {
        deps = Some(&none_map);
    }

    let mut deps = deps
        .unwrap()
        .par_iter()
        .map(|dep| {
            let pkg = Package {
                name: dep.0.clone(),
                package: dep.1.clone(),
            };

            let smith_to_use = smiths
                .iter()
                .find(|s| s.get_package_name(&pkg.name).is_some())
                .ok_or(MainError)
                .into_report()
                .attach_printable_lazy(|| {
                    format!("Failed to find smith. Package name: {}", pkg.name)
                })?;

            let package = WithSmith {
                smith: smith_to_use.name(),
                package: pkg,
            };

            resolve_package(smiths, package)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let smith_to_use = smiths
        .iter()
        .find(|s| s.name() == package.smith)
        .ok_or(MainError)
        .into_report()
        .attach_printable_lazy(|| format!("Failed to find smith. Smith name: {}", package.smith))?;

    let loader_data = smith_to_use
        .resolve_dyn(&package.package)
        .attach_printable_lazy(|| {
            format!(
                "Failed to resolve package. Package name: {}",
                package.package.name
            )
        })
        .change_context(MainError)?;

    let final_package = (loader_data, package);
    let mut final_vec = vec![final_package];
    final_vec.append(&mut deps);

    Ok(final_vec)
}
