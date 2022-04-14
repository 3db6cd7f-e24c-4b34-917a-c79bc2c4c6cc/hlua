use std::{marker::PhantomData, mem, ops::Deref, slice, str};

use crate::{AnyLuaString, AnyLuaValue, AsLua, AsMutLua, LuaRead, Push, PushGuard, PushOne, Void};

macro_rules! integer_impl(
    ($t:ident) => (
        impl<'lua, L> Push<L> for $t where L: AsMutLua<'lua> {
            type Err = Void;

            #[inline]
            fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Void, L)> {
                let raw_lua = lua.as_mut_lua();
                unsafe { ffi::lua_pushinteger(raw_lua.as_ptr(), self as ffi::lua_Integer) };
                Ok(PushGuard { lua, size: 1, raw_lua })
            }
        }

        impl<'lua, L> PushOne<L> for $t where L: AsMutLua<'lua> {
        }

        impl<'lua, L> LuaRead<L> for $t where L: AsLua<'lua> {
            #[inline]
            fn lua_read_at_position(lua: L, index: i32) -> Result<$t, L> {
                let mut success = mem::MaybeUninit::uninit();
                let val = unsafe { ffi::lua_tointegerx(lua.as_lua().as_ptr(), index, success.as_mut_ptr()) };
                match unsafe { success.assume_init() } {
                    0 => Err(lua),
                    _ => Ok(val as $t)
                }
            }
        }
    );
);

integer_impl!(i8);
integer_impl!(i16);
integer_impl!(i32);
// integer_impl!(i64)   // data loss

macro_rules! unsigned_impl(
    ($t:ident) => (
        impl<'lua, L> Push<L> for $t where L: AsMutLua<'lua> {
            type Err = Void;

            #[inline]
            fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Void, L)> {
                let raw_lua = lua.as_mut_lua();

                match () {
                    #[cfg(feature = "_luaapi_51")] () => unsafe { ffi::lua_pushnumber(raw_lua.as_ptr(), self as _) },
                    #[cfg(feature = "_luaapi_52")] () => unsafe { ffi::lua_pushunsigned(raw_lua.as_ptr(), self as _) },
                    #[cfg(feature = "_luaapi_54")] () => unsafe { ffi::lua_pushunsigned(raw_lua.as_ptr(), self as _) },
                }

                Ok(PushGuard { lua, size: 1, raw_lua })
            }
        }

        impl<'lua, L> PushOne<L> for $t where L: AsMutLua<'lua> {
        }

        impl<'lua, L> LuaRead<L> for $t where L: AsLua<'lua> {
            #[inline]
            fn lua_read_at_position(lua: L, index: i32) -> Result<$t, L> {
                let mut success = mem::MaybeUninit::uninit();
                let val = match () {
                    #[cfg(feature = "_luaapi_51")] () => unsafe { ffi::lua_tonumberx(lua.as_lua().as_ptr(), index, success.as_mut_ptr()) as $t },
                    #[cfg(feature = "_luaapi_52")] () => unsafe { ffi::lua_tounsignedx(lua.as_lua().as_ptr(), index, success.as_mut_ptr()) },
                    #[cfg(feature = "_luaapi_54")] () => unsafe { ffi::lua_tounsignedx(lua.as_lua().as_ptr(), index, success.as_mut_ptr()) },
                };
                match unsafe { success.assume_init() } {
                    0 => Err(lua),
                    _ => Ok(val as $t)
                }
            }
        }
    );
);

unsigned_impl!(u8);
unsigned_impl!(u16);
unsigned_impl!(u32);
// unsigned_impl!(u64);   // data loss

macro_rules! numeric_impl(
    ($t:ident) => (
        impl<'lua, L> Push<L> for $t where L: AsMutLua<'lua> {
            type Err = Void;

            #[inline]
            fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Void, L)> {
                let raw_lua = lua.as_mut_lua();
                unsafe { ffi::lua_pushnumber(raw_lua.as_ptr(), self as ffi::lua_Number) };
                Ok(PushGuard { lua, size: 1, raw_lua })
            }
        }

        impl<'lua, L> PushOne<L> for $t where L: AsMutLua<'lua> {
        }

        impl<'lua, L> LuaRead<L> for $t where L: AsLua<'lua> {
            #[inline]
            fn lua_read_at_position(lua: L, index: i32) -> Result<$t, L> {
                let mut success = mem::MaybeUninit::uninit();
                let val = unsafe { ffi::lua_tonumberx(lua.as_lua().as_ptr(), index, success.as_mut_ptr()) };
                match unsafe { success.assume_init() } {
                    0 => Err(lua),
                    _ => Ok(val as $t)
                }
            }
        }
    );
);

