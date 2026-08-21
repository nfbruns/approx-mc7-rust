//! High-level, safe PAC model counting for [`rustsat`] CNF formulas backed by
//! ApproxMC.
//!
//! Clauses are streamed directly into the native solver via [`approxmc_sys`];
//! no DIMACS files or pipes are involved.

use std::fmt;

use num_bigint::BigUint;
use rustsat::instances::Cnf;
use rustsat::types::{Lit, Var};
use thiserror::Error;

/// Errors that can occur while counting.
#[derive(Debug, Error)]
pub enum CountingError {
    #[error("Internal ApproxMC solver error: {0}")]
    SolverError(String),
    #[error("Formula is unsatisfiable (0 models)")]
    Unsatisfiable,
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
}

/// A point estimate together with its PAC confidence interval.
#[derive(Debug, Clone, PartialEq)]
pub struct PacBounds {
    pub point_estimate: BigUint,
    pub lower_bound: BigUint,
    pub upper_bound: BigUint,
    pub epsilon: f64,
    pub delta: f64,
    /// Confidence that the true count lies in `[lower_bound, upper_bound]`.
    pub confidence: f64,
}

impl fmt::Display for PacBounds {
    /// Renders the estimate and its interval in `m.mm × 10^e` notation, e.g.
    /// `2.50 × 10^3 [2.08 × 10^3, 3.00 × 10^3] (confidence 0.9500)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}, {}] (confidence {:.4})",
            sci(&self.point_estimate),
            sci(&self.lower_bound),
            sci(&self.upper_bound),
            self.confidence
        )
    }
}

/// Formats a `BigUint` as `m.mm × 10^e` with three significant digits.
fn sci(n: &BigUint) -> String {
    let digits = n.to_string();
    let exp = digits.len() - 1;
    let sig = &digits[..digits.len().min(3)];
    let mantissa = sig.parse::<f64>().unwrap_or(0.0) / 10f64.powi((sig.len() - 1) as i32);
    format!("{mantissa:.2} \u{d7} 10^{exp}")
}

/// Trait for approximate model counters.
pub trait ModelCounter {
    /// Counts all satisfying assignments of `cnf`.
    fn count(&mut self, cnf: &Cnf) -> Result<PacBounds, CountingError>;

    /// Counts satisfying assignments projected onto `projection`.
    fn count_projected(
        &mut self,
        cnf: &Cnf,
        projection: &[Var],
    ) -> Result<PacBounds, CountingError>;
}

/// Configuration for [`ApproxMcEngine`].
#[derive(Debug, Clone)]
pub struct ApproxMcConfig {
    pub epsilon: f64,
    pub delta: f64,
    pub seed: u32,
}

impl Default for ApproxMcConfig {
    fn default() -> Self {
        Self {
            epsilon: 0.2,
            delta: 0.05,
            seed: 1,
        }
    }
}

/// An ApproxMC-backed model counter.
pub struct ApproxMcEngine {
    config: ApproxMcConfig,
}

impl ApproxMcEngine {
    /// Creates an engine with the given configuration.
    pub fn new(config: ApproxMcConfig) -> Self {
        Self { config }
    }

    /// Returns the current configuration.
    pub fn config(&self) -> &ApproxMcConfig {
        &self.config
    }

    fn validate(&self) -> Result<(), CountingError> {
        if !(self.config.epsilon > 0.0) {
            return Err(CountingError::InvalidParameter(format!(
                "epsilon must be > 0, got {}",
                self.config.epsilon
            )));
        }
        if !(self.config.delta > 0.0 && self.config.delta <= 1.0) {
            return Err(CountingError::InvalidParameter(format!(
                "delta must be in (0, 1], got {}",
                self.config.delta
            )));
        }
        Ok(())
    }

