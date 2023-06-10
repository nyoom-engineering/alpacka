use bytecheck::TupleStructCheckError;
use rkyv::{
    check_archived_root,
    validation::{validators::DefaultValidatorError, CheckArchiveError},
};

use crate::manifest::{ArchivedGenerationsFile, GenerationsFile};

pub mod clap;
pub mod install;
pub mod list_generations;

pub(crate) fn get_generations_from_file(
    generations_file: &[u8],
) -> Result<&ArchivedGenerationsFile, CheckArchiveError<TupleStructCheckError, DefaultValidatorError>>
{
    check_archived_root::<GenerationsFile>(generations_file)
}
