use crate::AsMutLua;

use crate::{LuaRead, LuaTable, Push, PushGuard, PushOne, Void};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnyLuaString(pub Vec<u8>);

/// Represents any value that can be stored by Lua
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AnyHashableLuaValue {
    LuaString(String),
    LuaAnyString(AnyLuaString),
    LuaInteger(i32),
    LuaBoolean(bool),
    LuaArray(Vec<(AnyHashableLuaValue, AnyHashableLuaValue)>),
    LuaNil,

    /// The "Other" element is (hopefully) temporary and will be replaced by "Function" and "Userdata".
    /// A panic! will trigger if you try to push a Other.
    LuaOther,
}

/// Represents any value that can be stored by Lua
#[derive(Clone, Debug, PartialEq)]
pub enum AnyLuaValue {
    LuaString(String),
    LuaAnyString(AnyLuaString),
    LuaNumber(f64),
    LuaInteger(i32),
    LuaBoolean(bool),
    LuaArray(Vec<(AnyLuaValue, AnyLuaValue)>),
    LuaNil,

    /// The "Other" element is (hopefully) temporary and will be replaced by "Function" and "Userdata".
    /// A panic! will trigger if you try to push a Other.
    LuaOther,
}

impl<'lua, L> Push<L> for AnyLuaValue
where
    L: AsMutLua<'lua>,
{
    type Err = Void;

    #[inline]
    fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Void, L)> {
        let raw_lua = lua.as_mut_lua();
        Ok(match self {
            AnyLuaValue::LuaString(val) => val.push_no_err(lua),
            AnyLuaValue::LuaAnyString(val) => val.push_no_err(lua),
            AnyLuaValue::LuaNumber(val) => val.push_no_err(lua),
            AnyLuaValue::LuaInteger(val) => val.push_no_err(lua),
            AnyLuaValue::LuaBoolean(val) => val.push_no_err(lua),
            AnyLuaValue::LuaArray(val) => {
                // Pushing a `Vec<(AnyLuaValue, AnyLuaValue)>` on a `L` requires calling the
                // function that pushes a `AnyLuaValue` on a `&mut L`, which in turns requires
                // calling the function that pushes a `AnyLuaValue` on a `&mut &mut L`, and so on.
                // In order to avoid this infinite recursion, we push the array on LuaContext instead.

                // We also need to destroy and recreate the push guard, otherwise the type parameter
                // doesn't match.
                let size = val.push_no_err(raw_lua).forget_internal();
                PushGuard { lua, size, raw_lua }
            },
            AnyLuaValue::LuaNil => {
                unsafe { ffi::lua_pushnil(raw_lua.as_ptr()) };
                PushGuard { lua, size: 1, raw_lua }
            }, // Use ffi::lua_pushnil.
            AnyLuaValue::LuaOther => panic!("can't push a AnyLuaValue of type Other"),
        })
    }
}

impl<'lua, L> PushOne<L> for AnyLuaValue where L: AsMutLua<'lua> {}

impl<'lua, L> LuaRead<L> for AnyLuaValue
where
    L: AsMutLua<'lua>,
{
    #[inline]
    fn lua_read_at_position(lua: L, index: i32) -> Result<AnyLuaValue, L> {
        use AnyLuaValue as Value;

        let mut lua = lua;
        let raw_lua = lua.as_lua();

        match unsafe { ffi::lua_type(raw_lua.as_ptr(), index) } {
            ffi::LUA_TNIL => Ok(Value::LuaNil),
            ffi::LUA_TBOOLEAN => LuaRead::lua_read_at_position(lua, index).map(Value::LuaBoolean),
            ffi::LUA_TNUMBER => LuaRead::lua_read_at_position(lua, index).map(Value::LuaNumber),
            ffi::LUA_TSTRING => Err(lua)
                .or_else(|lua| LuaRead::lua_read_at_position(lua, index).map(Value::LuaString))
                .or_else(|lua| LuaRead::lua_read_at_position(lua, index).map(Value::LuaAnyString)),
            ffi::LUA_TTABLE => LuaTable::lua_read_at_position(lua.as_mut_lua(), index)
                .map(|mut v| v.iter::<Value, Value>().flatten().collect())
                .map(Value::LuaArray)
                .map_err(|_| lua),
            _ => Ok(Value::LuaOther),
        }
        .or(Ok(Value::LuaOther))
    }
}

