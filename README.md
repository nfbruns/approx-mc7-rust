# approx-mc7-rust

Native, in-memory Rust bindings to the [ApproxMC](https://github.com/meelgroup/approxmc)
approximate model counter, integrated with [`rustsat`](https://crates.io/crates/rustsat).

ApproxMC counts the satisfying assignments of a CNF formula with **PAC**
(Probably Approximately Correct) guarantees: given tolerance `ε` and confidence
`1 − δ`, the returned estimate lies within a factor of `(1 + ε)` of the true
count with probability at least `1 − δ`.

Clauses are streamed **directly** into the C++ solver through a small FFI shim —
there are no DIMACS files, temp files, or pipes involved.

## Workspace layout

| Crate           | Description                                                        |
| --------------- | ------------------------------------------------------------------ |
| `approxmc-sys`  | Unsafe FFI: a C++ shim over `ApproxMC::AppMC` + raw `extern "C"`.   |
| `approx-mc7-rust` | Safe high-level API: `ApproxMcEngine`, `ModelCounter`, `PacBounds`. |

The native solver stack (CryptoMiniSat5, Arjun, SBVA, cadical/cadiback,
ApproxMC) is built from source into `third_party/install/` and linked as shared
libraries.

## Prerequisites

- Rust (stable) and Cargo
- A C++20 compiler (GCC 13+ / Clang 16+), `cmake`, `ninja`, `git`
- Development headers for GMP, MPFR, zlib, and Boost

On Fedora:

```bash
sudo dnf install -y gcc-c++ cmake ninja-build git \
    gmp-devel gmp-c++ mpfr-devel zlib-devel boost-devel
```

On Debian/Ubuntu:

```bash
sudo apt install -y g++ cmake ninja-build git \
    libgmp-dev libmpfr-dev zlib1g-dev libboost-dev
```

## Building the native dependencies (cloning)

The ApproxMC sources are **not vendored** in this repository. A helper script
clones them and builds the whole stack. `third_party/` and the generated
`.cargo/config.toml` are git-ignored.

```bash
./scripts/setup-native.sh
```

This will:

1. `git clone --depth 1 https://github.com/meelgroup/approxmc.git` into
   `third_party/approxmc` (skipped if already present). ApproxMC's CMake then
   fetches CryptoMiniSat, Arjun, SBVA, and cadical/cadiback via `FetchContent`.
2. Configure, build, and install shared libraries into `third_party/install/`.
3. Generate `.cargo/config.toml` with the correct `rpath` so binaries and tests
   locate the shared libraries at runtime without `LD_LIBRARY_PATH`.

Useful options / overrides:

```bash
./scripts/setup-native.sh --clean          # wipe third_party/ and rebuild
APPROXMC_REPO=git@github.com:meelgroup/approxmc.git ./scripts/setup-native.sh  # SSH clone
```

> **Why a script instead of a submodule?** ApproxMC pulls its own dependencies
> through CMake `FetchContent` at configure time, so a single clone is not
> self-contained. The script keeps the whole transitive build reproducible and
> writes the machine-specific `rpath` for you.

Once the native build is in place:

```bash
cargo build
cargo test
```

The install prefix defaults to `third_party/install`; override it with the
`APPROXMC_PREFIX` environment variable to build/link against a different (e.g.
system) install.

## Using from another workspace

`approx-mc7-rust` is a workspace with the library in a nested crate directory, so
depend on that inner directory (not the virtual workspace root):

```toml
[dependencies]
approx-mc7-rust = { path = "../approx-mc7-rust/approx-mc7-rust" }
```

The consumer binary must also locate the native shared libraries at runtime.
Easiest is a one-time system install (then no per-project config is needed):

```bash
sudo cp third_party/install/lib64/lib*.so* /usr/local/lib/ && sudo ldconfig
```

Alternatively, copy this repo's generated `.cargo/config.toml` `rustflags` (the
`--disable-new-dtags` + `-rpath` flags) into the consumer workspace, or run with
`LD_LIBRARY_PATH=<prefix>/lib64`.

## Usage

Add the crate (path or git dependency) and use the safe API:

```rust
use num_bigint::BigUint;
use rustsat::instances::Cnf;
use rustsat::types::{Lit, Var};
use approx_mc7_rust::{ApproxMcConfig, ApproxMcEngine, ModelCounter, PacBounds};

// Build a CNF: exactly-one over x0, x1, plus a free variable x2.
let mut cnf = Cnf::new();
cnf.add_nary(&[Lit::positive(0), Lit::positive(1)]); // x0 ∨ x1
cnf.add_nary(&[Lit::negative(0), Lit::negative(1)]); // ¬x0 ∨ ¬x1
cnf.add_nary(&[Lit::positive(2), Lit::negative(2)]); // tautology keeps x2 alive

let mut engine = ApproxMcEngine::new(ApproxMcConfig {
    epsilon: 0.2,
    delta: 0.05,
    seed: 1,
});

// Full count over all variables: 2 (exactly-one) × 2 (free x2) = 4.
let bounds: PacBounds = engine.count(&cnf).unwrap();
println!("point estimate = {}", bounds.point_estimate);
println!("interval = [{}, {}]", bounds.lower_bound, bounds.upper_bound);
println!("confidence = {}", bounds.confidence); // 1 - delta

assert_eq!(bounds.point_estimate, BigUint::from(4u32));
```

### Configuration (`ApproxMcConfig`)

`ApproxMcConfig` controls the accuracy/speed trade-off of the counter:

| Field     | Type  | Meaning                                                                                     |
| --------- | ----- | ------------------------------------------------------------------------------------------- |
| `epsilon` | `f64` | Multiplicative tolerance `ε`. The interval is `[count/(1+ε), count·(1+ε)]`. Must be `> 0`.   |
| `delta`   | `f64` | Failure probability `δ`. Confidence is `1 − δ`. Must be in `(0, 1]`.                         |
| `seed`    | `u32` | RNG seed; fixing it makes a run reproducible.                                                |

Smaller `epsilon` (tighter interval) and smaller `delta` (higher confidence)
both make counting **slower**. `ApproxMcConfig::default()` uses
`epsilon = 0.2`, `delta = 0.05`, `seed = 1` — roughly a ±20% interval at 95%
confidence.

```rust
use approx_mc7_rust::{ApproxMcConfig, ApproxMcEngine};

// Defaults: epsilon 0.2, delta 0.05, seed 1.
let mut engine = ApproxMcEngine::default();

// Tighter and more confident (slower): ±5% interval at 99% confidence.
let mut precise = ApproxMcEngine::new(ApproxMcConfig {
    epsilon: 0.05,
    delta: 0.01,
    seed: 42,
});

// Looser and faster: wide interval, 80% confidence.
let mut fast = ApproxMcEngine::new(ApproxMcConfig {
    epsilon: 0.8,
    delta: 0.2,
    seed: 1,
});

// Override just one field, keep the rest at their defaults.
let mut reproducible = ApproxMcEngine::new(ApproxMcConfig {
    seed: 7,
    ..Default::default()
});
```

Invalid values (`epsilon <= 0` or `delta` outside `(0, 1]`) are rejected up
front with `CountingError::InvalidParameter` — see [Error handling](#error-handling).

### Projected counting

Count models projected onto a subset of variables (the independent support):

```rust
// Only x0, x1 matter → 2 models regardless of x2.
let projected = engine
    .count_projected(&cnf, &[Var::new(0), Var::new(1)])
    .unwrap();
assert_eq!(projected.point_estimate, BigUint::from(2u32));
```

### Interpreting the result

`PacBounds` bundles the estimate with its guarantee:

```rust
pub struct PacBounds {
    pub point_estimate: BigUint, // cell_sol_count * 2^hash_count
    pub lower_bound: BigUint,    // point_estimate / (1 + epsilon)
    pub upper_bound: BigUint,    // point_estimate * (1 + epsilon)
    pub epsilon: f64,
    pub delta: f64,
    pub confidence: f64,         // 1.0 - delta
}
```

### Error handling

```rust
use approx_mc7_rust::CountingError;

match engine.count(&cnf) {
    Ok(bounds) => println!("{}", bounds.point_estimate),
    Err(CountingError::Unsatisfiable) => println!("0 models"),
    Err(CountingError::InvalidParameter(msg)) => eprintln!("bad config: {msg}"),
    Err(CountingError::SolverError(msg)) => eprintln!("solver error: {msg}"),
}
```

`epsilon` must be `> 0` and `delta` must be in `(0, 1]`; invalid values are
rejected up front with `CountingError::InvalidParameter` rather than reaching
the solver.

## Variable indexing

`rustsat` variables are 0-based (`Var::new(0)` is the first variable). The
bridge converts literals to the solver's 1-based signed form via
`Lit::to_ipasir()` and projection variables via `Var::idx32()`, so you always
work in `rustsat`'s indexing.

## Integrating from another repo

[`SKILL.md`](SKILL.md) is a self-contained integration guide (skill format with
YAML frontmatter) that a coding agent in another repository can read or copy to
wire this library into a Rust project — it covers native setup, Cargo wiring,
the API, and the linking/indexing pitfalls.

## Troubleshooting

- **`error while loading shared libraries: libapproxmc.so...`** — the rpath is
  missing or stale. Re-run `./scripts/setup-native.sh` to regenerate
  `.cargo/config.toml`, or move the repo and re-run it (the rpath is an absolute
  path into `third_party/install`).
- **`libcryptominisat5.so... cannot open shared object file`** — the transitive
  rpath tag is missing; the generated config uses `-Wl,--disable-new-dtags` to
  make the rpath apply to indirectly-loaded libraries. Regenerate it with the
  script.
- **CMake cannot find `gmp` / `mpfr`** — install the `-devel`/`-dev` packages
  listed under [Prerequisites](#prerequisites).

## License

MIT. Note that the bundled native dependencies (ApproxMC, CryptoMiniSat, Arjun,
…) carry their own licenses.
