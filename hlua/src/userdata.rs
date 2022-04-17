use std::{
    any::{Any, TypeId},
    marker::PhantomData,
    mem,
    ops::{Deref, DerefMut},
    ptr::{addr_of, NonNull},
};

use crate::{AsLua, AsMutLua, InsideCallback, LuaContext, LuaRead, LuaTable, Push, PushGuard};

mod raw {
    use std::{
        any::TypeId,
        mem::{align_of, size_of},
        ptr::{self, NonNull},
    };

    use libc::c_void;

    use crate::LuaContext;

    // All supported versions of Lua ensure 8-byte alignment for all userdata allocations.
    // If this changes in the future we'll need to read unaligned data or otherwise work around it.
    // If you're not sure what this should be set to or can't guarantee any alignment, set it to 0.
    const LUA_ALLOCATOR_ALIGNMENT: usize = 8;

    /// Pushes a userdata value onto the Lua stack.
    #[inline(always)]
    pub unsafe fn push<T: 'static>(data: T, lua: LuaContext) {
        let pad = match is_zst::<T>() {
            true => 0, // We don't need to actually store ZSTs, so we don't need any padding
            false => align_of::<T>().saturating_sub(LUA_ALLOCATOR_ALIGNMENT),
        };

        let len = size_of::<TypeId>() + size_of::<T>() + pad;
        let mem = ffi::lua_newuserdata(lua.as_ptr(), len);
        let tid = mem.cast::<TypeId>();

        // Ensure that alignment matches
        debug_assert_eq!(mem as usize % LUA_ALLOCATOR_ALIGNMENT, 0);
        debug_assert_eq!(tid as usize % align_of::<TypeId>(), 0);

        // We always want to write the TypeId
        std::ptr::write(tid, TypeId::of::<T>());

        // If we're holding ZST data there's no reason to actually save it, since it's zero-sized
        // This matters since if we *do* save it it'll not necessarily have the correct alignment
        if !is_zst::<T>() {
            let ptr = data_ptr::<T>(mem);

            // Ensure that alignment matches
            debug_assert_eq!(ptr as usize % align_of::<T>(), 0);
            std::ptr::write(ptr.cast(), data);
        }
    }

    /// Drops the inner data.
    pub unsafe fn drop_in_place<T>(ptr: *mut c_void) {
        ptr::drop_in_place(data_ptr::<T>(ptr));
    }

    /// Reads a [`TypeId`] from the pointer and ensures it matches `T`.
    pub unsafe fn validate_type_id<T: 'static>(ptr: *mut c_void) -> bool {
        *ptr.cast::<TypeId>() == TypeId::of::<T>()
    }

    /// Returns a pointer to the inner data.
    pub unsafe fn data_ptr<T>(ptr: *mut c_void) -> *mut T {
        if is_zst::<T>() {
            // We don't actually need to do this, we can use the same logic as below for ZSTs, but
            // there is no reason to potentially deal with extra masking when we can just create a
            // valid pointer without any extra work
            return NonNull::dangling().as_ptr();
        }

        // Skip past the TypeId
        let data = ptr.cast::<TypeId>().add(1);

        // The check for align > LUA_ALLOCATOR_ALIGNMENT should always be resolved at compile-time
        let align = align_of::<T>();
        match align > LUA_ALLOCATOR_ALIGNMENT {
            // Skip past the padding
            // "Alignment is measured in bytes, and must be at least 1, and always a power of 2"
            true => ((data as usize + align - 1) & !(align - 1)) as *mut _,
            false => data.cast(),
        }
    }

    /// Returns a reference to the inner data.
    pub unsafe fn data_ref<'a, T>(ptr: *mut c_void) -> &'a T {
        &*data_ptr::<T>(ptr)
    }

    /// Returns a mutable reference to the inner data.
    pub unsafe fn data_mut<'a, T>(ptr: *mut c_void) -> &'a mut T {
        &mut *data_ptr::<T>(ptr)
    }

    /// Returns a mutable reference to the inner data.
    ///
    /// This also checks so that the pointer is not null and validates that the type matches.
    /// If you know that the pointer is valid, you can use [`data_mut`] instead.
    pub unsafe fn data_mut_checked<'a, T: 'static>(ptr: *mut c_void) -> Option<&'a mut T> {
        (!ptr.is_null() && validate_type_id::<T>(ptr)).then(|| data_mut::<T>(ptr))
    }

    /// Returns whether [`T`] is a zero-sized struct.
    const fn is_zst<T>() -> bool {
        size_of::<T>() == 0
    }
}

// Called when an object inside Lua that requires Drop is being dropped.
#[inline]
extern "C" fn destructor_wrapper<T: 'static>(lua: *mut ffi::lua_State) -> libc::c_int {
    unsafe {
        raw::drop_in_place::<T>(ffi::lua_touserdata(lua, -1));
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
        #[cold]
        unsafe fn create_metatable<'lua, T, F>(
            raw_lua: LuaContext,
            metatable: F,
            tid_ptr: *const i8,
            tid_len: usize,
        ) where
            F: FnOnce(LuaTable<OpaqueLua<'lua>>),
            T: Send + Any + 'static,
        {
            // Create and register a metatable for T.
            ffi::lua_pop(raw_lua.as_ptr(), 1);
            ffi::lua_createtable(raw_lua.as_ptr(), 0, mem::needs_drop::<T>() as i32);
            ffi::lua_pushlstring(raw_lua.as_ptr(), tid_ptr, tid_len);
            ffi::lua_pushvalue(raw_lua.as_ptr(), -2);
            ffi::lua_rawset(raw_lua.as_ptr(), ffi::LUA_REGISTRYINDEX);

            // Only assign "__gc" if T needs to be dropped.
            if mem::needs_drop::<T>() {
                "__gc".push_no_err(raw_lua).forget();
                ffi::lua_pushcfunction(raw_lua.as_ptr(), Some(destructor_wrapper::<T>));
                ffi::lua_rawset(raw_lua.as_ptr(), -3);
            }

            // Calling the metatable closure.
            let mut guard = PushGuard::new(raw_lua, 1);
            let mtl = OpaqueLua(guard.as_mut_lua(), PhantomData);
            metatable(LuaRead::lua_read(mtl).ok().unwrap());
            guard.forget();
        }

        let raw_lua = lua.as_mut_lua();
        raw::push::<T>(data, raw_lua);

        // Get TypeId of T.
        let typeid = TypeId::of::<T>();
        let tid_ptr = addr_of!(typeid).cast();
        let tid_len = std::mem::size_of::<TypeId>();

        // Get the metatable if one already exists.
        ffi::lua_pushlstring(raw_lua.as_ptr(), tid_ptr, tid_len);
        ffi::lua_rawget(raw_lua.as_ptr(), ffi::LUA_REGISTRYINDEX);

        // If no metatable exists, create one.
        if ffi::lua_isnil(raw_lua.as_ptr(), -1) {
            create_metatable::<'_, T, _>(raw_lua, metatable, tid_ptr, tid_len);
        }

        ffi::lua_setmetatable(raw_lua.as_ptr(), -2);
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
        raw::data_mut_checked::<T>(ptr).ok_or(lua)
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
            match NonNull::new(ffi::lua_touserdata(lua.as_lua().as_ptr(), index)) {
                Some(x) if raw::validate_type_id::<T>(x.as_ptr()) => {
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
            raw::data_ref::<T>(ffi::lua_touserdata(self.variable.as_lua().as_ptr(), self.index))
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
            raw::data_mut::<T>(ffi::lua_touserdata(self.variable.as_lua().as_ptr(), self.index))
        }
    }
}
