use crate::any::{AnyHashableLuaValue, AnyLuaValue};

use crate::{AsMutLua, LuaContext, LuaRead, Push, PushGuard, PushOne, TuplePushError};

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    iter,
};

unsafe fn table_len<'a>(lua: LuaContext, index: libc::c_int) -> usize {
    match () {
        #[cfg(feature = "_luaapi_51")]
        () => ffi::lua_objlen(lua.as_ptr(), index),
        #[cfg(feature = "_luaapi_52")]
        () => ffi::lua_rawlen(lua.as_ptr(), index),
        #[cfg(feature = "_luaapi_54")]
        () => ffi::lua_rawlen(lua.as_ptr(), index),
    }
}

#[inline]
fn push_iter<'lua, L, V, I, E>(mut lua: L, iterator: I) -> Result<PushGuard<L>, (E, L)>
where
    L: AsMutLua<'lua>,
    V: for<'b> Push<&'b mut L, Err = E>,
    I: Iterator<Item = V>,
{
    let raw_lua = lua.as_mut_lua();

    // creating empty table with pre-allocated array elements
    unsafe { ffi::lua_createtable(raw_lua.as_ptr(), iterator.size_hint().0 as i32, 0) };

    for (elem, index) in iterator.zip(1..) {
        let size = match elem.push_to_lua(&mut lua) {
            Ok(pushed) => pushed.forget_internal(),
            Err((_err, _lua)) => panic!(), // TODO: wrong   return Err((err, lua)),      // FIXME: destroy the temporary table
        };

        match size {
            0 => continue,
            1 => unsafe { ffi::lua_rawseti(raw_lua.as_ptr(), -2, index) },
            2 => unsafe { ffi::lua_settable(raw_lua.as_ptr(), -3) },
            _ => unreachable!(),
        }
    }

    Ok(PushGuard { lua, size: 1, raw_lua })
}

#[inline]
fn push_rec_iter<'lua, L, V, I, E>(mut lua: L, iterator: I) -> Result<PushGuard<L>, (E, L)>
where
    L: AsMutLua<'lua>,
    V: for<'a> Push<&'a mut L, Err = E>,
    I: Iterator<Item = V>,
{
    let raw_lua = lua.as_mut_lua();

    let (nrec, _) = iterator.size_hint();

    // creating empty table with pre-allocated non-array elements
    unsafe { ffi::lua_createtable(raw_lua.as_ptr(), 0, nrec as i32) };

    for elem in iterator {
        let size = match elem.push_to_lua(&mut lua) {
            Ok(pushed) => pushed.forget_internal(),
            Err((_err, _lua)) => panic!(), // TODO: wrong   return Err((err, lua)),      // FIXME: destroy the temporary table
        };

        match size {
            0 => continue,
            2 => unsafe { ffi::lua_settable(raw_lua.as_ptr(), -3) },
            _ => unreachable!(),
        }
    }

    Ok(PushGuard { lua, size: 1, raw_lua })
}

pub struct IntoIteratorWrapper<I: IntoIterator>(pub I);
impl<I: IntoIterator> From<I> for IntoIteratorWrapper<I> {
    fn from(iter: I) -> Self {
        IntoIteratorWrapper(iter)
    }
}
impl<V, T: Iterator<Item = V>, I: IntoIterator<Item = V, IntoIter = T>> IntoIterator
    for IntoIteratorWrapper<I>
{
    type Item = V;
    type IntoIter = T;
    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        self.0.into_iter()
    }
}

impl<'lua, L, T, E, I> Push<L> for IntoIteratorWrapper<I>
where
    L: AsMutLua<'lua>,
    I: IntoIterator<Item = T>,
    T: for<'a> Push<&'a mut L, Err = E>,
{
    type Err = E;
    #[inline]
    fn push_to_lua(self, lua: L) -> Result<PushGuard<L>, (E, L)> {
        push_iter(lua, self.0.into_iter())
    }
}

impl<'lua, L, T, E, I> PushOne<L> for IntoIteratorWrapper<I>
where
    L: AsMutLua<'lua>,
    I: IntoIterator<Item = T>,
    T: for<'a> Push<&'a mut L, Err = E>,
{
}

