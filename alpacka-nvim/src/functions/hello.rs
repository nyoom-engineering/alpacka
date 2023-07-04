use mlua::prelude::*;

pub fn hello(_: &Lua, _: ()) -> LuaResult<()> {
    println!("Hello from alpacka!");
    Ok(())
}
