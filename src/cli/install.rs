use crate::{
    cli::get_generations_from_file,
    config::Config,
    manifest::{
        add_to_generations, get_latest, ArchivedGenerationsFile, GenerationsFile, Manifest, Plugin,
    },
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
    process::{Command, Stdio},
    thread,
};
use tracing::{debug, info, warn};

#[derive(Debug)]
pub enum Error {
    LoadError,
    GenerationFetchError,
    LoadManifestError,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::LoadError => "Failed to load alpacka",
            Self::GenerationFetchError => "Failed to fetch generations",
            Self::LoadManifestError => "Failed to load manifest",
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
            .change_context(Error::LoadError)?;
    }

    load_alpacka(data_path, config_path)?;

    Ok(())
}

fn load_alpacka(data_path: &Path, config_path: PathBuf) -> Result<(), Error> {
    let config_file = std::fs::File::open(config_path)
        .into_report()
        .attach_printable_lazy(|| "Failed to open config file".to_string())
        .change_context(Error::LoadError)?;

    let config: Config = serde_json::from_reader(config_file)
        .into_report()
        .attach_printable_lazy(|| "Failed to parse config file".to_string())
        .change_context(Error::LoadError)?;

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
            .change_context(Error::LoadError)?;

        let generations = get_generations_from_file(&generations_file)
            .map_err(|_| Error::LoadError)
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
        .change_context(Error::LoadManifestError)?;

    info!("packages created, resolving");
    let resolved_packages = packages
        .into_par_iter()
        .map(|package| package.resolve_recurse(smiths))
        .collect::<Result<Vec<_>, _>>()
        .change_context(Error::LoadManifestError)
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
                .ok_or(Error::LoadManifestError)
                .into_report()
                .attach_printable_lazy(|| {
                    format!("Failed to find smith. Smith name: {}", package.smith)
                })?;

            let plugin = Plugin {
                name: smith
                    .get_package_name(&package.package.name)
                    .ok_or(Error::LoadManifestError)
                    .into_report()
                    .attach_printable_lazy(|| {
                        format!(
                            "Failed to get package name. Package name: {}",
                            package.package.name
                        )
                    })?,
                unresolved_name: package.package.name,
                rename: package.package.config_package.rename.clone(),
                optional: package.package.config_package.optional.unwrap_or(false),
                dependencies: package
                    .package
                    .config_package
                    .dependencies
                    .clone()
                    .keys()
                    .cloned()
                    .collect(),
                build: package
                    .package
                    .config_package
                    .build
                    .clone()
                    .unwrap_or_default(),
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
        .change_context(Error::LoadError)?;

    let bytes = to_bytes::<_, 1024>(&new_generations_file)
        .into_report()
        .attach_printable_lazy(|| "Failed to serialize generations file")
        .change_context(Error::LoadError)?;

    file.write_all(&bytes)
        .into_report()
        .attach_printable_lazy(|| {
            format!(
                "Failed to write to generations file. Path: {}",
                generations_path.display()
            )
        })
        .change_context(Error::LoadError)?;

    info!("generations file saved, getting latest manifest");

    let manifest = new_generations_file
        .0
        .into_iter()
        .last()
        .ok_or(Error::LoadError)
        .into_report()
        .attach_printable_lazy(|| "Failed to get latest manifest")
        .change_context(Error::LoadManifestError)?;

    Ok(manifest.1)
}

#[tracing::instrument]
fn load_plugin(smiths: &[Loaders], plugin: &Plugin, data_path: &Path) -> Result<(), Error> {
    let smith = smiths
        .iter()
        .find(|s| s.name() == plugin.smith)
        .ok_or(Error::LoadManifestError)
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
        .change_context(Error::LoadManifestError)?;

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
            .change_context(Error::LoadManifestError)?;

        let stdout = command
            .stdout
            .ok_or(Error::LoadManifestError)
            .into_report()?;

        let stderr = command
            .stderr
            .ok_or(Error::LoadManifestError)
            .into_report()?;

        let stdout = BufReader::new(stdout);
        let stderr = BufReader::new(stderr);

        thread::scope(move |threads| -> Result<(), Error> {
            let stdout = threads.spawn(move || -> Result<(), Error> {
                for line in stdout.lines() {
                    let line = line.into_report().change_context(Error::LoadError)?;
                    info!("STDOUT from {}: {}", plugin.name, line);
                }

                Ok(())
            });

            let stderr = threads.spawn(move || -> Result<(), Error> {
                for line in stderr.lines() {
                    let line = line.into_report().change_context(Error::LoadError)?;
                    warn!("STDERR from {}: {}", plugin.name, line);
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