numeric_impl!(f32);
numeric_impl!(f64);

impl<'lua, L> Push<L> for String
where
    L: AsMutLua<'lua>,
{
    type Err = Void;

    #[inline]
    fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Void, L)> {
        unsafe {
            let raw_lua = lua.as_mut_lua();
            ffi::lua_pushlstring(
                raw_lua.as_ptr(),
                self.as_bytes().as_ptr().cast(),
                self.as_bytes().len() as libc::size_t,
            );

            Ok(PushGuard { lua, size: 1, raw_lua })
        }
    }
}

impl<'lua, L> PushOne<L> for String where L: AsMutLua<'lua> {}

impl<'lua, L> LuaRead<L> for String
where
    L: AsLua<'lua>,
{
    #[inline]
    fn lua_read_at_position(lua: L, index: i32) -> Result<String, L> {
        let mut size = mem::MaybeUninit::uninit();
        let c_str_raw =
            unsafe { ffi::lua_tolstring(lua.as_lua().as_ptr(), index, size.as_mut_ptr()) };
        if c_str_raw.is_null() {
            return Err(lua);
        }

        let size = unsafe { size.assume_init() };

        let c_slice = unsafe { slice::from_raw_parts(c_str_raw.cast(), size) };
        let maybe_string = String::from_utf8(c_slice.to_vec());
        match maybe_string {
            Ok(string) => Ok(string),
            Err(_) => Err(lua),
        }
    }
}

impl<'lua, L> Push<L> for AnyLuaString
where
    L: AsMutLua<'lua>,
{
    type Err = Void;

    #[inline]
    fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Void, L)> {
        let AnyLuaString(v) = self;
        unsafe {
            let raw_lua = lua.as_mut_lua();
            ffi::lua_pushlstring(
                raw_lua.as_ptr(),
                v[..].as_ptr().cast(),
                v[..].len() as libc::size_t,
            );

            Ok(PushGuard { lua, size: 1, raw_lua })
        }
    }
}

impl<'lua, L> LuaRead<L> for AnyLuaString
where
    L: AsLua<'lua>,
{
    #[inline]
    fn lua_read_at_position(lua: L, index: i32) -> Result<AnyLuaString, L> {
        let mut size = mem::MaybeUninit::uninit();
        let c_str_raw =
            unsafe { ffi::lua_tolstring(lua.as_lua().as_ptr(), index, size.as_mut_ptr()) };
        if c_str_raw.is_null() {
            return Err(lua);
        }

        let size = unsafe { size.assume_init() };

        let c_slice = unsafe { slice::from_raw_parts(c_str_raw.cast::<u8>(), size) };
        Ok(AnyLuaString(c_slice.to_vec()))
    }
}

impl<'lua, 's, L> Push<L> for &'s str
where
    L: AsMutLua<'lua>,
{
    type Err = Void;

    #[inline]
    fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Void, L)> {
        unsafe {
            let raw_lua = lua.as_mut_lua();
            ffi::lua_pushlstring(
                raw_lua.as_ptr(),
                self.as_bytes().as_ptr().cast(),
                self.as_bytes().len() as libc::size_t,
            );

            Ok(PushGuard { lua, size: 1, raw_lua })
        }
    }
}

impl<'lua, 's, L> PushOne<L> for &'s str where L: AsMutLua<'lua> {}

/// String on the Lua stack.
///
/// It is faster -but less convenient- to read a `StringInLua` rather than a `String` because you
/// avoid any allocation.
///
/// The `StringInLua` derefs to `str`.
///
/// # Example
///
/// ```
/// let mut lua = hlua::Lua::new();
/// lua.set("a", "hello");
///
/// let s: hlua::StringInLua<_> = lua.get("a").unwrap();
/// println!("{}", &*s);    // Prints "hello".
/// ```
#[derive(Debug)]
pub struct StringInLua<L> {
    // We want to lock [`StringInLua`] to the lifetime of L, or we might end up with UAF.
    _lua: PhantomData<L>,

