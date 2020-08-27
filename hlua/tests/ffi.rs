//! Test to make sure the low-level API can be accessed from hlua.

extern crate hlua;

use hlua::AsLua;

#[test]
fn get_version() {
    let lua = hlua::Lua::new();
    let state_ptr = lua.as_lua().as_ptr();

    let version = unsafe { hlua::ffi::lua_version(state_ptr) };
    
    #[cfg(feature = "lua52")] assert_eq!(502.0, unsafe { *version });
    #[cfg(feature = "lua54")] assert_eq!(504.0, version);
}
