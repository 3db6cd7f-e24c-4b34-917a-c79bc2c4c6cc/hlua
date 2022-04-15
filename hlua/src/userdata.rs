use std::{
    any::{Any, TypeId},
    marker::PhantomData,
    mem::{self, align_of, size_of},
    ops::{Deref, DerefMut},
    ptr::{self, addr_of, addr_of_mut, NonNull},
};

use libc::c_void;

use crate::{AsLua, AsMutLua, InsideCallback, LuaContext, LuaRead, LuaTable, Push, PushGuard};

// Lua and LuaJIT both ensure 8-byte alignment for all userdata.
// If this changes in the future, we will need to read unaligned data or otherwise work around it.
const LUA_ALLOCATOR_ALIGNMENT: usize = 8;

/// PLEASE do not construct this struct, it is dynamically sized.
#[repr(C)]
struct RawUserdata<T: 'static>(PhantomData<T>);

impl<T: 'static> RawUserdata<T> {
    #[inline(always)]
    unsafe fn push(data: T, lua: LuaContext) {
        let mem = ffi::lua_newuserdata(lua.as_ptr(), Self::size());
        let tid = mem.cast::<TypeId>();

        // Ensure that alignment matches
        debug_assert_eq!(mem as usize % LUA_ALLOCATOR_ALIGNMENT, 0);
        debug_assert_eq!(tid as usize % align_of::<TypeId>(), 0);

        // We always want to write the TypeId
        std::ptr::write(tid, TypeId::of::<T>());

        // If we're holding ZST data there's no reason to actually save it, since it's zero-sized
        // This matters since if we *do* save it it'll not necessarily have the correct alignment
        if !Self::holds_zst() {
            // Find the data right after
            let dta = mem.cast::<u8>().add(size_of::<TypeId>());

            // Ensure that alignment matches
            debug_assert_eq!(dta as usize % Self::data_fld_align(), 0);

            match Self::inline() {
                true => std::ptr::write(dta.cast(), data),
                false => std::ptr::write(dta.cast(), Box::new(data)),
            }
        }
    }

    /// Reads a [`TypeId`] from the pointer and ensures it matches [`T`].
    unsafe fn validate_type_id(ptr: *mut c_void) -> bool {
        *ptr.cast::<TypeId>() == TypeId::of::<T>()
    }

    const fn size() -> usize {
        size_of::<TypeId>() + if Self::inline() { size_of::<T>() } else { size_of::<Box<T>>() }
    }

    const fn holds_zst() -> bool {
        size_of::<T>() == 0
    }

    /// Returns whether or not the data is stored inline.
    /// If it is not, it is stored in a box.
    const fn inline() -> bool {
        !Self::holds_zst() && align_of::<T>() <= LUA_ALLOCATOR_ALIGNMENT
    }

    /// Returns the alignment of the data field.
    const fn data_fld_align() -> usize {
        match Self::inline() {
            true => align_of::<T>(),
            false => align_of::<Box<T>>(),
        }
    }

    /// Returns a pointer to the inner data.
    unsafe fn data_ptr(ptr: *mut c_void) -> *mut T {
        // Always return a valid ZST pointer for ZSTs.
        // We're handling ZSTs like this to avoid allocating space for and empty box.
        if size_of::<T>() == 0 {
            return NonNull::dangling().as_ptr();
        }

        let dta = ptr.cast::<u8>().add(size_of::<TypeId>());
        match Self::inline() {
            true => dta.cast::<T>(),
            false => addr_of_mut!(*(*dta.cast::<Box<T>>())),
        }
    }

    // Returns a mutable reference to the inner data.
    unsafe fn data_mut<'a>(ptr: *mut c_void) -> &'a mut T {
        &mut *Self::data_ptr(ptr)
    }

    /// Drops the data.
    unsafe fn drop(ptr: *mut c_void) {
        let dta = ptr.cast::<u8>().add(size_of::<TypeId>());
        match Self::inline() {
            true => ptr::drop_in_place(dta.cast::<T>()),
            false => ptr::drop_in_place(dta.cast::<Box<T>>()),
        }
    }
}

// Called when an object inside Lua that requires Drop is being dropped.
#[inline]
extern "C" fn destructor_wrapper<T: 'static>(lua: *mut ffi::lua_State) -> libc::c_int {
    unsafe {
        RawUserdata::<T>::drop(ffi::lua_touserdata(lua, -1));
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
        RawUserdata::push(data, raw_lua);

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
        match ptr.is_null() {
            false if RawUserdata::<T>::validate_type_id(ptr) => Ok(RawUserdata::<T>::data_mut(ptr)),
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
            match ptr.is_null() {
                false if RawUserdata::<T>::validate_type_id(ptr) => {
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
            RawUserdata::data_mut(ptr)
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
            RawUserdata::data_mut(ptr)
        }
    }
}
