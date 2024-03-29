use alpacka::{
    config::Config,
    manifest::{
        add_to_generations, get_latest, ArchivedGenerationsFile, GenerationsFile, Manifest, Plugin,
    },
    package::{Config as PackageConfig, Package, WithSmith},
    smith::{enums::Loaders, Git},
};
use error_stack::{Context, IntoReport, Result, ResultExt};
use rayon::prelude::*;
use rkyv::{to_bytes, Deserialize, Infallible};
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    fmt::{Display, Formatter},
    fs::File,
    hash::{Hash, Hasher},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{ChildStderr, ChildStdout, Command, Stdio},
    thread::{self, Scope},
};
use tracing::{debug, info, warn};

use crate::cli::get_generations_from_file;

#[derive(Debug)]
pub enum Error {
    Load,
    LoadManifest,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Load => "Failed to load alpacka",
            Self::LoadManifest => "Failed to load manifest",
        })
    }
}

impl Context for Error {}

/// Installs the latest generation of plugins
///
/// # Errors
/// Errors if the config file cannot be opened, or if the generations file cannot be fetched.
/// May also error if a install command cannot be run.
pub fn install(config_path: PathBuf, data_path: &PathBuf) -> Result<(), Error> {
    if !data_path.exists() {
        std::fs::create_dir_all(data_path)
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "Failed to create alpacka directory. Alpacka directory path: {}",
                    data_path.display()
                )
            })
            .change_context(Error::Load)?;
    }

    load_alpacka(data_path, config_path)?;

    Ok(())
}

