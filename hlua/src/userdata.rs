use std::{
    any::{Any, TypeId},
    marker::PhantomData,
    mem,
    ops::{Deref, DerefMut},
    ptr::{self, addr_of},
};

use crate::{AsLua, AsMutLua, InsideCallback, LuaContext, LuaRead, LuaTable, Push, PushGuard};

// Lua and LuaJIT both ensure 8-byte alignment for all userdata.
// If this changes in the future, we will need to read unaligned data or otherwise work around it.
struct RawUserdata<T> {
    typeid: TypeId,
    data: Box<T>,
}

impl<T: 'static> RawUserdata<T> {
    #[inline(always)]
    fn new(data: Box<T>) -> RawUserdata<T> {
        RawUserdata { typeid: TypeId::of::<T>(), data }
    }
}

// Called when an object inside Lua that requires Drop is being dropped.
#[inline]
extern "C" fn destructor_wrapper<T>(lua: *mut ffi::lua_State) -> libc::c_int {
    unsafe {
        ptr::drop_in_place(ffi::lua_touserdata(lua, -1).cast::<RawUserdata<T>>());
        0
    }
}

// This might not be required?
pub struct OpaqueLua<'lua>(LuaContext, PhantomData<&'lua ()>);
unsafe impl<'lua> AsLua<'lua> for OpaqueLua<'lua> {
    #[inline]
    fn as_lua(&self) -> LuaContext {
        self.0
    }
}
unsafe impl<'lua> AsMutLua<'lua> for OpaqueLua<'lua> {
    #[inline]
    fn as_mut_lua(&mut self) -> LuaContext {
        self.0
    }
}

