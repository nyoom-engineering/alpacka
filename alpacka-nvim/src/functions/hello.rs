use mlua::prelude::*;

#[allow(clippy::unnecessary_wraps)]
pub fn hello(_: &Lua, _: ()) -> LuaResult<()> {
    println!("Hello from alpacka!");
    Ok(())
}
