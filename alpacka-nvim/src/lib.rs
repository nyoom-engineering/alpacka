use mlua::prelude::*;
mod functions;

#[mlua::lua_module]
fn alpacka(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    exports.set("hello", lua.create_function(functions::hello)?)?;

    Ok(exports)
}