impl<'lua, L, T, E> Push<L> for Vec<T>
where
    L: AsMutLua<'lua>,
    T: for<'a> Push<&'a mut L, Err = E>,
{
    type Err = E;

    #[inline]
    fn push_to_lua(self, lua: L) -> Result<PushGuard<L>, (E, L)> {
        push_iter(lua, self.into_iter())
    }
}

impl<'lua, L, T, E> PushOne<L> for Vec<T>
where
    L: AsMutLua<'lua>,
    T: for<'a> Push<&'a mut L, Err = E>,
{
}

impl<'lua, L, T> LuaRead<L> for Vec<T>
where
    L: AsMutLua<'lua>,
    T: for<'a> LuaRead<&'a mut L>,
{
    fn lua_read_at_position(lua: L, index: i32) -> Result<Self, L> {
        let mut me = lua;
        let raw_lua = me.as_mut_lua().as_mut_lua();

        if unsafe { !ffi::lua_istable(raw_lua.as_ptr(), index) } {
            return Err(me);
        }

        let len = unsafe { table_len(raw_lua, index) };

        let mut vec = Vec::<T>::with_capacity(len as _);

        for n in 1..=len as _ {
            // push(vec[n])
            unsafe { ffi::lua_rawgeti(raw_lua.as_ptr(), index, n) };
            let _g = unsafe { PushGuard::new(raw_lua, 1) };

            // if it's nil, we reached the "end"
            if unsafe { ffi::lua_isnil(raw_lua.as_ptr(), -1) } {
                break;
            }

            // try to read the top value as a T
            match T::lua_read_at_position(&mut me, -1).ok() {
                // if we succeed, add it to the output vector
                Some(val) => vec.push(val),
                // if not, pop the value and return Err
                None => return Err(me),
            }
        }

        // return the vec
        Ok(vec)
    }
}

impl<'lua, L, T, const C: usize> LuaRead<L> for [T; C]
where
    L: AsMutLua<'lua>,
    T: for<'a> LuaRead<&'a mut L> + Copy,
{
    fn lua_read_at_position(lua: L, index: i32) -> Result<Self, L> {
        use std::mem::MaybeUninit;

        let mut me = lua;
        let raw_lua = me.as_mut_lua().as_mut_lua();

        if unsafe { !ffi::lua_istable(raw_lua.as_ptr(), index) } {
            return Err(me);
        }

        let len = unsafe { table_len(raw_lua, index) };

        // we can't check == since the object might have more properties than just array indices
        // we could just disallow this, but there's not really a reason to force mapping objects
        if len < C {
            return Err(me);
        }

        // TODO: Use MaybeUninit::uninit_array() once it's stabilized
        let mut arr: [MaybeUninit<T>; C] = unsafe { MaybeUninit::uninit().assume_init() };

        for n in 0..C as _ {
            // push(vec[n])
            unsafe { ffi::lua_rawgeti(raw_lua.as_ptr(), index, (n + 1) as _) };
            let _g = unsafe { PushGuard::new(raw_lua, 1) };

            // try to read the top value as a T
            match T::lua_read_at_position(&mut me, -1).ok() {
                // if we succeed, add it to the output array
                Some(val) => arr[n] = MaybeUninit::new(val),
                // if not, pop the value and return Err
                None => return Err(me),
            }
        }

        let out = unsafe {
            // Workaround since const generics currently can't be transmuted
            // Dangerous and not to be trusted, please replace as soon as the issue is resolved
            // https://github.com/rust-lang/rust/issues/61956
            //
            // TODO: Use `MaybeUninit::array_assume_init(_)` once it's stabilized
            // https://github.com/rust-lang/rust/issues/80908

            core::mem::transmute_copy(&arr)
        };

        // return the array
        Ok(out)
    }
}

impl<'lua, L, T, E, const C: usize> Push<L> for [T; C]
where
    L: AsMutLua<'lua>,
    T: for<'a> Push<&'a mut L, Err = E> + Copy,
{
    type Err = E;

    #[inline]
    fn push_to_lua(self, lua: L) -> Result<PushGuard<L>, (E, L)> {
        push_iter(lua, self.iter().copied())
    }
}