    fn run(
        &self,
        cnf: &Cnf,
        projection: Option<&[Var]>,
    ) -> Result<PacBounds, CountingError> {
        self.validate()?;

        // Determine how many variables the solver must allocate.
        let mut max_idx: Option<u32> = None;
        for clause in cnf.iter() {
            for lit in clause.iter() {
                let idx = lit.vidx32();
                max_idx = Some(max_idx.map_or(idx, |m| m.max(idx)));
            }
        }
        if let Some(proj) = projection {
            for var in proj {
                let idx = var.idx32();
                max_idx = Some(max_idx.map_or(idx, |m| m.max(idx)));
            }
        }
        let n_vars = max_idx.map_or(0, |m| m + 1);

        // Safety: the handle is created here and freed on all paths via the
        // guard's Drop impl. All pointers passed to the FFI are valid slices.
        let handle = Handle::new()?;

        unsafe {
            approxmc_sys::approxmc_shim_set_epsilon(handle.ptr, self.config.epsilon);
            approxmc_sys::approxmc_shim_set_delta(handle.ptr, self.config.delta);
            approxmc_sys::approxmc_shim_set_seed(handle.ptr, self.config.seed);

            if n_vars > 0 {
                approxmc_sys::approxmc_shim_new_vars(handle.ptr, n_vars);
            }

            let mut buf: Vec<i32> = Vec::new();
            for clause in cnf.iter() {
                buf.clear();
                buf.extend(clause.iter().map(lit_to_ipasir));
                approxmc_sys::approxmc_shim_add_clause(
                    handle.ptr,
                    buf.as_ptr(),
                    buf.len(),
                );
            }

            let (sampl, projected): (Vec<u32>, bool) = match projection {
                Some(proj) => (proj.iter().map(|v| v.idx32()).collect(), true),
                None => ((0..n_vars).collect(), false),
            };
            approxmc_sys::approxmc_shim_set_sampling_vars(
                handle.ptr,
                sampl.as_ptr(),
                sampl.len(),
                projected as i32,
            );

            let count = approxmc_sys::approxmc_shim_count(handle.ptr);
            if count.valid == 0 || count.cell_sol_count == 0 {
                return Err(CountingError::Unsatisfiable);
            }

            let point = BigUint::from(count.cell_sol_count) << count.hash_count;
            Ok(self.bounds(point))
        }
    }

    fn bounds(&self, point_estimate: BigUint) -> PacBounds {
        // Work in fixed-point to keep the interval computation on BigUint.
        const SCALE: u64 = 1_000_000;
        let factor = ((1.0 + self.config.epsilon) * SCALE as f64).round() as u64;
        let scale = BigUint::from(SCALE);
        let factor = BigUint::from(factor);

        let upper_bound = (&point_estimate * &factor) / &scale;
        let lower_bound = (&point_estimate * &scale) / &factor;

        PacBounds {
            point_estimate,
            lower_bound,
            upper_bound,
            epsilon: self.config.epsilon,
            delta: self.config.delta,
            confidence: 1.0 - self.config.delta,
        }
    }
}

impl Default for ApproxMcEngine {
    fn default() -> Self {
        Self::new(ApproxMcConfig::default())
    }
}

impl ModelCounter for ApproxMcEngine {
    fn count(&mut self, cnf: &Cnf) -> Result<PacBounds, CountingError> {
        self.run(cnf, None)
    }

    fn count_projected(
        &mut self,
        cnf: &Cnf,
        projection: &[Var],
    ) -> Result<PacBounds, CountingError> {
        self.run(cnf, Some(projection))
    }
}

/// Converts a rustsat literal to a 1-based signed (DIMACS/IPASIR) integer.
fn lit_to_ipasir(lit: &Lit) -> i32 {
    lit.to_ipasir()
}

/// RAII guard that frees the native handle on drop.
struct Handle {
    ptr: *mut approxmc_sys::AppMcHandle,
}

impl Handle {
    fn new() -> Result<Self, CountingError> {
        let ptr = unsafe { approxmc_sys::approxmc_shim_new() };
        if ptr.is_null() {
            return Err(CountingError::SolverError(
                "failed to allocate ApproxMC instance".into(),
            ));
        }
        Ok(Self { ptr })
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { approxmc_sys::approxmc_shim_free(self.ptr) }
    }
}