    c_str_raw: *const libc::c_char,
    size: libc::size_t,
}

impl<'lua, L> LuaRead<L> for StringInLua<L>
where
    L: AsLua<'lua>,
{
    #[inline]
    fn lua_read_at_position(lua: L, index: i32) -> Result<StringInLua<L>, L> {
        let mut size = mem::MaybeUninit::uninit();
        let c_str_raw =
            unsafe { ffi::lua_tolstring(lua.as_lua().as_ptr(), index, size.as_mut_ptr()) };
        if c_str_raw.is_null() {
            return Err(lua);
        }

        let size = unsafe { size.assume_init() };

        let c_slice = unsafe { slice::from_raw_parts(c_str_raw.cast::<u8>(), size) };
        match str::from_utf8(c_slice) {
            Ok(_) => (),
            Err(_) => return Err(lua),
        };

        Ok(StringInLua { _lua: PhantomData, c_str_raw, size })
    }
}

impl<L> Deref for StringInLua<L> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        let c_slice = unsafe { slice::from_raw_parts(self.c_str_raw.cast::<u8>(), self.size) };
        match str::from_utf8(c_slice) {
            Ok(s) => s,
            Err(_) => unreachable!(), // Checked earlier
        }
    }
}

impl<'lua, L> Push<L> for bool
where
    L: AsMutLua<'lua>,
{
    type Err = Void;

    #[inline]
    fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Void, L)> {
        let raw_lua = lua.as_mut_lua();
        unsafe { ffi::lua_pushboolean(raw_lua.as_ptr(), self as libc::c_int) };
        Ok(PushGuard { lua, size: 1, raw_lua })
    }
}

impl<'lua, L> PushOne<L> for bool where L: AsMutLua<'lua> {}

impl<'lua, L> LuaRead<L> for bool
where
    L: AsLua<'lua>,
{
    #[inline]
    fn lua_read_at_position(lua: L, index: i32) -> Result<bool, L> {
        let raw_lua = lua.as_lua();
        if !unsafe { ffi::lua_isboolean(raw_lua.as_ptr(), index) } {
            return Err(lua);
        }

        Ok(unsafe { ffi::lua_toboolean(raw_lua.as_ptr(), index) != 0 })
    }
}

impl<'lua, L> Push<L> for ()
where
    L: AsMutLua<'lua>,
{
    type Err = Void;

    #[inline]
    fn push_to_lua(self, lua: L) -> Result<PushGuard<L>, (Void, L)> {
        let raw_lua = lua.as_lua();
        Ok(PushGuard { lua, size: 0, raw_lua })
    }
}

impl<'lua, L> LuaRead<L> for ()
where
    L: AsLua<'lua>,
{
    #[inline]
    fn lua_read_at_position(_: L, _: i32) -> Result<(), L> {
        Ok(())
    }
}

impl<'lua, L, T, E> Push<L> for Option<T>
where
    T: Push<L, Err = E>,
    L: AsMutLua<'lua>,
{
    type Err = E;

    #[inline]
    fn push_to_lua(self, lua: L) -> Result<PushGuard<L>, (E, L)> {
        match self {
            Some(val) => val.push_to_lua(lua),
            None => Ok(AnyLuaValue::LuaNil.push_no_err(lua)),
        }
    }
}

impl<'lua, L, T, E> PushOne<L> for Option<T>
where
    T: PushOne<L, Err = E>,
    L: AsMutLua<'lua>,
{
}