impl<'lua, L, T, E, const C: usize> PushOne<L> for [T; C]
where
    L: AsMutLua<'lua>,
    T: for<'a> Push<&'a mut L, Err = E> + Copy,
{
}

impl<'a, 'lua, L, T, E> Push<L> for &'a [T]
where
    L: AsMutLua<'lua>,
    T: Clone + for<'b> Push<&'b mut L, Err = E>,
{
    type Err = E;

    #[inline]
    fn push_to_lua(self, lua: L) -> Result<PushGuard<L>, (E, L)> {
        push_iter(lua, self.iter().cloned())
    }
}

impl<'a, 'lua, L, T, E> PushOne<L> for &'a [T]
where
    L: AsMutLua<'lua>,
    T: Clone + for<'b> Push<&'b mut L, Err = E>,
{
}

impl<'lua, L, S> LuaRead<L> for HashMap<AnyHashableLuaValue, AnyLuaValue, S>
where
    L: AsMutLua<'lua>,
    S: std::hash::BuildHasher + Default,
{
    // TODO: this should be implemented using the LuaTable API instead of raw Lua calls.
    fn lua_read_at_position(lua: L, index: i32) -> Result<Self, L> {
        let mut me = lua;
        let raw_lua = me.as_mut_lua();
        unsafe { ffi::lua_pushnil(raw_lua.as_ptr()) };
        let index = index - 1;
        let mut result = HashMap::<_, _, S>::default();

        loop {
            if unsafe { ffi::lua_next(raw_lua.as_ptr(), index) } == 0 {
                break;
            }

            let key = {
                let maybe_key: Option<AnyHashableLuaValue> =
                    LuaRead::lua_read_at_position(&mut me, -2).ok();
                match maybe_key {
                    None => {
                        // Cleaning up after ourselves
                        unsafe { ffi::lua_pop(raw_lua.as_ptr(), 2) };
                        return Err(me);
                    },
                    Some(k) => k,
                }
            };

            let value: AnyLuaValue = LuaRead::lua_read_at_position(&mut me, -1).ok().unwrap();

            unsafe { ffi::lua_pop(raw_lua.as_ptr(), 1) };

            result.insert(key, value);
        }

        Ok(result)
    }
}

// TODO: use an enum for the error to allow different error types for K and V
impl<'lua, L, K, V, E, S> Push<L> for HashMap<K, V, S>
where
    L: AsMutLua<'lua>,
    K: for<'a, 'b> PushOne<&'a mut &'b mut L, Err = E> + Eq + Hash,
    V: for<'a, 'b> PushOne<&'a mut &'b mut L, Err = E>,
    S: std::hash::BuildHasher,
{
    type Err = E;

    #[inline]
    fn push_to_lua(self, lua: L) -> Result<PushGuard<L>, (E, L)> {
        match push_rec_iter(lua, self.into_iter()) {
            Ok(g) => Ok(g),
            Err((TuplePushError::First(err), lua)) => Err((err, lua)),
            Err((TuplePushError::Other(err), lua)) => Err((err, lua)),
        }
    }
}

impl<'lua, L, K, V, E, S> PushOne<L> for HashMap<K, V, S>
where
    L: AsMutLua<'lua>,
    K: for<'a, 'b> PushOne<&'a mut &'b mut L, Err = E> + Eq + Hash,
    V: for<'a, 'b> PushOne<&'a mut &'b mut L, Err = E>,
    S: std::hash::BuildHasher,
{
}

impl<'lua, L, K, E, S> Push<L> for HashSet<K, S>
where
    L: AsMutLua<'lua>,
    K: for<'a, 'b> PushOne<&'a mut &'b mut L, Err = E> + Eq + Hash,
    S: std::hash::BuildHasher,
{
    type Err = E;

    #[inline]
    fn push_to_lua(self, lua: L) -> Result<PushGuard<L>, (E, L)> {
        match push_rec_iter(lua, self.into_iter().zip(iter::repeat(true))) {
            Ok(g) => Ok(g),
            Err((TuplePushError::First(err), lua)) => Err((err, lua)),
            Err((TuplePushError::Other(_), _)) => unreachable!(),
        }
    }
}

