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
cadical/cadiback) are compiled from source. The consuming machine needs:

- A C++20 compiler, `cmake`, `ninja`, `git`
- Dev headers: GMP (+ C++), MPFR, zlib, Boost

Fedora: `sudo dnf install -y gcc-c++ cmake ninja-build git gmp-devel gmp-c++ mpfr-devel zlib-devel boost-devel`
Debian/Ubuntu: `sudo apt install -y g++ cmake ninja-build git libgmp-dev libmpfr-dev zlib1g-dev libboost-dev`

## Step 2 — Build the native stack

From the `approx-mc7-rust` checkout, run its setup script once:

```bash
./scripts/setup-native.sh
```

It clones `meelgroup/approxmc`, builds shared libs into `third_party/install/`,
and generates `.cargo/config.toml` with the required `rpath`. Re-run after
moving the checkout (the rpath is an absolute path).

## Step 3 — Depend on the crate

`approx-mc7-rust` is a path/git crate (not published). In the consumer `Cargo.toml`:

```toml
[dependencies]
approx-mc7-rust = { path = "../approx-mc7-rust/approx-mc7-rust" } # or git = "..."
rustsat = "0.5"
num-bigint = "0.4"
```

Important: the consumer's own binaries/tests must also carry the `rpath` to
`third_party/install/lib64`. Either build inside the `approx-mc7-rust`
workspace, or copy the generated `.cargo/config.toml` `rustflags` (both the
`-Wl,--disable-new-dtags` and `-Wl,-rpath,<abs>/third_party/install/lib64`
flags) into the consumer workspace's `.cargo/config.toml`. Without
`--disable-new-dtags`, transitively-loaded libraries (e.g.
`libcryptominisat5.so`) fail to load at runtime.

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

`cargo build` then `cargo test`. A runtime error
`error while loading shared libraries: libapproxmc.so...` means the `rpath` is
missing — re-run `./scripts/setup-native.sh` and ensure the consumer inherits
the generated `.cargo/config.toml` rustflags.
