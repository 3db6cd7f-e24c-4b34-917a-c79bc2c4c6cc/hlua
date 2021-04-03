use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hlua::Lua;

fn read_vec(name: &str, lua: &mut Lua) -> Vec<u32> {
    lua.get::<Vec<u32>, _>(name).unwrap()
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut lua = Lua::new();
    lua.set("vec1", (0..1).collect::<Vec<u32>>());
    lua.set("vec10", (0..10).collect::<Vec<u32>>());
    lua.set("vec100", (0..100).collect::<Vec<u32>>());

    c.bench_function("vec 1", |b| b.iter(|| read_vec("vec1", black_box(&mut lua))));
    c.bench_function("vec 10", |b| b.iter(|| read_vec("vec10", black_box(&mut lua))));
    c.bench_function("vec 100", |b| b.iter(|| read_vec("vec100", black_box(&mut lua))));

    lua.set("func", hlua::function0(|| 1));
    c.bench_function("[lua -> c] call func(): 1 (x10000)", |b| {
        b.iter(|| {
            lua.execute::<()>("for i=0,10000 do func() end").unwrap();
        })
    });

    lua.set("func", hlua::function1(|x: u32| x));
    c.bench_function("[lua -> c] call func(u32): u32 (x10000)", |b| {
        b.iter(|| {
            lua.execute::<()>("for i=0,10000 do func(1) end").unwrap();
        })
    });

    #[derive(Copy, Clone)]
    struct Foo(u32);

    hlua::implement_lua_read!(Foo);
    hlua::implement_lua_push!(Foo, |_| {});

    lua.set("val", Foo(1));
    lua.set("func", hlua::function1(|x: &Foo| *x));
    c.bench_function("[lua -> c] call func(Foo): Foo (x10000)", |b| {
        b.iter(|| {
            lua.execute::<()>("for i=0,10000 do func(val) end").unwrap();
        })
    });

    lua.set("func", hlua::function1(|_: &Foo| {}));
    c.bench_function("[lua -> c] call func(Foo) (x10000)", |b| {
        b.iter(|| {
            lua.execute::<()>("for i=0,10000 do func(val) end").unwrap();
        })
    });

    lua.set("func", hlua::function0(|| Foo(1)));
    c.bench_function("[lua -> c] call func(): Foo (x10000)", |b| {
        b.iter(|| {
            lua.execute::<()>("for i=0,10000 do func() end").unwrap();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
