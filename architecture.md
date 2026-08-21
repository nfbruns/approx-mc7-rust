Workspace Structure:
├── approxmc-sys/        # Unsafe C/C++ FFI bindings (build.rs / cxx or bindgen)
└── rustsat-mc/          # Idiomatic Rust wrapper integrated with `rustsat`

Dependencies:
- rustsat (v0.5)
- num-bigint (v0.4)
- num-traits (v0.2)
- thiserror (v1.0 / v2.0)
- cxx (v1.0) OR bindgen + cc (build-time)

System Dependencies (Fedora/Linux):
- cryptominisat5-devel (or compiled in-tree)
- approxmc C++ library headers and static/dynamic library

Key Requirements:
1. In-Memory Direct C++ Binding (`approxmc-sys`):
   - Wrap ApproxMC C++ class:
     - `ApproxMC::AppMC` instance initialization.
     - Direct clause ingestion: `add_clause(const std::vector<Lit>& lits)` to avoid DIMACS string round-tripping.
     - Configuration hooks: `set_epsilon(double)`, `set_delta(double)`, `set_projection_set(const std::vector<uint32_t>& vars)`.
     - Direct execution hook: `count()` returning `ApproxMC::SolCount` (`cells`, `hashCount`, `cellSolCount`).

2. High-Level Rust Model Counter (`rustsat-mc`):
   - Implements `ModelCounter` trait:
     - `fn count(&mut self, cnf: &Cnf) -> Result<CountResult, CountingError>`
     - `fn count_projected<I>(&mut self, cnf: &Cnf, projection: I) -> Result<CountResult, CountingError> where I: IntoIterator<Item = Var>`
   - Conversion layer from `rustsat::types::Lit` / `Var` to native CryptoMiniSat/ApproxMC literals.

3. Structured PAC Confidence Output:
   - Calculate lower and upper bounds using (1 / (1 + ε)) * count and (1 + ε) * count.
   - Return exact `BigUint` point estimate along with bound intervals and confidence `1.0 - delta`.

4. Test Suite Requirements:
   - Exact count tests on known small Boolean formulas (all-SAT tautology, pigeonhole UNSAT, single-cube model).
   - Projected count tests comparing full vs. projected variable sets on one-hot chains.
   - PAC bound assertion tests verifying that counts fall within [exact / (1+ε), exact * (1+ε)].