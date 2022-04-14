/// Thin wrappers around FFI functions
use std::hint::unreachable_unchecked;

use crate::LuaContext;

#[inline(always)]
pub unsafe fn lua_error(l: *mut ffi::lua_State) -> ! {
    ffi::lua_error(l);
    unreachable_unchecked();
}

#[inline(always)]
pub unsafe fn lua_rawlen(lua: LuaContext, index: libc::c_int) -> usize {
    match () {
        #[cfg(feature = "_luaapi_51")]
        () => ffi::lua_objlen(lua.as_ptr(), index),
        #[cfg(feature = "_luaapi_52")]
        () => ffi::lua_rawlen(lua.as_ptr(), index),
        #[cfg(feature = "_luaapi_54")]
        () => ffi::lua_rawlen(lua.as_ptr(), index) as usize,
    }
}