impl<'lua, L> Push<L> for AnyHashableLuaValue
where
    L: AsMutLua<'lua>,
{
    type Err = Void;

    #[inline]
    fn push_to_lua(self, mut lua: L) -> Result<PushGuard<L>, (Void, L)> {
        let raw_lua = lua.as_mut_lua();
        Ok(match self {
            AnyHashableLuaValue::LuaString(val) => val.push_no_err(lua),
            AnyHashableLuaValue::LuaAnyString(val) => val.push_no_err(lua),
            AnyHashableLuaValue::LuaInteger(val) => val.push_no_err(lua),
            AnyHashableLuaValue::LuaBoolean(val) => val.push_no_err(lua),
            AnyHashableLuaValue::LuaArray(val) => {
                // Pushing a `Vec<(AnyHashableLuaValue, AnyHashableLuaValue)>` on a `L` requires calling the
                // function that pushes a `AnyHashableLuaValue` on a `&mut L`, which in turns requires
                // calling the function that pushes a `AnyHashableLuaValue` on a `&mut &mut L`, and so on.
                // In order to avoid this infinite recursion, we push the array on LuaContext instead.

                // We also need to destroy and recreate the push guard, otherwise the type parameter
                // doesn't match.
                let size = val.push_no_err(raw_lua).forget_internal();
                PushGuard { lua, size, raw_lua }
            },
            AnyHashableLuaValue::LuaNil => {
                unsafe { ffi::lua_pushnil(raw_lua.as_ptr()) };
                PushGuard { lua, size: 1, raw_lua }
            },
            AnyHashableLuaValue::LuaOther => {
                panic!("can't push a AnyHashableLuaValue of type Other")
            },
        })
    }
}

impl<'lua, L> PushOne<L> for AnyHashableLuaValue where L: AsMutLua<'lua> {}

impl<'lua, L> LuaRead<L> for AnyHashableLuaValue
where
    L: AsMutLua<'lua>,
{
    #[inline]
    fn lua_read_at_position(lua: L, index: i32) -> Result<AnyHashableLuaValue, L> {
        use AnyHashableLuaValue as Value;

        let mut lua = lua;
        let raw_lua = lua.as_lua();

        match unsafe { ffi::lua_type(raw_lua.as_ptr(), index) } {
            ffi::LUA_TNIL => Ok(Value::LuaNil),
            ffi::LUA_TBOOLEAN => LuaRead::lua_read_at_position(lua, index).map(Value::LuaBoolean),
            ffi::LUA_TNUMBER => Err(lua)
                .or_else(|lua| LuaRead::lua_read_at_position(lua, index).map(Value::LuaInteger))
                .or_else(|lua| LuaRead::lua_read_at_position(lua, index).map(Value::LuaString)),
            ffi::LUA_TSTRING => Err(lua)
                .or_else(|lua| LuaRead::lua_read_at_position(lua, index).map(Value::LuaString))
                .or_else(|lua| LuaRead::lua_read_at_position(lua, index).map(Value::LuaAnyString)),
            ffi::LUA_TTABLE => LuaTable::lua_read_at_position(lua.as_mut_lua(), index)
                .map(|mut v| v.iter::<Value, Value>().flatten().collect())
                .map(Value::LuaArray)
                .map_err(|_| lua),

            _ => Ok(Value::LuaOther),
        }
        .or(Ok(Value::LuaOther))
    }
}

#[cfg(test)]
mod tests {
    use crate::{AnyHashableLuaValue, AnyLuaString, AnyLuaValue, Lua};

    #[test]
    fn read_numbers() {
        let mut lua = Lua::new();

        let val: AnyLuaValue =
            crate::LuaFunction::load(&mut lua, "return 2.5;").unwrap().call().unwrap();
        assert_eq!(val, AnyLuaValue::LuaNumber(2.5));

        lua.set("a", "-2");
        lua.set("b", 3.5f32);
        lua.set("c", -2.0f32);

        let x: AnyLuaValue = lua.get("a").unwrap();
        assert_eq!(x, AnyLuaValue::LuaString("-2".to_owned()));

        let y: AnyLuaValue = lua.get("b").unwrap();
        assert_eq!(y, AnyLuaValue::LuaNumber(3.5));

        let z: AnyLuaValue = lua.get("c").unwrap();
        assert_eq!(z, AnyLuaValue::LuaNumber(-2.0));
    }

    #[test]
    fn read_hashable_numbers() {
        let mut lua = Lua::new();

        lua.set("a", -2.0f32);
        lua.set("b", 4.0f32);
        lua.set("c", "4");

        let x: AnyHashableLuaValue = lua.get("a").unwrap();
        assert_eq!(x, AnyHashableLuaValue::LuaInteger(-2));

        let y: AnyHashableLuaValue = lua.get("b").unwrap();
        assert_eq!(y, AnyHashableLuaValue::LuaInteger(4));

        let z: AnyHashableLuaValue = lua.get("c").unwrap();
        assert_eq!(z, AnyHashableLuaValue::LuaString("4".to_owned()));
    }

