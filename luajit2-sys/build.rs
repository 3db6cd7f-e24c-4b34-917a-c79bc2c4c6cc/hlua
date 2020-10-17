use fs_extra::dir;
use fs_extra::dir::CopyOptions;
use std::env;
use std::process::{Command, Stdio};

fn get_defines() -> Vec<&'static str> {
    vec![
        #[cfg(feature = "ctype_check_anchor")]
        "LUAJIT_CTYPE_CHECK_ANCHOR",
        #[cfg(feature = "debug_ra")]
        "LUAJIT_DEBUG_RA",
        #[cfg(feature = "disable_debuginfo")]
        "LUAJIT_DISABLE_DEBUGINFO",
        #[cfg(feature = "disable_ffi")]
        "LUAJIT_DISABLE_FFI",
        #[cfg(feature = "disable_gc64")]
        "LUAJIT_DISABLE_GC64",
        #[cfg(feature = "disable_jit")]
        "LUAJIT_DISABLE_JIT",
        #[cfg(feature = "disable_jitutil")]
        "LUAJIT_DISABLE_JITUTIL",
        #[cfg(feature = "disable_profile")]
        "LUAJIT_DISABLE_PROFILE",
        #[cfg(feature = "disable_vmevent")]
        "LUAJIT_DISABLE_VMEVENT",
        #[cfg(feature = "enable_checkhook")]
        "LUAJIT_ENABLE_CHECKHOOK",
        #[cfg(feature = "enable_jit")]
        "LUAJIT_ENABLE_JIT",
        #[cfg(feature = "enable_lua52compat")]
        "LUAJIT_ENABLE_LUA52COMPAT",
        #[cfg(feature = "enable_table_bump")]
        "LUAJIT_ENABLE_TABLE_BUMP",
        #[cfg(feature = "no_unaligned")]
        "LUAJIT_NO_UNALIGNED",
        #[cfg(feature = "unwind_external")]
        "LUAJIT_UNWIND_EXTERNAL",
        #[cfg(feature = "security_mcode_insecure")]
        "LUAJIT_SECURITY_MCODE=0",
        #[cfg(feature = "security_mcode_secure")]
        "LUAJIT_SECURITY_MCODE=1",
        #[cfg(feature = "security_prng_insecure")]
        "LUAJIT_SECURITY_PRNG=0",
        #[cfg(feature = "security_prng_secure")]
        "LUAJIT_SECURITY_PRNG=1",
        #[cfg(feature = "security_strhash_sparse")]
        "LUAJIT_SECURITY_STRHASH=0",
        #[cfg(feature = "security_strhash_sparse_dense")]
        "LUAJIT_SECURITY_STRHASH=1",
        #[cfg(feature = "security_strid_linear")]
        "LUAJIT_SECURITY_STRID=0",
        #[cfg(feature = "security_strid_reseed_255")]
        "LUAJIT_SECURITY_STRID=1",
        #[cfg(feature = "security_strid_reseed_15")]
        "LUAJIT_SECURITY_STRID=2",
        #[cfg(feature = "security_strid_random")]
        "LUAJIT_SECURITY_STRID=3",
        #[cfg(feature = "use_gdbjit")]
        "LUAJIT_USE_GDBJIT",
        #[cfg(feature = "use_perftools")]
        "LUAJIT_PERFTOOLS",
        #[cfg(feature = "use_sysmalloc")]
        "LUAJIT_SYSMALLOC",
        #[cfg(feature = "use_valgrind")]
        "LUAJIT_VALGRIND",
        #[cfg(feature = "nummode_single")]
        "LUAJIT_NUMMODE=1",
        #[cfg(feature = "nummode_dual")]
        "LUAJIT_NUMMODE=2",
        #[cfg(feature = "use_apicheck")]
        "LUA_USE_APICHECK",
        #[cfg(feature = "use_assert")]
        "LUA_USE_ASSERT",
    ]
}

fn get_env_args() -> Vec<(&'static str, &'static str)> {
    #[allow(unreachable_patterns)]
    let arg = match true {
        // jit + ffi
        #[cfg(not(any(feature = "disable_ffi", feature = "disable_jit")))]
        true => "-D JIT -D FFI",
        
        // neither
        #[cfg(all(feature = "disable_ffi", feature = "disable_jit"))]
        true => " ",

        // just jit
        #[cfg(feature = "disable_ffi")]
        true => "-D JIT",

        // just ffi
        #[cfg(feature = "disable_jit")]
        true => "-D FFI",

        // no value types in rust :(
        false => unreachable!(),
    };

    vec![("DASMFLAGS_OPTS", arg)]
}

fn main() {
    let luajit_dir = format!("{}/lua", env!("CARGO_MANIFEST_DIR"));
    let out_dir = env::var("OUT_DIR").unwrap();
    let src_dir = format!("{}/lua/src", out_dir);

    dbg!(&luajit_dir);
    dbg!(&out_dir);
    dbg!(&src_dir);

    // DEP_LUAJIT_INCLUDE
    // DEB_LUAJIT_LIB_NAME

    let lib_name = build_luajit(&luajit_dir, &out_dir, &src_dir);

    println!("cargo:lib-name={}", lib_name);
    println!("cargo:include={}", src_dir);
    println!("cargo:rustc-link-search=native={}", src_dir);
    println!("cargo:rustc-link-lib=static={}", lib_name);

    // if cfg!(target_os = "macos") && cfg!(target_pointer_width = "64") {
    //     // RUSTFLAGS='-C link-args=-pagezero_size 10000 -image_base 100000000'
    // }
}

#[cfg(target_env = "msvc")]
fn build_luajit(luajit_dir: &str, out_dir: &str, src_dir: &str) -> &'static str {
    const LIB_NAME: &'static str = "lua51";
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
