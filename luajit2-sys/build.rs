use cc::windows_registry::find;
use fs_extra::{dir, dir::CopyOptions};
use std::{
    env,
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    process::Command,
};

#[rustfmt::skip]
fn with_defines(cc: &mut cc::Build) -> &mut cc::Build {
    cc  // LuaJIT defines
        .define_if(feature("ctype_check_anchor"),            "LUAJIT_CTYPE_CHECK_ANCHOR", None)
        .define_if(feature("debug_ra"),                      "LUAJIT_DEBUG_RA", None)
        .define_if(feature("disable_debuginfo"),             "LUAJIT_DISABLE_DEBUGINFO", None)
        .define_if(feature("disable_ffi"),                   "LUAJIT_DISABLE_FFI", None)
        .define_if(feature("disable_gc64"),                  "LUAJIT_DISABLE_GC64", None)
        .define_if(feature("disable_jit"),                   "LUAJIT_DISABLE_JIT", None)
        .define_if(feature("disable_jitutil"),               "LUAJIT_DISABLE_JITUTIL", None)
        .define_if(feature("disable_profile"),               "LUAJIT_DISABLE_PROFILE", None)
        .define_if(feature("disable_vmevent"),               "LUAJIT_DISABLE_VMEVENT", None)
        .define_if(feature("enable_checkhook"),              "LUAJIT_ENABLE_CHECKHOOK", None)
        .define_if(feature("enable_jit"),                    "LUAJIT_ENABLE_JIT", None)
        .define_if(feature("enable_lua52compat"),            "LUAJIT_ENABLE_LUA52COMPAT", None)
        .define_if(feature("enable_table_bump"),             "LUAJIT_ENABLE_TABLE_BUMP", None)
        .define_if(feature("no_unaligned"),                  "LUAJIT_NO_UNALIGNED", None)
        .define_if(feature("no_unwind"),                     "LUAJIT_NO_UNWIND", None)
        .define_if(feature("unwind_external"),               "LUAJIT_UNWIND_EXTERNAL", None)
        .define_if(feature("security_mcode_insecure"),       "LUAJIT_SECURITY_MCODE", Some("0"))
        .define_if(feature("security_mcode_secure"),         "LUAJIT_SECURITY_MCODE", Some("1"))
        .define_if(feature("security_prng_insecure"),        "LUAJIT_SECURITY_PRNG", Some("0"))
        .define_if(feature("security_prng_secure"),          "LUAJIT_SECURITY_PRNG", Some("1"))
        .define_if(feature("security_strhash_sparse"),       "LUAJIT_SECURITY_STRHASH", Some("0"))
        .define_if(feature("security_strhash_sparse_dense"), "LUAJIT_SECURITY_STRHASH", Some("1"))
        .define_if(feature("security_strid_linear"),         "LUAJIT_SECURITY_STRID", Some("0"))
        .define_if(feature("security_strid_reseed_255"),     "LUAJIT_SECURITY_STRID", Some("1"))
        .define_if(feature("security_strid_reseed_15"),      "LUAJIT_SECURITY_STRID", Some("2"))
        .define_if(feature("security_strid_random"),         "LUAJIT_SECURITY_STRID", Some("3"))
        .define_if(feature("use_gdbjit"),                    "LUAJIT_USE_GDBJIT", None)
        .define_if(feature("use_perftools"),                 "LUAJIT_USE_PERFTOOLS", None)
        .define_if(feature("use_sysmalloc"),                 "LUAJIT_USE_SYSMALLOC", None)
        .define_if(feature("use_valgrind"),                  "LUAJIT_USE_VALGRIND", None)
        .define_if(feature("nummode_single"),                "LUAJIT_NUMMODE", Some("1"))
        .define_if(feature("nummode_dual"),                  "LUAJIT_NUMMODE", Some("2"))
        // Lua defines
        .define_if(feature("use_apicheck"),                  "LUA_USE_APICHECK", None)
        .define_if(feature("use_assert"),                    "LUA_USE_ASSERT", None)
        // Custom defines
        .define_if(feature("disable_dylibs"),                "WB_DISABLE_DYLIBS", None)
        .define_if(feature("disable_lib_base"),              "WB_DISABLE_LIB_BASE", None)
        .define_if(feature("disable_lib_package"),           "WB_DISABLE_LIB_PACKAGE", None)
        .define_if(feature("disable_lib_table"),             "WB_DISABLE_LIB_TABLE", None)
        .define_if(feature("disable_lib_io"),                "WB_DISABLE_LIB_IO", None)
        .define_if(feature("disable_lib_os"),                "WB_DISABLE_LIB_OS", None)
        .define_if(feature("disable_lib_string"),            "WB_DISABLE_LIB_STRING", None)
        .define_if(feature("disable_lib_math"),              "WB_DISABLE_LIB_MATH", None)
        .define_if(feature("disable_lib_debug"),             "WB_DISABLE_LIB_DEBUG", None)
        .define_if(feature("disable_lib_bit"),               "WB_DISABLE_LIB_BIT", None)
        .define_if(feature("disable_lib_jit"),               "WB_DISABLE_LIB_JIT", None)

        .define_if(feature("disable_func_loadfile"),         "WB_DISABLE_FUNC_LOADFILE", None)
        .define_if(feature("disable_func_debug_debug"),      "WB_DISABLE_FUNC_DEBUG_DEBUG", None)

}