    #[test]
    fn read_strings() {
        let mut lua = Lua::new();

        lua.set("a", "hello");
        lua.set("b", "3x");
        lua.set("c", "false");

        let x: AnyLuaValue = lua.get("a").unwrap();
        assert_eq!(x, AnyLuaValue::LuaString("hello".to_string()));

        let y: AnyLuaValue = lua.get("b").unwrap();
        assert_eq!(y, AnyLuaValue::LuaString("3x".to_string()));

        let z: AnyLuaValue = lua.get("c").unwrap();
        assert_eq!(z, AnyLuaValue::LuaString("false".to_string()));
    }

    #[test]
    fn read_hashable_strings() {
        let mut lua = Lua::new();

        lua.set("a", "hello");
        lua.set("b", "3x");
        lua.set("c", "false");

        let x: AnyHashableLuaValue = lua.get("a").unwrap();
        assert_eq!(x, AnyHashableLuaValue::LuaString("hello".to_string()));

        let y: AnyHashableLuaValue = lua.get("b").unwrap();
        assert_eq!(y, AnyHashableLuaValue::LuaString("3x".to_string()));

        let z: AnyHashableLuaValue = lua.get("c").unwrap();
        assert_eq!(z, AnyHashableLuaValue::LuaString("false".to_string()));
    }

    #[test]
    fn read_booleans() {
        let mut lua = Lua::new();

        lua.set("a", true);
        lua.set("b", false);

        let x: AnyLuaValue = lua.get("a").unwrap();
        assert_eq!(x, AnyLuaValue::LuaBoolean(true));

        let y: AnyLuaValue = lua.get("b").unwrap();
        assert_eq!(y, AnyLuaValue::LuaBoolean(false));
    }

    #[test]
    fn read_hashable_booleans() {
        let mut lua = Lua::new();

        lua.set("a", true);
        lua.set("b", false);

        let x: AnyHashableLuaValue = lua.get("a").unwrap();
        assert_eq!(x, AnyHashableLuaValue::LuaBoolean(true));

        let y: AnyHashableLuaValue = lua.get("b").unwrap();
        assert_eq!(y, AnyHashableLuaValue::LuaBoolean(false));
    }

