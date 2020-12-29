use fs_extra::dir;
use fs_extra::dir::CopyOptions;
use std::process::{Command, Stdio};
use std::{env, path::PathBuf};

#[rustfmt::skip]
fn get_defines() -> Vec<&'static str> {
    vec![
        // LuaJIT defines
        #[cfg(feature = "ctype_check_anchor")]            "LUAJIT_CTYPE_CHECK_ANCHOR",
        #[cfg(feature = "debug_ra")]                      "LUAJIT_DEBUG_RA",
        #[cfg(feature = "disable_debuginfo")]             "LUAJIT_DISABLE_DEBUGINFO",
        #[cfg(feature = "disable_ffi")]                   "LUAJIT_DISABLE_FFI",
        #[cfg(feature = "disable_gc64")]                  "LUAJIT_DISABLE_GC64",
        #[cfg(feature = "disable_jit")]                   "LUAJIT_DISABLE_JIT",
        #[cfg(feature = "disable_jitutil")]               "LUAJIT_DISABLE_JITUTIL",
        #[cfg(feature = "disable_profile")]               "LUAJIT_DISABLE_PROFILE",
        #[cfg(feature = "disable_vmevent")]               "LUAJIT_DISABLE_VMEVENT",
        #[cfg(feature = "enable_checkhook")]              "LUAJIT_ENABLE_CHECKHOOK",
        #[cfg(feature = "enable_jit")]                    "LUAJIT_ENABLE_JIT",
        #[cfg(feature = "enable_lua52compat")]            "LUAJIT_ENABLE_LUA52COMPAT",
        #[cfg(feature = "enable_table_bump")]             "LUAJIT_ENABLE_TABLE_BUMP",
        #[cfg(feature = "no_unaligned")]                  "LUAJIT_NO_UNALIGNED",
        #[cfg(feature = "unwind_external")]               "LUAJIT_UNWIND_EXTERNAL",
        #[cfg(feature = "security_mcode_insecure")]       "LUAJIT_SECURITY_MCODE=0",
        #[cfg(feature = "security_mcode_secure")]         "LUAJIT_SECURITY_MCODE=1",
        #[cfg(feature = "security_prng_insecure")]        "LUAJIT_SECURITY_PRNG=0",
        #[cfg(feature = "security_prng_secure")]          "LUAJIT_SECURITY_PRNG=1",
        #[cfg(feature = "security_strhash_sparse")]       "LUAJIT_SECURITY_STRHASH=0",
        #[cfg(feature = "security_strhash_sparse_dense")] "LUAJIT_SECURITY_STRHASH=1",
        #[cfg(feature = "security_strid_linear")]         "LUAJIT_SECURITY_STRID=0",
        #[cfg(feature = "security_strid_reseed_255")]     "LUAJIT_SECURITY_STRID=1",
        #[cfg(feature = "security_strid_reseed_15")]      "LUAJIT_SECURITY_STRID=2",
        #[cfg(feature = "security_strid_random")]         "LUAJIT_SECURITY_STRID=3",
        #[cfg(feature = "use_gdbjit")]                    "LUAJIT_USE_GDBJIT",
        #[cfg(feature = "use_perftools")]                 "LUAJIT_PERFTOOLS",
        #[cfg(feature = "use_sysmalloc")]                 "LUAJIT_SYSMALLOC",
        #[cfg(feature = "use_valgrind")]                  "LUAJIT_VALGRIND",
        #[cfg(feature = "nummode_single")]                "LUAJIT_NUMMODE=1",
        #[cfg(feature = "nummode_dual")]                  "LUAJIT_NUMMODE=2",

        // Lua defines
        #[cfg(feature = "use_apicheck")]                  "LUA_USE_APICHECK",
        #[cfg(feature = "use_assert")]                    "LUA_USE_ASSERT",

        // Our own defines
        #[cfg(feature = "disable_dylibs")]                "WB_DISABLE_DYLIBS",
        #[cfg(feature = "disable_lib_base")]              "WB_DISABLE_LIB_BASE",
        #[cfg(feature = "disable_lib_package")]           "WB_DISABLE_LIB_PACKAGE",
        #[cfg(feature = "disable_lib_table")]             "WB_DISABLE_LIB_TABLE",
        #[cfg(feature = "disable_lib_io")]                "WB_DISABLE_LIB_IO",
        #[cfg(feature = "disable_lib_os")]                "WB_DISABLE_LIB_OS",
        #[cfg(feature = "disable_lib_string")]            "WB_DISABLE_LIB_STRING",
        #[cfg(feature = "disable_lib_math")]              "WB_DISABLE_LIB_MATH",
        #[cfg(feature = "disable_lib_debug")]             "WB_DISABLE_LIB_DEBUG",
        #[cfg(feature = "disable_lib_bit")]               "WB_DISABLE_LIB_BIT",
        #[cfg(feature = "disable_lib_jit")]               "WB_DISABLE_LIB_JIT",
    ]
}