impl<'lua, T, L> LuaRead<L> for Option<T>
where
    T: LuaRead<L>,
    L: AsLua<'lua>,
{
    #[inline]
    fn lua_read_at_position(lua: L, index: i32) -> Result<Option<T>, L> {
        if unsafe { ffi::lua_isnoneornil(lua.as_lua().as_ptr(), index) } {
            return Ok(None);
        }

        T::lua_read_at_position(lua, index).map(Some)
    }

    #[inline]
    fn lua_read_out_of_bounds(_: L) -> Result<Self, L> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use crate::{AnyLuaString, AnyLuaValue, Lua, StringInLua};

    #[test]
    fn read_i32s() {
        let mut lua = Lua::new();

        lua.set("a", 2);

        let x: i32 = lua.get("a").unwrap();
        assert_eq!(x, 2);

        let y: i8 = lua.get("a").unwrap();
        assert_eq!(y, 2);

        let z: i16 = lua.get("a").unwrap();
        assert_eq!(z, 2);

        let w: i32 = lua.get("a").unwrap();
        assert_eq!(w, 2);

        let a: u32 = lua.get("a").unwrap();
        assert_eq!(a, 2);

        let b: u8 = lua.get("a").unwrap();
        assert_eq!(b, 2);

        let c: u16 = lua.get("a").unwrap();
        assert_eq!(c, 2);

        let d: u32 = lua.get("a").unwrap();
        assert_eq!(d, 2);
    }

    #[test]
    fn validate_extreme_numbers() {
        let mut lua = Lua::new();
        lua.openlibs();

        macro_rules! validate_extremes {
            ($validate_in_lua:expr, $t:ident) => {{
                type T = $t;
                const MIN: T = T::MIN as T;
                const MAX: T = T::MAX as T;

                lua.set("min_v", MIN);
                lua.set("max_v", MAX);

                assert_eq!(lua.get::<T, _>("min_v").expect("1"), MIN, "min invalid (roundtrip)");
                assert_eq!(lua.get::<T, _>("max_v").expect("2"), MAX, "max invalid (roundtrip)");

                lua.set("min_fn_ret", crate::function0(|| -> T { MIN }));
                lua.set("max_fn_ret", crate::function0(|| -> T { MAX }));

                lua.set("min_fn_arg", crate::function1(|x: T| -> bool { x == MIN }));
                lua.set("max_fn_arg", crate::function1(|x: T| -> bool { x == MAX }));

                assert_eq!(
                    lua.execute::<T>("return min_fn_ret()").expect("3"),
                    MIN,
                    "min invalid (func return to lua)"
                );
                assert_eq!(
                    lua.execute::<T>("return max_fn_ret()").expect("4"),
                    MAX,
                    "max invalid (func return to lua)"
                );

                assert!(
                    lua.execute::<bool>("return min_fn_arg(min_v)").expect("5"),
                    "min invalid (func arg from lua)"
                );
                assert!(
                    lua.execute::<bool>("return max_fn_arg(max_v)").expect("6"),
                    "max invalid (func arg from lua)"
                );

                if $validate_in_lua {
                    assert_eq!(
                        lua.execute::<f64>(&format!("return {}", MIN)).expect("7") as T,
                        MIN,
                        "min invalid (read from lua)"
                    );
                    assert_eq!(
                        lua.execute::<f64>(&format!("return {}", MAX)).expect("8") as T,
                        MAX,
                        "max invalid (read from lua)"
                    );

                    lua.execute::<()>(&format!("min_l = {}", MIN)).expect("9");
                    lua.execute::<()>(&format!("max_l = {}", MAX)).expect("10");

                    assert_eq!(
                        lua.execute::<String>("return string.format('%f', min_l)").expect("15"),
                        lua.execute::<String>("return string.format('%f', min_v)").expect("15"),
                        "min invalid (string in lua)"
                    );

                    assert_eq!(
                        lua.execute::<String>("return string.format('%.0f', max_l)").expect("15"),
                        lua.execute::<String>("return string.format('%.0f', max_v)").expect("15"),
                        "max invalid (string in lua)"
                    );

                    assert!(
                        lua.execute::<bool>("return min_l == min_fn_ret()").expect("13"),
                        "min invalid (in lua, func return)"
                    );
                    assert!(
                        lua.execute::<bool>("return max_l == max_fn_ret()").expect("14"),
                        "max invalid (in lua, func return)"
                    );

                    assert_eq!(
                        lua.execute::<String>(&format!("return string.format('%.0f', {})", MIN))
                            .expect("15"),
                        format!("{}", MIN),
                        "min invalid (string in lua)"
                    );
                    assert_eq!(
                        lua.execute::<String>(&format!("return string.format('%.0f', {})", MAX))
                            .expect("16"),
                        format!("{}", MAX),
                        "max invalid (string in lua)"
                    );
                }
            }};
        }

        validate_extremes!(true, i8);
        validate_extremes!(true, i16);
        validate_extremes!(true, i32);

        validate_extremes!(true, u8);
        validate_extremes!(true, u16);
        validate_extremes!(true, u32);

        validate_extremes!(false, f32);
        validate_extremes!(false, f64);
    }

    #[test]
    fn write_i32s() {
        // TODO:

        let mut lua = Lua::new();

        lua.set("a", 2);
        let x: i32 = lua.get("a").unwrap();
        assert_eq!(x, 2);
    }

    #[test]
    fn readwrite_floats() {
        let mut lua = Lua::new();

        lua.set("a", 2.51234 as f32);
        lua.set("b", 3.4123456789 as f64);

        let x: f32 = lua.get("a").unwrap();
        assert!(x - 2.51234 < 0.000001);

        let y: f64 = lua.get("a").unwrap();
        assert!(y - 2.51234 < 0.000001);

        let z: f32 = lua.get("b").unwrap();
        assert!(z - 3.4123456789 < 0.000001);

        let w: f64 = lua.get("b").unwrap();
        assert!(w - 3.4123456789 < 0.000001);
    }

    #[test]
    fn readwrite_bools() {
        let mut lua = Lua::new();

        lua.set("a", true);
        lua.set("b", false);

        let x: bool = lua.get("a").unwrap();
        assert_eq!(x, true);

        let y: bool = lua.get("b").unwrap();
        assert_eq!(y, false);
    }

    #[test]
    fn readwrite_strings() {
        let mut lua = Lua::new();

        lua.set("a", "hello");
        lua.set("b", "hello".to_string());

        let x: String = lua.get("a").unwrap();
        assert_eq!(x, "hello");

        let y: String = lua.get("b").unwrap();
        assert_eq!(y, "hello");

        assert_eq!(lua.execute::<String>("return 'abc'").unwrap(), "abc");
        assert_eq!(lua.execute::<u32>("return #'abc'").unwrap(), 3);
        assert_eq!(lua.execute::<u32>("return #'a\\x00c'").unwrap(), 3);
        assert_eq!(lua.execute::<AnyLuaString>("return 'a\\x00c'").unwrap().0, vec!(97, 0, 99));
        assert_eq!(lua.execute::<AnyLuaString>("return 'a\\x00c'").unwrap().0.len(), 3);
        assert_eq!(lua.execute::<AnyLuaString>("return '\\x01\\xff'").unwrap().0, vec!(1, 255));
        lua.execute::<String>("return 'a\\x00\\xc0'").unwrap_err();
    }

    #[test]
    fn i32_to_string() {
        let mut lua = Lua::new();

        lua.set("a", 2);

        let x: String = lua.get("a").unwrap();
        assert_eq!(x, "2");
    }

    #[test]
    fn string_to_i32() {
        let mut lua = Lua::new();

        lua.set("a", "2");
        lua.set("b", "aaa");

        let x: i32 = lua.get("a").unwrap();
        assert_eq!(x, 2);

        let y: Option<i32> = lua.get("b");
        assert!(y.is_none());
    }

    #[test]
    fn string_on_lua() {
        let mut lua = Lua::new();

        lua.set("a", "aaa");
        {
            let x: StringInLua<_> = lua.get("a").unwrap();
            assert_eq!(&*x, "aaa");
        }

        lua.set("a", 18);
        {
            let x: StringInLua<_> = lua.get("a").unwrap();
            assert_eq!(&*x, "18");
        }
    }

    #[test]
    fn push_opt() {
        let mut lua = Lua::new();

        lua.set("some", crate::function0(|| Some(123)));
        lua.set("none", crate::function0(|| Option::None::<i32>));

        match lua.execute::<i32>("return some()") {
            Ok(123) => {},
            unexpected => panic!("{:?}", unexpected),
        }

        match lua.execute::<AnyLuaValue>("return none()") {
            Ok(AnyLuaValue::LuaNil) => {},
            unexpected => panic!("{:?}", unexpected),
        }

        lua.set("no_value", None::<i32>);
        lua.set("some_value", Some("Hello!"));

        assert_eq!(lua.get("no_value"), None::<String>);
        assert_eq!(lua.get("some_value"), Some("Hello!".to_string()));
    }

    #[test]
    fn read_opt() {
        let mut lua = Lua::new();

        lua.set("is_some", crate::function1(|foo: Option<String>| foo.is_some()));

        assert_eq!(lua.execute::<bool>("return is_some('foo')").unwrap(), true);
        assert_eq!(lua.execute::<bool>("return is_some(nil)").unwrap(), false);
    }
}