fn main() {
    generate_bindings("ffi.h");
    println!("cargo:rerun-if-changed=ffi.h");
    println!("cargo:rerun-if-env-changed=CXX");

    let base_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    let lib_name = env::var("CARGO_MANIFEST_LINKS").unwrap();

    build_luajit(&lib_name, Path::new(&base_dir).join("lua")).unwrap();

    println!("cargo:lib-name={}", lib_name);
    println!("cargo:rustc-link-lib=static={}", lib_name);
}

fn generate_bindings(header_name: &str) {
    let bindings = bindgen::Builder::default()
        .allowlist_var("LUA.*")
        .allowlist_var("LUAJIT.*")
        .allowlist_type("lua_.*")
        .allowlist_type("luaL_.*")
        .allowlist_function("lua_.*")
        .allowlist_function("luaL_.*")
        .allowlist_function("luaJIT.*")
        .allowlist_function("luaopen.*")
        .ctypes_prefix("libc")
        .use_core()
        .impl_debug(true)
        .size_t_is_usize(true)
        .default_macro_constant_type(bindgen::MacroTypeVariation::Signed)
        .header(header_name)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

fn build_luajit(lib_name: &str, luajit_dir: impl AsRef<Path>) -> io::Result<()> {
    let target = &env::var("TARGET").unwrap();
    let outdir = env::var_os("OUT_DIR").unwrap();

    let is_windows = env::var("CARGO_CFG_WINDOWS").is_ok();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    let outdir = Path::new(&outdir);
    let luadir = outdir.join("lua");

    dir::copy(luajit_dir, &outdir, &CopyOptions { overwrite: true, ..Default::default() })
        .expect("failed to copy luajit source to out dir");

    cc::Build::new()
        .cargo_metadata(false)
        .define("_CRT_SECURE_NO_DEPRECATE", None)
        .define("_CRT_STDIO_INLINE=__inline", None)
        .file(outdir.join("lua/src/host/minilua.c"))
        .compile("minilua");

    linker(&target)
        .current_dir(&outdir)
        .arg("/nologo")
        .arg("/out:minilua.exe")
        .arg(outdir.join("lua/src/host/minilua.o"))
        .log()
        .spawn()?
        .wait()?;

    #[rustfmt::skip]
    Command::new(outdir.join("minilua.exe"))
        .current_dir(outdir.join("lua/src/"))
        .arg("../dynasm/dynasm.lua")
        .arg("-LN")
        .args(["-o", "host/buildvm_arch.h"])
        .args_if(is_windows,              ["-D", "WIN"])
        .args_if(!feature("disable_ffi"), ["-D", "FFI"])
        .args_if(!feature("disable_jit"), ["-D", "JIT"])
        .args_if(feature("no_unwind"),    ["-D", "NO_UNWIND"])
        .args_if_each([
            (target_arch == "x86_64", ["-D", "P64"]),
        ])
        .args_if_each([
            (target_arch == "x86_64", ["vm_x64.dasc"]),
            (target_arch == "x86",    ["vm_x86.dasc"]),
        ])
        .log()
        .spawn()?
        .wait()?;

    with_defines(&mut cc::Build::new())
        .cargo_metadata(false)
        .define("_CRT_SECURE_NO_DEPRECATE", None)
        .define("_CRT_STDIO_INLINE=__inline", None)
        .include(luadir.join("src"))
        .include(luadir.join("src/host"))
        .include(luadir.join("dynasm"))
        .file(luadir.join("src/host/buildvm.c"))
        .file(luadir.join("src/host/buildvm_asm.c"))
        .file(luadir.join("src/host/buildvm_peobj.c"))
        .file(luadir.join("src/host/buildvm_lib.c"))
        .file(luadir.join("src/host/buildvm_fold.c"))
        .out_dir(outdir.join("buildvm"))
        .compile("buildvm");

    linker(&target)
        .current_dir(outdir.join("buildvm"))
        .arg("/nologo")
        .arg("/out:../buildvm.exe")
        .arg("buildvm*.o")
        .log()
        .spawn()?
        .wait()?;

    #[rustfmt::skip]
    let buildvm = [
        //mode       output          inputs
        ("peobj",   "lj_vm.obj",     &[][..]),
        ("folddef", "lj_folddef.h",  &["lj_opt_fold.c"][..]),

        ("bcdef",   "lj_bcdef.h",    &["lib_base.c", "lib_math.c", "lib_bit.c", "lib_string.c", "lib_table.c", "lib_io.c", "lib_os.c", "lib_package.c", "lib_debug.c", "lib_jit.c", "lib_ffi.c", "lib_buffer.c"][..]),
        ("ffdef",   "lj_ffdef.h",    &["lib_base.c", "lib_math.c", "lib_bit.c", "lib_string.c", "lib_table.c", "lib_io.c", "lib_os.c", "lib_package.c", "lib_debug.c", "lib_jit.c", "lib_ffi.c", "lib_buffer.c"][..]),
        ("libdef",  "lj_libdef.h",   &["lib_base.c", "lib_math.c", "lib_bit.c", "lib_string.c", "lib_table.c", "lib_io.c", "lib_os.c", "lib_package.c", "lib_debug.c", "lib_jit.c", "lib_ffi.c", "lib_buffer.c"][..]),
        ("recdef",  "lj_recdef.h",   &["lib_base.c", "lib_math.c", "lib_bit.c", "lib_string.c", "lib_table.c", "lib_io.c", "lib_os.c", "lib_package.c", "lib_debug.c", "lib_jit.c", "lib_ffi.c", "lib_buffer.c"][..]),
        ("vmdef",   "jit/vmdef.lua", &["lib_base.c", "lib_math.c", "lib_bit.c", "lib_string.c", "lib_table.c", "lib_io.c", "lib_os.c", "lib_package.c", "lib_debug.c", "lib_jit.c", "lib_ffi.c", "lib_buffer.c"][..]),
    ];

    for (mode, output, inputs) in buildvm {
        Command::new(outdir.join("buildvm.exe"))
            .current_dir(luadir.join("src")) //
            .args(["-m", mode])
            .args(["-o", output])
            .args(inputs)
            .log()
            .spawn()?;
    }

    with_defines(&mut cc::Build::new())
        .define("_CRT_SECURE_NO_DEPRECATE", None)
        .define("_CRT_STDIO_INLINE", "__inline")
        .files(glob(luadir.join("src/lj_*.c")))
        .files(glob(luadir.join("src/lib_*.c")))
        .object(luadir.join("src/lj_vm.obj"))
        // The CC crate defaults to IA32 when using clang-cl, which is a ridiculous default.
        .flag_if_supported("-arch:AVX2")
        .compile(lib_name);

    Ok(())
}

fn linker(target: impl AsRef<str>) -> Command {
    let msvc = find(target.as_ref(), "link.exe");

    if let Some(exe) = env::var("RUSTC_LINKER").ok() {
        let mut command = Command::new(exe);

        // Steal environment variables, this helps with finding libraries on Windows.
        // We could resolve these ourselves but stealing them from `cc` is easier.
        for (key, val) in msvc.iter().flat_map(|x| x.get_envs()) {
            match val {
                Some(val) => command.env(key, val),
                None => command.env_remove(key),
            };
        }

        command
    } else {
        msvc.expect("failed to find linker")
    }
}

fn envize(string: impl AsRef<str>) -> String {
    string.as_ref().to_ascii_uppercase().replace("-", "_")
}

fn feature(feature: &str) -> bool {
    env::var(format!("CARGO_FEATURE_{}", envize(feature))).is_ok()
}

fn glob(path: impl AsRef<Path>) -> impl IntoIterator<Item = PathBuf> {
    let path = path.as_ref().to_str().unwrap();
    glob::glob(path).expect("Failed to read glob pattern").filter_map(Result::ok)
}

trait BuildExt {
    fn define_if<'a>(
        &mut self,
        cond: bool,
        var: &str,
        val: impl Into<Option<&'a str>>,
    ) -> &mut Self;
}