impl<'lua, L, K, E, S> PushOne<L> for HashSet<K, S>
where
    L: AsMutLua<'lua>,
    K: for<'a, 'b> PushOne<&'a mut &'b mut L, Err = E> + Eq + Hash,
    S: std::hash::BuildHasher,
{
}

#[cfg(test)]
mod tests {
    use crate::{AnyHashableLuaValue, AnyLuaValue, Lua, LuaTable};
    use std::collections::{BTreeMap, HashMap, HashSet};

    #[test]
    fn write() {
        let mut lua = Lua::new();

        lua.set("a", vec![9, 8, 7]);

        let mut table: LuaTable<_> = lua.get("a").unwrap();

        let values: Vec<(i32, i32)> = table.iter().filter_map(|e| e).collect();
        assert_eq!(values, vec![(1, 9), (2, 8), (3, 7)]);
    }

    #[test]
    fn write_map() {
        let mut lua = Lua::new();

        let mut map = HashMap::new();
        map.insert(5, 8);
        map.insert(13, 21);
        map.insert(34, 55);

        lua.set("a", map.clone());

        let mut table: LuaTable<_> = lua.get("a").unwrap();

        let values: HashMap<i32, i32> = table.iter().filter_map(|e| e).collect();
        assert_eq!(values, map);
    }

    #[test]
    fn write_set() {
        let mut lua = Lua::new();

        let mut set = HashSet::new();
        set.insert(5);
        set.insert(8);
        set.insert(13);
        set.insert(21);
        set.insert(34);
        set.insert(55);

        lua.set("a", set.clone());

        let mut table: LuaTable<_> = lua.get("a").unwrap();

        let values: HashSet<i32> = table
            .iter()
            .filter_map(|e| e)
            .map(|(elem, set): (i32, bool)| {
                assert!(set);
                elem
            })
            .collect();

        assert_eq!(values, set);
    }

    #[test]
    fn globals_table() {
        let mut lua = Lua::new();

        lua.globals_table().set("a", 12);

        let val: i32 = lua.get("a").unwrap();
        assert_eq!(val, 12);
    }

    #[test]
    fn reading_array_works() {
        let mut lua = Lua::new();

        let orig = [1., 2., 3.];

        lua.set("v", &orig[..]);

        let read: [f32; 3] = lua.get("v").unwrap();
        assert_eq!(read, orig);
    }

    #[test]
    fn reading_array_as_arg_works() {
        let mut lua = Lua::new();

        lua.set("fn", crate::function1(|_array: [u32; 2]| {}));
        assert_ne!(lua.get::<AnyLuaValue, _>("fn"), None);
    }

    #[test]
    fn reading_too_large_array_ish_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { [1] = 1.0, ["foo"] = 2.0 }"#).unwrap();