/// Pushes an object as a user data.
///
/// In Lua, a user data is anything that is not recognized by Lua. When the script attempts to
/// copy a user data, instead only a reference to the data is copied.
///
/// The way a Lua script can use the user data depends on the content of the **metatable**, which
/// is a Lua table linked to the object.
///
/// [See this link for more infos.](http://www.lua.org/manual/5.2/manual.html#2.4)
///
/// # About the Drop trait
///
/// When the Lua context detects that a userdata is no longer needed it calls the function at the
/// `__gc` index in the userdata's metatable, if any. The hlua library will automatically fill this
/// index with a function that invokes the `Drop` trait of the userdata.
///
/// You can replace the function if you wish so, although you are strongly discouraged to do it.
/// It is no unsafe to leak data in Rust, so there is no safety issue in doing so.
///
/// # Arguments
///
///  - `metatable`: Function that fills the metatable of the object.
///
#[inline]
pub fn push_userdata<'lua, L, T, F>(data: T, mut lua: L, metatable: F) -> PushGuard<L>
where
    F: FnOnce(LuaTable<OpaqueLua<'lua>>),
    L: AsMutLua<'lua>,
    T: Send + Any + 'static,
{
    /// This allows the compiler to not instantiate the entire function once
    /// for each different `L` that might call the outer function.
    #[inline(never)]
    unsafe fn inner<'lua, T, F>(data: T, mut lua: LuaContext, metatable: F)
    where
        F: FnOnce(LuaTable<OpaqueLua<'lua>>),
        T: Send + Any + 'static,
    {
        let raw_lua = lua.as_mut_lua();
        let lua_data = {
            let size = mem::size_of::<RawUserdata<T>>();
            ffi::lua_newuserdata(raw_lua.as_ptr(), size as libc::size_t).cast::<RawUserdata<T>>()
        };

        // We check the alignment requirements.
        debug_assert_eq!(lua_data as usize % mem::align_of::<RawUserdata<T>>(), 0);

        // We write the `RawUserdata` block.
        ptr::write(lua_data, RawUserdata::new(Box::new(data)));

        // Get TypeId of T.
        let typeid = TypeId::of::<T>();
        let typeid_ptr = addr_of!(typeid).cast();
        let typeid_size = std::mem::size_of::<TypeId>();

        // Get the metatable if one already exist
        ffi::lua_pushlstring(raw_lua.as_ptr(), typeid_ptr, typeid_size);
        ffi::lua_rawget(raw_lua.as_ptr(), ffi::LUA_REGISTRYINDEX);

        //| -2 userdata (data: T)
        //| -1 nil | table (metatable)
        if ffi::lua_isnil(raw_lua.as_ptr(), -1) {
            //| -2 userdata (data: T)
            //| -1 nil

            // Creating and registering the type T's metatable.
            {
                ffi::lua_pop(raw_lua.as_ptr(), 1);
                //| -1 userdata (data: T)
                ffi::lua_createtable(raw_lua.as_ptr(), 0, mem::needs_drop::<T>() as i32);
                //| -2 userdata (data: T)
                //| -1 table (metatable)
                ffi::lua_pushlstring(raw_lua.as_ptr(), typeid_ptr, typeid_size);
                //| -3 userdata (data: T)
                //| -2 table (metatable)
                //| -1 string (typeid)
                ffi::lua_pushvalue(raw_lua.as_ptr(), -2);
                //| -4 userdata (data: T)
                //| -3 table (metatable)
                //| -2 string (typeid)
                //| -1 table (metatable)
                ffi::lua_rawset(raw_lua.as_ptr(), ffi::LUA_REGISTRYINDEX);
                //| -2 userdata (data: T)
                //| -1 table (metatable)
            }

            // Index "__gc" in the metatable calls the object's destructor.
            // Only assign it if the type T needs to be explicitly dropped.
            if mem::needs_drop::<T>() {
                "__gc".push_no_err(raw_lua).forget();
                //| -3 userdata (data: T)
                //| -2 table (metatable)
                //| -1 string ("__gc")
                ffi::lua_pushcfunction(raw_lua.as_ptr(), Some(destructor_wrapper::<T>));
                //| -4 userdata (data: T)
                //| -3 table (metatable)
                //| -2 string ("__gc")
                //| -1 cfunction (destructor_wrapper::<T>)
                ffi::lua_rawset(raw_lua.as_ptr(), -3);
                //| -2 userdata (data: T)
                //| -1 table (metatable)
            }

            // Calling the metatable closure.
            {
                let mut guard = PushGuard::new(raw_lua, 1);
                let mtl = OpaqueLua(guard.as_mut_lua(), PhantomData);
                metatable(LuaRead::lua_read(mtl).ok().unwrap());
                guard.forget();
            }
        }
        //| -2 userdata (data: T)
        //| -1 table (metatable)

        ffi::lua_setmetatable(raw_lua.as_ptr(), -2);
        //| -1 userdata (data: T)
    }

    let raw_lua = lua.as_mut_lua();
    unsafe { inner(data, raw_lua, metatable) };
    PushGuard { lua, size: 1, raw_lua }
}

///
#[inline]
pub fn read_userdata<'t, 'c, T>(
    lua: &'c mut InsideCallback,
    index: i32,
) -> Result<&'t mut T, &'c mut InsideCallback>
where
    T: 'static + Any,
{
    unsafe {
        let ptr = ffi::lua_touserdata(lua.as_lua().as_ptr(), index);
        match ptr.cast::<RawUserdata<T>>().as_mut() {
            Some(ud) if ud.typeid == TypeId::of::<T>() => Ok(Box::deref_mut(&mut ud.data)),
            _ => Err(lua),
        }
    }
}

/// Represents a user data located inside the Lua context.
#[derive(Debug)]
pub struct UserdataOnStack<T, L> {
    variable: L,
    index: i32,
    marker: PhantomData<T>,
}

impl<'lua, T, L> LuaRead<L> for UserdataOnStack<T, L>
where
    L: AsMutLua<'lua>,
    T: 'lua + Any,
{
    #[inline]
    fn lua_read_at_position(lua: L, index: i32) -> Result<UserdataOnStack<T, L>, L> {
        unsafe {
            let ptr = ffi::lua_touserdata(lua.as_lua().as_ptr(), index);
            match ptr.cast::<RawUserdata<T>>().as_mut() {
                Some(ud) if ud.typeid == TypeId::of::<T>() => {
                    Ok(UserdataOnStack { variable: lua, index, marker: PhantomData })
                },
                _ => Err(lua),
            }
        }
    }
}

unsafe impl<'lua, T, L> AsLua<'lua> for UserdataOnStack<T, L>
where
    L: AsLua<'lua>,
    T: 'lua + Any,
{
    #[inline]
    fn as_lua(&self) -> LuaContext {
        self.variable.as_lua()
    }
}

unsafe impl<'lua, T, L> AsMutLua<'lua> for UserdataOnStack<T, L>
where
    L: AsMutLua<'lua>,
    T: 'lua + Any,
{
    #[inline]
    fn as_mut_lua(&mut self) -> LuaContext {
        self.variable.as_mut_lua()
    }
}

impl<'lua, T, L> Deref for UserdataOnStack<T, L>
where
    L: AsLua<'lua>,
    T: 'lua + Any,
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        unsafe {
            let ptr = ffi::lua_touserdata(self.variable.as_lua().as_ptr(), self.index);
            &(*ptr.cast::<RawUserdata<T>>()).data
        }
    }
}

impl<'lua, T, L> DerefMut for UserdataOnStack<T, L>
where
    L: AsMutLua<'lua>,
    T: 'lua + Any,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        unsafe {
            let ptr = ffi::lua_touserdata(self.variable.as_lua().as_ptr(), self.index);
            &mut (*ptr.cast::<RawUserdata<T>>()).data
        }
    }
}
