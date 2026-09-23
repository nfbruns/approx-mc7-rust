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

`cargo build` fetches and builds the whole native solver stack itself: no
manual clone, no setup script, no rpath. `approxmc-sys`'s `build.rs` drives a
small CMake project that pulls ApproxMC straight from GitHub (which then pulls
CryptoMiniSat5, Arjun, SBVA, and cadical/cadiback via its own `FetchContent`)
and builds the whole chain as static (`.a`, `-fPIC`) libraries. Those archives
get linked directly into the Rust binary/test artifacts, so there is no `.so`
to find at runtime and therefore nothing that needs an `rpath` or
`LD_LIBRARY_PATH`. The only remaining dynamic dependencies (`libgmp`,
`libgmpxx`, `libstdc++`, ...) are ordinary system libraries on the default
linker search path.

## Prerequisites

- Rust (stable) and Cargo
- A C++20 compiler (GCC 13+ / Clang 16+), `cmake`, `git`
- Development headers for GMP, MPFR, zlib, and Boost

On Fedora:

```bash
sudo dnf install -y gcc-c++ cmake git \
    gmp-devel gmp-c++ mpfr-devel zlib-devel boost-devel
```

On Debian/Ubuntu:

```bash
sudo apt install -y g++ cmake git \
    libgmp-dev libmpfr-dev zlib1g-dev libboost-dev
```

## Building

```bash
cargo build
cargo test
```

The first build compiles ApproxMC and its whole dependency chain from source
(several minutes); the result is cached under `target/` like any other build
script output, so subsequent builds are incremental. To build against a
different ApproxMC ref, edit `APPROXMC_TAG`/`APPROXMC_REPO` in
[`approxmc-sys/vendor/CMakeLists.txt`](approxmc-sys/vendor/CMakeLists.txt)
(defaults to the `master` branch of `meelgroup/approxmc`).

## Using from another workspace

`approx-mc7-rust` is a workspace with the library in a nested crate directory, so
depend on that inner directory (not the virtual workspace root):

```toml
[dependencies]
approx-mc7-rust = { path = "../approx-mc7-rust/approx-mc7-rust" }
```

The native dependency chain is statically linked, so the consumer's binaries
need nothing extra at runtime (no rpath, no `LD_LIBRARY_PATH`, no system
install step) beyond the same system dev packages listed under
[Prerequisites](#prerequisites), which `cargo build` needs to rebuild it.

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

- **CMake cannot find `gmp` / `mpfr`** — install the `-devel`/`-dev` packages
  listed under [Prerequisites](#prerequisites).
- **Link errors about a missing/duplicate symbol from a new ApproxMC
  version** — a transitive static lib either isn't installed or duplicates
  another one's objects; see the comments in
  [`approxmc-sys/build.rs`](approxmc-sys/build.rs) (the `oracle`/`cryptominisat5`
  duplicate is handled there already).

## License

MIT. Note that the bundled native dependencies (ApproxMC, CryptoMiniSat, Arjun,
…) carry their own licenses.
