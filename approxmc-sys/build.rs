use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Recursively collect every `*.a` archive under `dir`, keyed by library name
/// (e.g. `libapproxmc.a` -> `approxmc`). First one found wins, which prefers
/// the installed copies over any duplicate build-tree intermediates since
/// `install/` is walked before `build/` by the caller.
fn collect_static_libs(dir: &Path, out: &mut BTreeMap<String, PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_static_libs(&path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("a") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(name) = stem.strip_prefix("lib") else {
            continue;
        };
        out.entry(name.to_string()).or_insert_with(|| path.clone());
    }
}

// The public C API this crate exposes to Rust (see shim/shim.h). Every other
// symbol pulled in from the vendored C++ stack gets stripped down to local
// linkage below, so only these names survive into the final static archive.
const SHIM_EXPORTS: &[&str] = &[
    "approxmc_shim_new",
    "approxmc_shim_free",
    "approxmc_shim_set_epsilon",
    "approxmc_shim_set_delta",
    "approxmc_shim_set_seed",
    "approxmc_shim_nvars",
    "approxmc_shim_new_vars",
    "approxmc_shim_add_clause",
    "approxmc_shim_set_sampling_vars",
    "approxmc_shim_count",
];

fn run(cmd: &mut Command) {
    let status = cmd.status().unwrap_or_else(|e| {
        panic!("failed to run {:?}: {e}", cmd.get_program());
    });
    assert!(
        status.success(),
        "{:?} failed with {status}",
        cmd.get_program()
    );
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Fetches ApproxMC from GitHub and builds it (plus CryptoMiniSat5, Arjun,
    // SBVA, cadical/cadiback, pulled in transitively via ApproxMC's own
    // FetchContent) as static libraries. No manual clone step, no rpath: the
    // whole dependency chain gets embedded directly into this crate's output.
    // Always Release: a SAT/model-counting stack built at -O0 is unusably
    // slow, regardless of the Rust crate's own dev/release profile.
    let dst = cmake::Config::new(manifest_dir.join("vendor"))
        .profile("Release")
        .build();

    let include = dst.join("include");
    let mut libs = BTreeMap::new();
    // Installed tree first (headers + most libs), then the raw build tree as
    // a fallback/supplement for any static archive that a dependency built
    // but didn't install (CMake install() rules for transitive third-party
    // deps aren't guaranteed complete).
    for sub in ["lib64", "lib"] {
        let d = dst.join(sub);
        if d.is_dir() {
            collect_static_libs(&d, &mut libs);
        }
    }
    let build_dir = dst.join("build");
    if build_dir.is_dir() {
        collect_static_libs(&build_dir, &mut libs);
    }
    if libs.is_empty() {
        panic!("no static libraries (*.a) found under {}", dst.display());
    }
    // `oracle`'s objects are compiled directly into `cryptominisat5` (not
    // merely linked against it), so whole-archiving both is a duplicate
    // symbol error. Its symbols are already provided by cryptominisat5.
    libs.remove("oracle");

    let shim = manifest_dir.join("shim/shim.cpp");
    let header = manifest_dir.join("shim/shim.h");

    // Compile the shim without going through `cc::Build::compile()`: that
    // helper prints its own `cargo:rustc-link-lib=static=...` directive,
    // which would statically bundle a second, un-stripped copy of the shim
    // (and thus try to pull in the C++ stack a second time) alongside the
    // combined archive built below. `get_compiler()` just resolves the
    // compiler + flags (opt level, `-fPIC`, `-std=c++20`, include paths)
    // without emitting any cargo directives, so we drive the actual
    // compilation ourselves.
    let compiler = cc::Build::new()
        .cpp(true)
        .std("c++20")
        .include(manifest_dir.join("shim"))
        .include(&include)
        .get_compiler();
    let shim_obj = out_dir.join("shim.o");
    run(compiler
        .to_command()
        .arg("-c")
        .arg(&shim)
        .arg("-o")
        .arg(&shim_obj));

    // Dependents before dependencies, mirroring each project's real link
    // graph: approxmc -> arjun -> cryptominisat5 -> sbva/cadiback/cadical,
    // cadiback -> cadical. Order doesn't actually matter for the `-r`
    // partial link below (nothing is pulled in lazily; everything is
    // whole-archived), but it's kept for readability/stability.
    const LINK_ORDER: &[&str] = &[
        "approxmc",
        "arjun",
        "cryptominisat5",
        "sbva",
        "cadiback",
        "cadical",
    ];
    let mut archives: Vec<PathBuf> = Vec::new();
    for name in LINK_ORDER {
        if let Some(path) = libs.remove(*name) {
            archives.push(path);
        }
    }
    // Anything unexpected (future ApproxMC version adding a new dependency)
    // still gets linked, just without an order guarantee.
    archives.extend(libs.into_values());
    archives.push(shim_obj);

    // Merge the whole vendored stack into a single relocatable object, then
    // (below) strip every symbol except the shim's public C API down to
    // local linkage. This keeps everything fully static (no rpath/dylib,
    // matching the original design) while making it impossible for internal
    // symbols (e.g. `CaDiCaL::Solver::...`) to collide with any OTHER crate
    // in the final binary that vendors its own separate copy of the same
    // C++ code (e.g. `rustsat-cadical`, which bundles its own CaDiCaL
    // build) -- duplicate *local* symbols across archive members are legal,
    // only duplicate globals are a link error.
    let combined = out_dir.join("combined.o");
    let mut ld = Command::new("ld");
    ld.arg("-r").arg("--whole-archive");
    ld.args(&archives);
    ld.arg("--no-whole-archive").arg("-o").arg(&combined);
    run(&mut ld);

    let hidden = out_dir.join("combined_hidden.o");
    let mut objcopy = Command::new("objcopy");
    for sym in SHIM_EXPORTS {
        objcopy.arg("--keep-global-symbol").arg(sym);
    }
    objcopy.arg(&combined).arg(&hidden);
    run(&mut objcopy);

    let archive = out_dir.join("libapproxmc_shim.a");
    let _ = std::fs::remove_file(&archive);
    run(Command::new("ar").arg("rcs").arg(&archive).arg(&hidden));

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    // `-bundle` keeps this lazily linked at the consumer's own final link
    // step instead of unconditionally re-embedding it into every downstream
    // binary; harmless either way now that its symbols are all local, but
    // keeps rlibs smaller for consumers that never actually call into it.
    println!("cargo:rustc-link-lib=static:-bundle=approxmc_shim");

    // Remaining dependencies are ordinary system shared libraries that live
    // on the default linker search path, so no rpath is needed for these
    // either. `gmpxx`/`gmp` are required because the shim instantiates
    // ArjunNS::FGenMpq, whose inline C++ code references GMP symbols
    // directly; the others are pulled in transitively by the static libs.
    for lib in ["gmpxx", "gmp", "mpfr", "stdc++", "z", "pthread", "m"] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }

    println!("cargo:rerun-if-changed={}", shim.display());
    println!("cargo:rerun-if-changed={}", header.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("vendor/CMakeLists.txt").display()
    );
}
