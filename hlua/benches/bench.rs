use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hlua::Lua;

fn read_vec(name: &str, lua: &mut Lua) -> Vec<u32> {
    lua.get::<Vec<u32>, _>(name).unwrap()
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut lua = Lua::new();
    lua.set("vec1",   (0..1).collect::<Vec<u32>>());
    lua.set("vec10",  (0..10).collect::<Vec<u32>>());
    lua.set("vec100", (0..100).collect::<Vec<u32>>());

    c.bench_function("vec 1",   |b| b.iter(|| read_vec("vec1",   black_box(&mut lua))));
    c.bench_function("vec 10",  |b| b.iter(|| read_vec("vec10",  black_box(&mut lua))));
    c.bench_function("vec 100", |b| b.iter(|| read_vec("vec100", black_box(&mut lua))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