impl BuildExt for cc::Build {
    fn define_if<'a>(
        &mut self,
        cond: bool,
        var: &str,
        val: impl Into<Option<&'a str>>,
    ) -> &mut Self {
        if cond {
            self.define(var, val)
        } else {
            self
        }
    }
}

trait CommandExt {
    fn arg_if(&mut self, cond: bool, arg: impl AsRef<OsStr>) -> &mut Self;
    fn args_if(
        &mut self,
        cond: bool,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> &mut Self;
    fn args_if_each(
        &mut self,
        args: impl IntoIterator<Item = (bool, impl IntoIterator<Item = impl AsRef<OsStr>>)>,
    ) -> &mut Self;

    fn log(&mut self) -> &mut Self;
}

impl CommandExt for Command {
    fn arg_if(&mut self, cond: bool, arg: impl AsRef<OsStr>) -> &mut Self {
        if cond {
            self.arg(arg)
        } else {
            self
        }
    }

    fn args_if(
        &mut self,
        cond: bool,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> &mut Self {
        if cond {
            self.args(args)
        } else {
            self
        }
    }

    fn args_if_each(
        &mut self,
        args: impl IntoIterator<Item = (bool, impl IntoIterator<Item = impl AsRef<OsStr>>)>,
    ) -> &mut Self {
        for (cond, arg) in args {
            self.args_if(cond, arg);
        }
        self
    }

    fn log(&mut self) -> &mut Self {
        eprintln!("command: {:?}", self);
        self
    }
}
