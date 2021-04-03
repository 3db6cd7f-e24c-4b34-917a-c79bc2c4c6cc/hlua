use crate::{AsLua, AsMutLua};

use crate::{LuaRead, Push, PushGuard, PushOne, Void};

macro_rules! tuple_impl {
    ($ty:ident) => (
        impl<'lua, LU, $ty> Push<LU> for ($ty,) where LU: AsMutLua<'lua>, $ty: Push<LU> {
            type Err = <$ty as Push<LU>>::Err;

            #[inline]
            fn push_to_lua(self, lua: LU) -> Result<PushGuard<LU>, (Self::Err, LU)> {
                self.0.push_to_lua(lua)
            }
        }

        impl<'lua, LU, $ty> PushOne<LU> for ($ty,) where LU: AsMutLua<'lua>, $ty: PushOne<LU> {
        }

        impl<'lua, LU, $ty> LuaRead<LU> for ($ty,) where LU: AsMutLua<'lua>, $ty: LuaRead<LU> {
            #[inline]
            fn lua_read_at_position(lua: LU, index: i32) -> Result<($ty,), LU> {
                LuaRead::lua_read_at_position(lua, index).map(|v| (v,))
            }
        }
    );

    ($first:ident, $($other:ident),+) => (
        #[allow(non_snake_case)]
        impl<'lua, LU, FE, OE, $first, $($other),+> Push<LU> for ($first, $($other),+)
            where LU: AsMutLua<'lua>,
                  $first: for<'a> Push<&'a mut LU, Err = FE>,
                  ($($other,)+): for<'a> Push<&'a mut LU, Err = OE>
        {
            type Err = TuplePushError<FE, OE>;

            #[inline]
            fn push_to_lua(self, mut lua: LU) -> Result<PushGuard<LU>, (Self::Err, LU)> {
                match self {
                    ($first, $($other),+) => {
                        let mut total = 0;

                        let first_err = match $first.push_to_lua(&mut lua) {
                            Ok(pushed) => { total += pushed.forget_internal(); None },
                            Err((err, _)) => Some(err),
                        };

                        if let Some(err) = first_err {
                            return Err((TuplePushError::First(err), lua));
                        }

                        let rest = ($($other,)+);
                        let other_err = match rest.push_to_lua(&mut lua) {
                            Ok(pushed) => { total += pushed.forget_internal(); None },
                            Err((err, _)) => Some(err),
                        };

                        if let Some(err) = other_err {
                            return Err((TuplePushError::Other(err), lua));
                        }

                        let raw_lua = lua.as_lua();
                        Ok(PushGuard { lua, size: total, raw_lua })
                    }
                }
            }
        }

        // TODO: what if T or U are also tuples? indices won't match
        #[allow(unused_assignments)]
        #[allow(non_snake_case)]
        impl<'lua, LU, $first: for<'a> LuaRead<&'a mut LU>, $($other: for<'a> LuaRead<&'a mut LU>),+>
            LuaRead<LU> for ($first, $($other),+) where LU: AsLua<'lua>
        {
            #[inline]
            fn lua_read_at_position(mut lua: LU, index: i32) -> Result<($first, $($other),+), LU> {
                let negative = index.is_negative();
                let mut i = index;

                let $first: $first = match LuaRead::lua_read_at_position(&mut lua, i) {
                    Ok(v) => v,
                    Err(_) => return Err(lua)
                };

                i += 1;

                $(
                    let $other: $other = {
                        // Prevent wrapping around if we're reading too far into the stack (-2, -1, 0, 1, ...)
                        let read = if negative == i.is_negative() {
                            LuaRead::lua_read_at_position(&mut lua, i)
                        } else {
                            LuaRead::lua_read_out_of_bounds(&mut lua)
                        };

                        match read {
                            Ok(v) => v,
                            Err(_) => return Err(lua)
                        }
                    };
                    i += 1;
                )+

                Ok(($first, $($other),+))

            }
        }

        tuple_impl!($($other),+);
    );
}

tuple_impl!(A, B, C, D, E, F, G, H, I, J, K, L, M);

/// Error that can happen when pushing multiple values at once.
// TODO: implement Error on that thing
#[derive(Debug, Copy, Clone)]
pub enum TuplePushError<C, O> {
    First(C),
    Other(O),
}

impl From<TuplePushError<Void, Void>> for Void {
    #[inline]
    fn from(_: TuplePushError<Void, Void>) -> Void {
        unreachable!()
    }
}

#[test]
fn no_stack_wrap() {
    let mut lua = crate::Lua::new();

    lua.set(
        "foo",
        crate::function3(|a: u32, b: Option<f32>, c: Option<f32>| {
            a == 10 && b.is_none() && c.is_none()
        }),
    );

    assert_eq!(lua.execute::<bool>("return foo(10,  20,  30)").unwrap(), false);
    assert_eq!(lua.execute::<bool>("return foo(10, nil, nil)").unwrap(), true);
    assert_eq!(lua.execute::<bool>("return foo(10, nil)").unwrap(), true);
    assert_eq!(lua.execute::<bool>("return foo(10)").unwrap(), true);
}

// TODO: Fix nested tuples!
// #[test]
// fn reading_tuple_vec_works() {
//     let mut lua = crate::Lua::new();

//     lua.execute::<()>(r#"v = { { 1, 2 }, { 3, 4 } }"#).unwrap();

//     let read: Vec<(u32, u32)> = lua.get("v").unwrap();
//     assert_eq!(read, [(1,2), (3,4)]);
// }

// #[test]
// fn reading_nested_tuple_works() {
//     let mut lua = crate::Lua::new();

//     lua.execute::<()>(r#"v = { { 1, 2 }, { 3, 4 } }"#).unwrap();

//     let read: ((u32, u32), (u32, u32)) = lua.get("v").unwrap();
//     assert_eq!(read, ((1,2), (3,4)));
// }