    #[test]
    fn read_tables() {
        let mut lua = Lua::new();
        lua.execute::<()>(
            "
        a = {x = 12, y = 19}
        b = {z = a, w = 'test string'}
        c = {'first', 'second'}
        ",
        )
        .unwrap();

        fn get<'a>(table: &'a AnyLuaValue, key: &str) -> &'a AnyLuaValue {
            let test_key = AnyLuaValue::LuaString(key.to_owned());
            match table {
                &AnyLuaValue::LuaArray(ref vec) => {
                    let &(_, ref value) =
                        vec.iter().find(|&&(ref key, _)| key == &test_key).expect("key not found");
                    value
                },
                _ => panic!("not a table"),
            }
        }

        fn get_numeric<'a>(table: &'a AnyLuaValue, key: usize) -> &'a AnyLuaValue {
            let test_key = AnyLuaValue::LuaNumber(key as f64);
            match table {
                &AnyLuaValue::LuaArray(ref vec) => {
                    let &(_, ref value) =
                        vec.iter().find(|&&(ref key, _)| key == &test_key).expect("key not found");
                    value
                },
                _ => panic!("not a table"),
            }
        }

        let a: AnyLuaValue = lua.get("a").unwrap();
        assert_eq!(get(&a, "x"), &AnyLuaValue::LuaNumber(12.0));
        assert_eq!(get(&a, "y"), &AnyLuaValue::LuaNumber(19.0));

        let b: AnyLuaValue = lua.get("b").unwrap();
        assert_eq!(get(&get(&b, "z"), "x"), get(&a, "x"));
        assert_eq!(get(&get(&b, "z"), "y"), get(&a, "y"));

        let c: AnyLuaValue = lua.get("c").unwrap();
        assert_eq!(get_numeric(&c, 1), &AnyLuaValue::LuaString("first".to_owned()));
        assert_eq!(get_numeric(&c, 2), &AnyLuaValue::LuaString("second".to_owned()));
    }

    #[test]
    fn read_hashable_tables() {
        let mut lua = Lua::new();
        lua.execute::<()>(
            "
        a = {x = 12, y = 19}
        b = {z = a, w = 'test string'}
        c = {'first', 'second'}
        ",
        )
        .unwrap();

        fn get<'a>(table: &'a AnyHashableLuaValue, key: &str) -> &'a AnyHashableLuaValue {
            let test_key = AnyHashableLuaValue::LuaString(key.to_owned());
            match table {
                &AnyHashableLuaValue::LuaArray(ref vec) => {
                    let &(_, ref value) =
                        vec.iter().find(|&&(ref key, _)| key == &test_key).expect("key not found");
                    value
                },
                _ => panic!("not a table"),
            }
        }

        fn get_numeric<'a>(table: &'a AnyHashableLuaValue, key: usize) -> &'a AnyHashableLuaValue {
            let test_key = AnyHashableLuaValue::LuaInteger(key as i32);
            match table {
                &AnyHashableLuaValue::LuaArray(ref vec) => {
                    let &(_, ref value) =
                        vec.iter().find(|&&(ref key, _)| key == &test_key).expect("key not found");
                    value
                },
                _ => panic!("not a table"),
            }
        }

        let a: AnyHashableLuaValue = lua.get("a").unwrap();
        assert_eq!(get(&a, "x"), &AnyHashableLuaValue::LuaInteger(12));
        assert_eq!(get(&a, "y"), &AnyHashableLuaValue::LuaInteger(19));

        let b: AnyHashableLuaValue = lua.get("b").unwrap();
        assert_eq!(get(&get(&b, "z"), "x"), get(&a, "x"));
        assert_eq!(get(&get(&b, "z"), "y"), get(&a, "y"));

        let c: AnyHashableLuaValue = lua.get("c").unwrap();
        assert_eq!(get_numeric(&c, 1), &AnyHashableLuaValue::LuaString("first".to_owned()));
        assert_eq!(get_numeric(&c, 2), &AnyHashableLuaValue::LuaString("second".to_owned()));
    }

    #[test]
    fn push_numbers() {
        let mut lua = Lua::new();

        lua.set("a", AnyLuaValue::LuaInteger(1));
        lua.set("b", AnyLuaValue::LuaNumber(2.5));

        let x: i32 = lua.get("a").unwrap();
        let y: f64 = lua.get("b").unwrap();

        assert_eq!(x, 1);
        assert_eq!(y, 2.5);
    }

    #[test]
    fn push_hashable_numbers() {
        let mut lua = Lua::new();

        lua.set("a", AnyHashableLuaValue::LuaInteger(3));

        let x: i32 = lua.get("a").unwrap();
        assert_eq!(x, 3);
    }

    #[test]
    fn push_strings() {
        let mut lua = Lua::new();

        lua.set("a", AnyLuaValue::LuaString("hello".to_string()));

        let x: String = lua.get("a").unwrap();
        assert_eq!(x, "hello");
    }

    #[test]
    fn push_hashable_strings() {
        let mut lua = Lua::new();

        lua.set("a", AnyHashableLuaValue::LuaString("hello".to_string()));

        let x: String = lua.get("a").unwrap();
        assert_eq!(x, "hello");
    }

    #[test]
    fn push_booleans() {
        let mut lua = Lua::new();

        lua.set("a", AnyLuaValue::LuaBoolean(true));

        let x: bool = lua.get("a").unwrap();
        assert_eq!(x, true);
    }

    #[test]
    fn push_hashable_booleans() {
        let mut lua = Lua::new();

        lua.set("a", AnyHashableLuaValue::LuaBoolean(true));

        let x: bool = lua.get("a").unwrap();
        assert_eq!(x, true);
    }

    #[test]
    fn push_nil() {
        let mut lua = Lua::new();

        lua.set("a", AnyLuaValue::LuaNil);

        let x: Option<i32> = lua.get("a");
        assert!(x.is_none(), "x is a Some value when it should be a None value. X: {:?}", x);
    }

    #[test]
    fn push_hashable_nil() {
        let mut lua = Lua::new();

        lua.set("a", AnyHashableLuaValue::LuaNil);

        let x: Option<i32> = lua.get("a");
        assert!(x.is_none(), "x is a Some value when it should be a None value. X: {:?}", x);
    }

    #[test]
    fn non_utf_8_string() {
        let mut lua = Lua::new();
        let a = lua.execute::<AnyLuaValue>(r"return '\xff\xfe\xff\xfe'").unwrap();
        match a {
            AnyLuaValue::LuaAnyString(AnyLuaString(v)) => {
                assert_eq!(Vec::from(&b"\xff\xfe\xff\xfe"[..]), v);
            },
            _ => panic!("Decoded to wrong variant"),
        }
    }
}
