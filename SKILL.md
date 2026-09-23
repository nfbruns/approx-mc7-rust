---
name: approx-mc7-rust-integration
description: >-
  Integrate ApproxMC-based approximate (PAC) model counting into a Rust project
  using the approx-mc7-rust + approxmc-sys crates. Use when a
  task needs to count (or approximately count) satisfying assignments of a CNF
  / SAT formula, optionally projected onto a subset of variables, with
  Probably-Approximately-Correct (epsilon/delta) guarantees, built on rustsat
  types. Covers native library setup, Cargo wiring, the safe API, and the
  common linking / indexing pitfalls.
license: MIT
---

# Integrating `approx-mc7-rust` (ApproxMC model counting)

This library provides safe, in-memory approximate model counting for
`rustsat` CNF formulas, backed by the C++ ApproxMC solver. Clauses are streamed
directly into the solver — no DIMACS files or pipes.

Two crates:

- `approxmc-sys` — unsafe FFI over a C++ shim wrapping `ApproxMC::AppMC`.
- `approx-mc7-rust` — safe API: `ApproxMcEngine`, `ModelCounter`, `PacBounds`,
  `ApproxMcConfig`, `CountingError`.

## When to use

- Count / approximately count models of a CNF or SAT instance.
- Projected model counting over an independent support (subset of variables).
- Need PAC bounds (point estimate + `(1+ε)` interval at confidence `1-δ`).
- The formula is already expressed with `rustsat` `Cnf` / `Lit` / `Var`.

Do NOT use for: exact #SAT on large instances where exactness is mandatory
(ApproxMC is approximate; it is exact only for small counts), or for solving
(SAT/UNSAT decision) — use a SAT solver for that.

## Step 1 — Native prerequisites (required, non-optional)

ApproxMC and its dependency chain (CryptoMiniSat5, Arjun, SBVA,
cadical/cadiback) are fetched from GitHub and compiled from source
automatically by `approxmc-sys`'s `build.rs` (via a small CMake project, no
manual clone step) the first time you `cargo build`. Everything builds as
static libraries and gets linked directly into your binary — no rpath, no
`LD_LIBRARY_PATH`, no shared libraries to install anywhere. The consuming
machine still needs the toolchain to build it:

- A C++20 compiler, `cmake`, `git`
- Dev headers: GMP (+ C++), MPFR, zlib, Boost

Fedora: `sudo dnf install -y gcc-c++ cmake git gmp-devel gmp-c++ mpfr-devel zlib-devel boost-devel`
Debian/Ubuntu: `sudo apt install -y g++ cmake git libgmp-dev libmpfr-dev zlib1g-dev libboost-dev`

## Step 2 — Build

Nothing to run manually — just build the workspace:

```bash
cargo build
```

The first build compiles ApproxMC and its whole dependency chain from source
(several minutes); it's cached under `target/` afterwards like any other build
script output. To pin a specific ApproxMC ref instead of tracking `master`,
edit `APPROXMC_TAG` in `approxmc-sys/vendor/CMakeLists.txt`.

## Step 3 — Depend on the crate

`approx-mc7-rust` is a path/git crate (not published). In the consumer `Cargo.toml`:

```toml
[dependencies]
approx-mc7-rust = { path = "../approx-mc7-rust/approx-mc7-rust" } # or git = "..."
rustsat = "0.5"
num-bigint = "0.4"
```

Important: the native dependency chain is statically linked into your
binaries/tests, so there is nothing extra to do at runtime — no rpath, no
`LD_LIBRARY_PATH`, no system install of `.so` files. The consumer just needs
the build-time prerequisites from Step 1.

## Core API

```rust
pub struct ApproxMcConfig { pub epsilon: f64, pub delta: f64, pub seed: u32 }
// Default: epsilon 0.2, delta 0.05, seed 1.

pub trait ModelCounter {
    fn count(&mut self, cnf: &Cnf) -> Result<PacBounds, CountingError>;
    fn count_projected(&mut self, cnf: &Cnf, projection: &[Var]) -> Result<PacBounds, CountingError>;
}

pub struct PacBounds {
    pub point_estimate: BigUint, // cell_sol_count * 2^hash_count
    pub lower_bound: BigUint,    // point_estimate / (1 + epsilon)
    pub upper_bound: BigUint,    // point_estimate * (1 + epsilon)
    pub epsilon: f64,
    pub delta: f64,
    pub confidence: f64,         // 1.0 - delta
}
// Display: "1.60 × 10^1 [1.33 × 10^1, 1.92 × 10^1] (confidence 0.9500)"

pub enum CountingError { SolverError(String), Unsatisfiable, InvalidParameter(String) }
```

## Example — full and projected counting

```rust
use rustsat::instances::Cnf;
use rustsat::types::{Lit, Var};
use approx_mc7_rust::{ApproxMcConfig, ApproxMcEngine, ModelCounter};

let mut cnf = Cnf::new();
cnf.add_nary(&[Lit::positive(0), Lit::positive(1)]); // x0 ∨ x1
cnf.add_nary(&[Lit::negative(0), Lit::negative(1)]); // ¬x0 ∨ ¬x1
cnf.add_nary(&[Lit::positive(2), Lit::negative(2)]); // keep x2 as a free var

let mut engine = ApproxMcEngine::new(ApproxMcConfig::default());

let full = engine.count(&cnf)?;                 // point_estimate = 4
let proj = engine.count_projected(&cnf, &[Var::new(0), Var::new(1)])?; // = 2
println!("{full}");                             // 10^-style Display
# Ok::<(), approx_mc7_rust::CountingError>(())
```

## Rules and pitfalls (read before writing code)

1. Indexing is `rustsat`-native and 0-based: `Var::new(0)` is the first
   variable. The bridge handles conversion to the solver's 1-based signed
   literals internally (`Lit::to_ipasir()`, `Var::idx32()`). Do not pre-convert.
2. Validate config via the API, not manually: `epsilon` must be `> 0`, `delta`
   in `(0, 1]`. Bad values return `CountingError::InvalidParameter` (the
   underlying C++ would otherwise `exit(-1)` the process — never bypass the safe
   wrapper).
3. Zero models (UNSAT or empty count) returns `CountingError::Unsatisfiable`,
   not `Ok(0)`.
4. The number of solver variables is derived from the max variable index used
   in the clauses and projection; include a variable (e.g. via a tautological
   clause `[Lit::positive(v), Lit::negative(v)]`) if it must be counted but is
   otherwise unconstrained.
5. Results are approximate. For assertions in tests use the interval
   (`lower_bound <= truth <= upper_bound`), except for small formulas where
   ApproxMC is exact.
6. `ApproxMcEngine::count*` take `&mut self`; reuse one engine across calls.

## Verify

`cargo build` then `cargo test`. If linking fails with an undefined or
duplicate symbol from ApproxMC's dependency chain, see the comments in
`approxmc-sys/build.rs` — it whole-archive-links every static lib CMake
produces, with one known duplicate (`oracle`, embedded in `cryptominisat5`)
excluded already.
