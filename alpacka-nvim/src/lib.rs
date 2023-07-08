#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use alpacka::manifest::{ArchivedGenerationsFile, GenerationsFile};
use bytecheck::TupleStructCheckError;
use mlua::prelude::*;
use rkyv::{
    check_archived_root,
    validation::{validators::DefaultValidatorError, CheckArchiveError},
};
mod functions;

#[mlua::lua_module]
fn alpacka_core(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("hello", lua.create_function(functions::hello)?)?;
    exports.set(
        "install",
        lua.create_function(functions::install::from_config)?,
    )?;

    Ok(exports)
}

pub(crate) fn get_generations_from_file(
    generations_file: &[u8],
) -> Result<&ArchivedGenerationsFile, CheckArchiveError<TupleStructCheckError, DefaultValidatorError>>
{
    check_archived_root::<GenerationsFile>(generations_file)
}
