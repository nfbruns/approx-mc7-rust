use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().expect("workspace root");

    // Location of the ApproxMC install prefix. Overridable via APPROXMC_PREFIX
    // so consumers outside this workspace (e.g. git/path deps) can point at a
    // system or shared install; defaults to the in-tree build.
    let install = match std::env::var_os("APPROXMC_PREFIX") {
        Some(prefix) => PathBuf::from(prefix),
        None => workspace_root.join("third_party/install"),
    };
    let include = install.join("include");
    // Shared libraries install to lib64 on some distros, lib on others.
    let libdir = {
        let lib64 = install.join("lib64");
        if lib64.join("libapproxmc.so").exists() {
            lib64
        } else {
            install.join("lib")
        }
    };

    let shim = manifest_dir.join("shim/shim.cpp");
    let header = manifest_dir.join("shim/shim.h");

    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .file(&shim)
        .include(manifest_dir.join("shim"))
        .include(&include)
        .compile("approxmc_shim");

    // Link against the pre-built ApproxMC dependency chain. `gmpxx`/`gmp` are
    // required because the shim instantiates ArjunNS::FGenMpq, whose inline
    // C++ code references GMP symbols directly.
    println!("cargo:rustc-link-search=native={}", libdir.display());
    for lib in ["approxmc", "arjun", "cryptominisat5", "gmpxx", "gmp"] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
    // Ensure the shared libraries are found at runtime without LD_LIBRARY_PATH.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", libdir.display());

    println!("cargo:rerun-if-env-changed=APPROXMC_PREFIX");
    println!("cargo:rerun-if-changed={}", shim.display());
    println!("cargo:rerun-if-changed={}", header.display());
}