fn get_env_args() -> Vec<(&'static str, &'static str)> {
    #[allow(unreachable_patterns)]
    let arg = match true {
        // jit + ffi
        #[cfg(not(any(feature = "disable_ffi", feature = "disable_jit")))]
        _ => "-D JIT -D FFI",

        // neither
        #[cfg(all(feature = "disable_ffi", feature = "disable_jit"))]
        _ => " ",

        // just jit
        #[cfg(feature = "disable_ffi")]
        _ => "-D JIT",

        // just ffi
        #[cfg(feature = "disable_jit")]
        _ => "-D FFI",
    };

    vec![("DASMFLAGS_OPTS", arg)]
}

fn main() {
    let wrapper_name = "ffi.h";
    let luajit_dir = format!("{}/lua", env!("CARGO_MANIFEST_DIR"));
    let out_dir = env::var("OUT_DIR").unwrap();
    let src_dir = format!("{}/lua/src", out_dir);

    dbg!(&luajit_dir);
    dbg!(&out_dir);
    dbg!(&src_dir);

    // DEP_LUAJIT_INCLUDE
    // DEB_LUAJIT_LIB_NAME

    generate_bindings(wrapper_name);

    let lib_name = build_luajit(&luajit_dir, &out_dir, &src_dir);

    println!("cargo:lib-name={}", lib_name);
    println!("cargo:include={}", src_dir);
    println!("cargo:rustc-link-search=native={}", src_dir);
    println!("cargo:rustc-link-lib=static={}", lib_name);
    println!("cargo:rerun-if-changed={}", wrapper_name);

    // if cfg!(target_os = "macos") && cfg!(target_pointer_width = "64") {
    //     // RUSTFLAGS='-C link-args=-pagezero_size 10000 -image_base 100000000'
    // }
}

fn generate_bindings(header_name: &str) {
    let bindings = bindgen::Builder::default()
        .whitelist_var("LUA.*")
        .whitelist_var("LUAJIT.*")

        .whitelist_type("lua_.*")
        .whitelist_type("luaL_.*")

        .whitelist_function("lua_.*")
        .whitelist_function("luaL_.*")
        .whitelist_function("luaJIT.*")
        .whitelist_function("luaopen.*")

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

#[cfg(target_env = "msvc")]
fn build_luajit(luajit_dir: &str, out_dir: &str, src_dir: &str) -> &'static str {
    const LIB_NAME: &str = "lua51";
    let lib_path = format!("{}/{}.lib", &src_dir, LIB_NAME);
    dbg!(&lib_path);
    if !std::fs::metadata(&lib_path).is_ok() {
        let target = env::var("TARGET").unwrap();
        let cl_exe: cc::Tool =
            cc::windows_registry::find_tool(&target, "cl.exe").expect("cl.exe not found");
        let msvcbuild_bat = format!("{}/msvcbuild.bat", &src_dir);
        dbg!(&msvcbuild_bat);

        let mut copy_options = CopyOptions::new();
        copy_options.overwrite = true;
        dir::copy(&luajit_dir, &out_dir, &copy_options)
            .expect("failed to copy luajit source to out dir");

        let mut buildcmd = Command::new(msvcbuild_bat);
        for (name, value) in cl_exe.env() {
            eprintln!("{:?} = {:?}", name, value);
            buildcmd.env(name, value);
        }

        // Add custom defines to the command line
        let defines: Vec<String> = get_defines().iter().map(|x| format!("-D{}", x)).collect();
        buildcmd.env("CL", defines.join(" "));
        // Ensure dynasm gets passed the right arguments - this seems to be taken care of automatically when using make
        buildcmd.envs(get_env_args());

        buildcmd.env("Configuration", "Release");
        buildcmd.args(&["static"]);
        buildcmd.current_dir(&src_dir);
        buildcmd.stderr(Stdio::inherit());

        let mut child = buildcmd.spawn().expect("failed to run msvcbuild.bat");

        if !child
            .wait()
            .map(|status| status.success())
            .map_err(|_| false)
            .unwrap_or(false)
        {
            panic!("Failed to build luajit");
        }
    }

    LIB_NAME
}

#[cfg(not(target_env = "msvc"))]
fn build_luajit(luajit_dir: &str, out_dir: &str, src_dir: &str) -> &'static str {
    const LIB_NAME: &'static str = "luajit";
    let lib_path = format!("{}/lib{}.a", &src_dir, LIB_NAME);
    dbg!(&lib_path);
    if !std::fs::metadata(&lib_path).is_ok() {
        let mut copy_options = CopyOptions::new();
        copy_options.overwrite = true;
        dir::copy(&luajit_dir, &out_dir, &copy_options).unwrap();
        std::fs::copy(format!("etc/Makefile"), format!("{}/Makefile", &src_dir)).unwrap();

        let mut buildcmd = Command::new("make");
        buildcmd.current_dir(&src_dir);
        buildcmd.stderr(Stdio::inherit());

        // TODO: Handle defines

        if cfg!(target_pointer_width = "32") {
            buildcmd.env("HOST_CC", "gcc -m32");
            buildcmd.arg("-e");
        }

        let mut child = buildcmd.spawn().expect("failed to run make");

        if !child
            .wait()
            .map(|status| status.success())
            .map_err(|_| false)
            .unwrap_or(false)
        {
            panic!("Failed to build luajit");
        }
    }
    LIB_NAME
}
