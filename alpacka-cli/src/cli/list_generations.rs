use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter},
    iter::once,
    path::Path,
};

use error_stack::{ensure, Context, IntoReport, Result, ResultExt};
use rkyv::{Deserialize, Infallible};
use tracing::{error, info};

use alpacka::manifest::{GenerationsFile, Json, JsonGenerationsFile, Manifest};

use crate::cli::get_generations_from_file;

use super::clap::ListGenerationsFormatMethod;

#[derive(Debug)]
pub enum Error {
    LoadError,
    FormattingError,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::LoadError => "Failed to load alpacka",
            Self::FormattingError => "Failed to format output",
        })
    }
}

impl Context for Error {}

/// List all generations in the generations file.
///
/// # Errors
/// Errors if the generations file does not exist, or if the generations file is invalid.
///
/// # Panics
/// Panics if the generations file is invalid.
pub fn list_generations(
    data_path: &Path,
    format_style: &ListGenerationsFormatMethod,
) -> Result<(), Error> {
    let generations_path = data_path.join("generations.rkyv");

    ensure!(generations_path.exists(), {
        error!("Generations file path does not exist. Aborting");
        Error::LoadError
    });

    let generations_file = std::fs::read(&generations_path)
        .into_report()
        .attach_printable_lazy(|| {
            format!(
                "Failed to read generations file. Generations file path: {}",
                generations_path.display()
            )
        })
        .change_context(Error::LoadError)?;

    let generations = get_generations_from_file(&generations_file)
        .map_err(|_e| {
            error!(
                "Failed to load generations file. Generations file path: {}",
                generations_path.display()
            );
            Error::LoadError
        })
        .into_report()?;

    match format_style {
        ListGenerationsFormatMethod::Human => {
            for (idx, (hash, manifest)) in generations.0.iter().enumerate() {
                let hashed_config_file = hash.0;
                let generation_number = hash.1;

                // TODO: use the manifest for output
                let _manifest: Manifest = manifest.deserialize(&mut Infallible).unwrap();

                info!("Manifest number {idx} | Hash {hashed_config_file} | generation {generation_number}");
            }
        }
        ListGenerationsFormatMethod::Json => {
            let deserialized: GenerationsFile = generations.deserialize(&mut Infallible).unwrap();

            // this is possibly the most cursed solution to this
            let json = deserialized.0.into_iter().fold(
                JsonGenerationsFile(BTreeMap::new()),
                |current, (hash, manifest)| {
                    let new_map = current
                        .0
                        .into_iter()
                        .chain(once((
                            hash.0.to_string(),
                            Json {
                                hash: hash.0.to_string(),
                                generation: hash.1.to_string(),
                                neovim_version: manifest.neovim_version,
                                plugins: manifest.plugins,
                            },
                        )))
                        .collect();

                    JsonGenerationsFile(new_map)
                },
            );

            let json = serde_json::to_string(&json)
                .into_report()
                .attach_printable_lazy(|| {
                    format!(
                        "Failed to convert generation file {} into JSON",
                        generations_path.display()
                    )
                })
                .change_context(Error::FormattingError)?;

            println!("{json}");
        }
    }

    Ok(())
}
