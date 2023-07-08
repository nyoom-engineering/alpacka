use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    fs::File,
    hash::{Hash, Hasher},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{ChildStderr, ChildStdout, Command, Stdio},
    thread::{self, Scope},
};

use alpacka::{
    config::Config,
    manifest::{
        add_to_generations, get_latest, ArchivedGenerationsFile, GenerationsFile, Manifest, Plugin,
    },
    package::{Config as PackageConfig, Package, WithSmith},
    smith::{enums::Loader, Git},
};
use mlua::prelude::*;
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rkyv::{to_bytes, Deserialize, Infallible};

use crate::get_generations_from_file;

#[allow(clippy::needless_pass_by_value)]
pub fn from_config(lua: &Lua, config: LuaTable<'_>) -> LuaResult<()> {
    let git = Loader::Git(Git::default());
    let smiths = &[git];

    let plugin_install_path = PathBuf::from(config.get::<_, String>("rtp_install_path")?);
    let generation_path = PathBuf::from(config.get::<_, String>("generation_path")?);
    let config: Config = lua.from_value(config.get("config")?)?;

    let config_hash = {
        let mut hasher = DefaultHasher::new();
        config.hash(&mut hasher);
        hasher.finish()
    };

    let manifest = match std::fs::read(&generation_path) {
        Ok(file) => {
            let generations = get_generations_from_file(&file)
                .map_err(|e| e.to_string())
                .to_lua_err()?;

            // find generation that have the same hash as the current config, and the highest generation
            get_latest(generations, config_hash)
                .map_or_else(
                    || {
                        create_manifest_from_config(
                            smiths,
                            &config,
                            &generation_path,
                            Some(generations),
                        )
                    },
                    |manifest| {
                        let manifest: Manifest = manifest.deserialize(&mut Infallible).unwrap();
                        Ok(manifest)
                    },
                )
                .to_lua_err()?
        }
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {
                create_manifest_from_config(smiths, &config, &generation_path, None)?
            }
            _ => return Err(e.to_lua_err()),
        },
    };

    manifest
        .plugins
        .par_iter()
        .map(|plugin| load_plugin(smiths, plugin, &plugin_install_path))
        .collect::<Result<_, _>>()?;

    Ok(())
}

fn create_manifest_from_config(
    smiths: &[Loader],
    config: &Config,
    generations_path: &Path,
    generations: Option<&ArchivedGenerationsFile>,
) -> LuaResult<Manifest> {
    let packages = config.create_package_list(smiths).to_lua_err()?;

    let resolved_packages = packages
        .into_par_iter()
        .map(|package| package.resolve_recurse(smiths))
        .collect::<Result<Vec<_>, _>>()
        .to_lua_err()?
        .into_par_iter()
        .flatten();

    let plugins: Vec<_> = resolved_packages
        .map(|(loader_data, package)| {
            let smith = smiths
                .iter()
                .find(|s| s.name() == package.smith)
                .expect("To be able to find smith");

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
                    .expect("To be able to get package name"),
                unresolved_name: name.to_string(),
                rename: rename.clone(),
                optional: optional.unwrap_or(false),
                dependencies: dependencies.keys().cloned().collect(),
                build: build.clone().unwrap_or_default(),
                smith: smith_to_use,
                loader_data,
            };

            plugin
        })
        .collect();

    let manifest = Manifest {
        neovim_version: "0.9.0".to_string(),
        plugins,
    };

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
    let mut file = File::create(generations_path).to_lua_err()?;

    let bytes = to_bytes::<_, 1024>(&new_generations_file).to_lua_err()?;

    file.write_all(&bytes).to_lua_err()?;

    let manifest = new_generations_file
        .0
        .into_iter()
        .last()
        .expect("To be able to get manifest in generations file");

    Ok(manifest.1)
}

fn load_plugin(smiths: &[Loader], plugin: &Plugin, data_path: &Path) -> LuaResult<()> {
    let smith = smiths
        .iter()
        .find(|s| s.name() == plugin.smith)
        .expect("To be able to find smith");

    let package_path = data_path
        .join(if plugin.optional { "opt" } else { "start" })
        .join(plugin.rename.as_ref().unwrap_or(&plugin.name));

    smith
        .load(&plugin.loader_data, &package_path)
        .to_lua_err()?;

    let build_script_exists = !plugin.build.is_empty();
    if build_script_exists {
        let mut build_arguments = plugin.build.split_whitespace();

        let build_command = build_arguments
            .next()
            .expect("To be able to get build command.");

        let command = Command::new(build_command)
            .args(build_arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&package_path)
            .spawn()
            .to_lua_err()?;

        let stdout = command.stdout.expect("To be able to get stdout");

        let stderr = command.stderr.expect("To be able to get stderr");

        let stdout = BufReader::new(stdout);
        let stderr = BufReader::new(stderr);

        thread::scope(move |threads| create_stdio_readers(&plugin.name, stdout, stderr, threads))?;
    }

    Ok(())
}

fn create_stdio_readers<'a>(
    _plugin_name: &'a str,
    stdout: BufReader<ChildStdout>,
    stderr: BufReader<ChildStderr>,
    threads: &'a Scope<'a, '_>,
) -> LuaResult<()> {
    let stdout = threads.spawn(move || -> LuaResult<()> {
        for line in stdout.lines() {
            let _line = line.to_lua_err()?;
        }

        Ok(())
    });

    let stderr = threads.spawn(move || -> LuaResult<()> {
        for line in stderr.lines() {
            let _line = line.to_lua_err()?;
        }

        Ok(())
    });

    stdout.join().unwrap()?;
    stderr.join().unwrap()?;

    Ok(())
}