        let read: [f32; 1] = lua.get("v").unwrap();
        assert_eq!(read[..], [1.]);
    }

    #[test]
    fn reading_too_large_array_works() {
        let mut lua = Lua::new();

        let orig = [1., 2., 3.];

        lua.set("v", &orig[..]);

        let read: [f32; 2] = lua.get("v").unwrap();
        assert_eq!(read[..], [1., 2.]);
    }

    #[test]
    fn reading_too_small_array_doesnt_work() {
        let mut lua = Lua::new();

        let orig = [1.];

        lua.set("v", &orig[..]);

        let read: Option<[f32; 2]> = lua.get("v");
        assert_eq!(read, None);
    }

    #[test]
    fn reading_too_small_array_ish_doesnt_work() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { [1] = 1.0, ["foo"] = 2.0 }"#).unwrap();

        let read: Option<[f32; 2]> = lua.get("v");
        assert_eq!(read, None);
    }

    #[test]
    fn reading_array_with_empty_table_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { }"#).unwrap();

        let read: [u32; 0] = lua.get("v").unwrap();
        assert_eq!(read.len(), 0);
    }

    #[test]
    fn reading_nested_array_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { { 1, 2 , 3, 4 }, { 5, 6, 7, 8 } }"#).unwrap();

        let read: [[u32; 4]; 2] = lua.get("v").unwrap();
        assert_eq!(read, [[1, 2, 3, 4], [5, 6, 7, 8]]);

        let read: Vec<[u32; 4]> = lua.get("v").unwrap();
        assert_eq!(read, [[1, 2, 3, 4], [5, 6, 7, 8]]);
    }

    #[test]
    fn writing_array_works() {
        let mut lua = Lua::new();

        let orig: [f64; 3] = [1., 2., 3.];

        lua.set("v", orig);

        let read: [f64; 3] = lua.get("v").unwrap();
        assert_eq!(read, orig);
    }

    #[test]
    fn reading_vec_works() {
        let mut lua = Lua::new();

        let orig = [1., 2., 3.];

        lua.set("v", &orig[..]);

        let read: Vec<_> = lua.get("v").unwrap();
        for (o, r) in orig.iter().zip(read.iter()) {
            if let AnyLuaValue::LuaNumber(ref n) = *r {
                assert_eq!(o, n);
            } else {
                panic!("Unexpected variant");
            }
        }
    }

    #[test]
    fn reading_vec_with_empty_table_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { }"#).unwrap();

        let read: Vec<AnyLuaValue> = lua.get("v").unwrap();
        assert_eq!(read.len(), 0);
    }

    #[test]
    fn reading_nested_vec_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { { { 1, 2 }, { 3, 4 }, { 5, 6 } }, { { 7, 8 } } }"#).unwrap();

        let read: Vec<Vec<Vec<u32>>> = lua.get("v").unwrap();
        assert_eq!(read, [&[[1, 2], [3, 4], [5, 6]][..], &[[7, 8]][..]]);
    }

    #[test]
    fn reading_heterogenous_vec_works() {
        let mut lua = Lua::new();

        let orig = [
            AnyLuaValue::LuaNumber(1.),
            AnyLuaValue::LuaBoolean(false),
            AnyLuaValue::LuaNumber(3.),
            // Pushing String to and reading it from makes it a number
            //AnyLuaValue::LuaString(String::from("3"))
        ];

        lua.set("v", &orig[..]);

        let read: Vec<AnyLuaValue> = lua.get("v").unwrap();
        assert_eq!(read, orig);
    }

    #[test]
    fn reading_vec_set_from_lua_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { 1, 2, 3 }"#).unwrap();

        let read: Vec<AnyLuaValue> = lua.get("v").unwrap();
        assert_eq!(
            read,
            [1., 2., 3.].iter().map(|x| AnyLuaValue::LuaNumber(*x)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reading_vec_types_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"vstr = { "foo", "bar", "baz" }"#).unwrap();
        lua.execute::<()>(r#"vu32 = { 1.0, 2.0, 3.0 }"#).unwrap();
        lua.execute::<()>(r#"vf64 = { 1.5, 2.5, 3.5 }"#).unwrap();

        assert_eq!(lua.get::<Vec<String>, _>("vstr").unwrap(), ["foo", "bar", "baz"]);
        assert_eq!(lua.get::<Vec<u32>, _>("vu32").unwrap(), [1, 2, 3]);
        assert_eq!(lua.get::<Vec<f64>, _>("vf64").unwrap(), [1.5, 2.5, 3.5]);
    }

    #[test]
    fn reading_hashmap_works() {
        let mut lua = Lua::new();

        let orig: HashMap<i32, f64> =
            [1., 2., 3.].iter().enumerate().map(|(k, v)| (k as i32, *v as f64)).collect();
        let orig_copy = orig.clone();
        // Collect to BTreeMap so that iterator yields values in order
        let orig_btree: BTreeMap<_, _> = orig_copy.into_iter().collect();

        lua.set("v", orig);

        let read: HashMap<AnyHashableLuaValue, AnyLuaValue> = lua.get("v").unwrap();
        // Same as above
        let read_btree: BTreeMap<_, _> = read.into_iter().collect();
        for (o, r) in orig_btree.iter().zip(read_btree.iter()) {
            if let (&AnyHashableLuaValue::LuaInteger(i), &AnyLuaValue::LuaNumber(n)) = r {
                let (&o_i, &o_n) = o;
                assert_eq!(o_i, i);
                assert_eq!(o_n, n);
            } else {
                panic!("Unexpected variant");
            }
        }
    }

    #[test]
    fn reading_hashmap_from_sparse_table_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { [-1] = -1, [2] = 2, [42] = 42 }"#).unwrap();

        let read: HashMap<_, _> = lua.get("v").unwrap();
        assert_eq!(read[&AnyHashableLuaValue::LuaInteger(-1)], AnyLuaValue::LuaNumber(-1.));
        assert_eq!(read[&AnyHashableLuaValue::LuaInteger(2)], AnyLuaValue::LuaNumber(2.));
        assert_eq!(read[&AnyHashableLuaValue::LuaInteger(42)], AnyLuaValue::LuaNumber(42.));
        assert_eq!(read.len(), 3);
    }

    #[test]
    fn reading_hashmap_with_empty_table_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { }"#).unwrap();

        let read: HashMap<_, _> = lua.get("v").unwrap();
        assert_eq!(read.len(), 0);
    }

    #[test]
    fn reading_hashmap_with_complex_indexes_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { [-1] = -1, ["foo"] = 2, [2.] = 42 }"#).unwrap();

        let read: HashMap<_, _> = lua.get("v").unwrap();
        assert_eq!(read[&AnyHashableLuaValue::LuaInteger(-1)], AnyLuaValue::LuaNumber(-1.));
        assert_eq!(
            read[&AnyHashableLuaValue::LuaString("foo".to_owned())],
            AnyLuaValue::LuaNumber(2.)
        );
        assert_eq!(read[&AnyHashableLuaValue::LuaInteger(2)], AnyLuaValue::LuaNumber(42.));
        assert_eq!(read.len(), 3);
    }

    #[test]
    #[cfg(feature = "_luaapi_52")]
    fn reading_hashmap_with_floating_indexes_works() {
        let mut lua = Lua::new();
        lua.execute::<()>(r#"v = { [-1.25] = -1, [2.5] = 42 }"#).unwrap();
        let read: HashMap<_, _> = lua.get("v").unwrap();
        // It works by truncating integers in some unspecified way
        // https://www.lua.org/manual/5.2/manual.html#lua_tointegerx
        assert_eq!(read[&AnyHashableLuaValue::LuaInteger(-1)], AnyLuaValue::LuaNumber(-1.));
        assert_eq!(read[&AnyHashableLuaValue::LuaInteger(2)], AnyLuaValue::LuaNumber(42.));
        assert_eq!(read.len(), 2);
    }

    #[test]
    fn reading_heterogenous_hashmap_works() {
        let mut lua = Lua::new();

        let mut orig = HashMap::new();
        orig.insert(AnyHashableLuaValue::LuaInteger(42), AnyLuaValue::LuaNumber(42.));
        orig.insert(
            AnyHashableLuaValue::LuaString("foo".to_owned()),
            AnyLuaValue::LuaString("foo".to_owned()),
        );
        orig.insert(AnyHashableLuaValue::LuaBoolean(true), AnyLuaValue::LuaBoolean(true));

        let orig_clone = orig.clone();
        lua.set("v", orig);

        let read: HashMap<_, _> = lua.get("v").unwrap();
        assert_eq!(read, orig_clone);
    }

    #[test]
    fn reading_hashmap_set_from_lua_works() {
        let mut lua = Lua::new();

        lua.execute::<()>(r#"v = { [1] = 2, [2] = 3, [3] = 4 }"#).unwrap();

        let read: HashMap<_, _> = lua.get("v").unwrap();
        assert_eq!(
            read,
            [2., 3., 4.]
                .iter()
                .enumerate()
                .map(|(k, v)| (
                    AnyHashableLuaValue::LuaInteger((k + 1) as i32),
                    AnyLuaValue::LuaNumber(*v)
                ))
                .collect::<HashMap<_, _>>()
        );
    }
}
