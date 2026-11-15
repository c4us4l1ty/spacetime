//! Build the narrow local LD_PRELOAD shim and its probe with the system C compiler.
//!
//! No crates.io build dependency is added. The shim is a local probe, not a security boundary.

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(
        env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR for build scripts"),
    );
    let compiler = env::var_os("CC").unwrap_or_else(|| OsString::from("cc"));
    let shim_source = PathBuf::from("shim/spacetime_shim.c");
    let probe_source = PathBuf::from("shim/spacetime_probe.c");
    let shim = out_dir.join("libspacetime_shim.so");
    let probe = out_dir.join("spacetime_shim_probe");

    let shim_status = Command::new(&compiler)
        .args([
            "-std=c11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-shared",
            "-fPIC",
        ])
        .arg(&shim_source)
        .arg("-o")
        .arg(&shim)
        .arg("-ldl")
        .status()
        .expect("run the system C compiler for the shim");
    assert!(shim_status.success(), "C compiler failed for spacetime_shim.c");

    let probe_status = Command::new(&compiler)
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(&probe_source)
        .arg("-o")
        .arg(&probe)
        .status()
        .expect("run the system C compiler for the probe");
    assert!(probe_status.success(), "C compiler failed for spacetime_probe.c");

    println!("cargo:rustc-env=SPACETIME_SHIM_PATH={}", shim.display());
    println!("cargo:rustc-env=SPACETIME_SHIM_PROBE_PATH={}", probe.display());
    println!("cargo:rerun-if-changed=shim/spacetime_shim.c");
    println!("cargo:rerun-if-changed=shim/spacetime_probe.c");
    println!("cargo:rerun-if-env-changed=CC");
}