fn load_alpacka(data_path: &Path, config_path: PathBuf) -> Result<(), Error> {
    let config_file = std::fs::File::open(config_path)
        .into_report()
        .attach_printable_lazy(|| "Failed to open config file".to_string())
        .change_context(Error::Load)?;

    let config: Config = serde_json::from_reader(config_file)
        .into_report()
        .attach_printable_lazy(|| "Failed to parse config file".to_string())
        .change_context(Error::Load)?;

    info!("Config loaded, checking for existing manifest");

    let config_hash = {
        let mut hasher = DefaultHasher::new();
        config.hash(&mut hasher);
        hasher.finish()
    };

    let smiths: Vec<Loaders> = vec![Loaders::Git(Git::new())];
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
            .change_context(Error::Load)?;

        let generations = get_generations_from_file(&generations_file)
            .map_err(|_| Error::Load)
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "Failed to parse generations file. Generations file path: {}",
                    generation_path.display()
                )
            })?;

        // find generation that have the same hash as the current config, and the highest generation
        get_latest(generations, config_hash).map_or_else(
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

#[tracing::instrument(skip(generations))]
fn create_manifest_from_config(
    smiths: &[Loaders],
    config: &Config,
    generations_path: &Path,
    generations: Option<&ArchivedGenerationsFile>,
) -> Result<Manifest, Error> {
    let packages = config
        .create_package_list(smiths)
        .attach_printable_lazy(|| "Failed to create package list")
        .change_context(Error::LoadManifest)?;

    info!("packages created, resolving");
    let resolved_packages = packages
        .into_par_iter()
        .map(|package| package.resolve_recurse(smiths))
        .collect::<Result<Vec<_>, _>>()
        .change_context(Error::LoadManifest)
        .attach_printable_lazy(|| "Failed to resolve packages!")?
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
                .ok_or(Error::LoadManifest)
                .into_report()
                .attach_printable_lazy(|| {
                    format!("Failed to find smith. Smith name: {}", package.smith)
                })?;

            let WithSmith {
                smith: smith_to_use,
                package,
            } = package;

            let Package {
                name,
                config_package,
            } = package;

            let PackageConfig {
                optional,
                build,
                dependencies,
                rename,
                version: _,
            } = config_package;

            let plugin = Plugin {
                name: smith
                    .get_package_name(name)
                    .ok_or(Error::LoadManifest)
                    .into_report()
                    .attach_printable_lazy(|| {
                        format!("Failed to get package name. Package name: {}", package.name)
                    })?,
                unresolved_name: name.to_string(),
                rename: rename.clone(),
                optional: optional.unwrap_or(false),
                dependencies: dependencies.keys().cloned().collect(),
                build: build.clone().unwrap_or_default(),
                smith: smith_to_use,
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

    let hash = {
        let mut hasher = DefaultHasher::new();
        config.hash(&mut hasher);
        hasher.finish()
    };

    let new_generations_file = if let Some(generations) = generations {
        add_to_generations(generations, hash, manifest)
    } else {
        let mut gen_file = GenerationsFile(BTreeMap::new());
        gen_file.add_to_generations(hash, manifest);
        gen_file
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
        .change_context(Error::Load)?;

    let bytes = to_bytes::<_, 1024>(&new_generations_file)
        .into_report()
        .attach_printable_lazy(|| "Failed to serialize generations file")
        .change_context(Error::Load)?;

    file.write_all(&bytes)
        .into_report()
        .attach_printable_lazy(|| {
            format!(
                "Failed to write to generations file. Path: {}",
                generations_path.display()
            )
        })
        .change_context(Error::Load)?;

    info!("generations file saved, getting latest manifest");

    let manifest = new_generations_file
        .0
        .into_iter()
        .last()
        .ok_or(Error::Load)
        .into_report()
        .attach_printable_lazy(|| "Failed to get latest manifest")
        .change_context(Error::LoadManifest)?;

    Ok(manifest.1)
}

#[tracing::instrument]
fn load_plugin(smiths: &[Loaders], plugin: &Plugin, data_path: &Path) -> Result<(), Error> {
    let smith = smiths
        .iter()
        .find(|s| s.name() == plugin.smith)
        .ok_or(Error::LoadManifest)
        .into_report()
        .attach_printable_lazy(|| format!("Failed to find smith. Smith name: {}", plugin.smith))?;

    let package_path = data_path
        .join(if plugin.optional { "opt" } else { "start" })
        .join(plugin.rename.as_ref().unwrap_or(&plugin.name));

    smith
        .load(&plugin.loader_data, &package_path)
        .attach_printable_lazy(|| {
            format!(
                "Failed to load package. Package name: {}, Package path: {}",
                plugin.name,
                package_path.display()
            )
        })
        .change_context(Error::LoadManifest)?;

    let build_script_exists = !plugin.build.is_empty();
    if build_script_exists {
        let mut build_arguments = plugin.build.split_whitespace();

        let build_command = build_arguments
            .next()
            .ok_or(Error::LoadManifest)
            .into_report()
            .attach_printable_lazy(|| {
                format!(
                    "Failed to get build command. You may have an invalid build command. Package name: {}, Package path: {}",
                    plugin.name,
                    package_path.display()
                )
            })
            .change_context(Error::LoadManifest)?;

        let command = Command::new(build_command)
            .args(build_arguments)
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
            .change_context(Error::LoadManifest)?;

        let stdout = command.stdout.ok_or(Error::LoadManifest).into_report()?;

        let stderr = command.stderr.ok_or(Error::LoadManifest).into_report()?;

        let stdout = BufReader::new(stdout);
        let stderr = BufReader::new(stderr);

        thread::scope(move |threads| create_stdio_readers(&plugin.name, stdout, stderr, threads))?;
    }

    Ok(())
}

fn create_stdio_readers<'a>(
    plugin_name: &'a str,
    stdout: BufReader<ChildStdout>,
    stderr: BufReader<ChildStderr>,
    threads: &'a Scope<'a, '_>,
) -> Result<(), Error> {
    let stdout = threads.spawn(move || -> Result<(), Error> {
        for line in stdout.lines() {
            let line = line.into_report().change_context(Error::Load)?;
            info!("STDOUT from build script of {}: {}", plugin_name, line);
        }

        Ok(())
    });

    let stderr = threads.spawn(move || -> Result<(), Error> {
        for line in stderr.lines() {
            let line = line.into_report().change_context(Error::Load)?;
            warn!("STDERR from build script {}: {}", plugin_name, line);
        }

        Ok(())
    });

    stdout.join().unwrap()?;
    stderr.join().unwrap()?;

    Ok(())
}
