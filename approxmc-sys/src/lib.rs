//! Low-level FFI bindings to the ApproxMC approximate model counter.
//!
//! This crate exposes a thin, `unsafe` C ABI over a hand-written C++ shim
//! (`shim/shim.cpp`) that wraps `ApproxMC::AppMC`. Prefer the safe
//! `approx-mc7-rust` crate for application code.

use std::os::raw::c_int;

/// Opaque handle to an `ApproxMC::AppMC` instance.
#[repr(C)]
pub struct AppMcHandle {
    _private: [u8; 0],
}

/// Raw counting result: the point estimate is `cell_sol_count * 2^hash_count`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AppMcCount {
    pub valid: c_int,
    pub hash_count: u32,
    pub cell_sol_count: u64,
}

extern "C" {
    pub fn approxmc_shim_new() -> *mut AppMcHandle;
    pub fn approxmc_shim_free(handle: *mut AppMcHandle);

    pub fn approxmc_shim_set_epsilon(handle: *mut AppMcHandle, epsilon: f64);
    pub fn approxmc_shim_set_delta(handle: *mut AppMcHandle, delta: f64);
    pub fn approxmc_shim_set_seed(handle: *mut AppMcHandle, seed: u32);

    pub fn approxmc_shim_nvars(handle: *mut AppMcHandle) -> u32;
    pub fn approxmc_shim_new_vars(handle: *mut AppMcHandle, n: u32);

    pub fn approxmc_shim_add_clause(
        handle: *mut AppMcHandle,
        lits: *const i32,
        len: usize,
    ) -> c_int;

    pub fn approxmc_shim_set_sampling_vars(
        handle: *mut AppMcHandle,
        vars: *const u32,
        len: usize,
        projected: c_int,
    );

    pub fn approxmc_shim_count(handle: *mut AppMcHandle) -> AppMcCount;
}
