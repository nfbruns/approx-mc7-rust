#ifndef APPROXMC_SHIM_H
#define APPROXMC_SHIM_H

#include <cstddef>
#include <cstdint>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handle wrapping an ApproxMC::AppMC instance plus its FieldGen.
typedef struct AppMcHandle AppMcHandle;

// Result of a counting run. `point_*` encode the point estimate as
// cell_sol_count * 2^hash_count; the Rust side assembles the BigUint.
typedef struct AppMcCount {
    int valid;              // 1 if the count is valid, 0 otherwise
    uint32_t hash_count;    // number of XOR hashes
    uint64_t cell_sol_count;// solutions found in the final cell
} AppMcCount;

AppMcHandle* approxmc_shim_new(void);
void approxmc_shim_free(AppMcHandle* handle);

void approxmc_shim_set_epsilon(AppMcHandle* handle, double epsilon);
void approxmc_shim_set_delta(AppMcHandle* handle, double delta);
void approxmc_shim_set_seed(AppMcHandle* handle, uint32_t seed);

uint32_t approxmc_shim_nvars(AppMcHandle* handle);
void approxmc_shim_new_vars(AppMcHandle* handle, uint32_t n);

// Adds a clause from a slice of 1-based signed literals (negative => negated).
// Returns 1 on success, 0 if the solver reports the formula is now UNSAT.
int approxmc_shim_add_clause(AppMcHandle* handle, const int32_t* lits, size_t len);

// Declares the sampling set from 0-based variable indices. Must be called
// before `approxmc_shim_count`. `projected` toggles projected counting.
void approxmc_shim_set_sampling_vars(
    AppMcHandle* handle, const uint32_t* vars, size_t len, int projected);

AppMcCount approxmc_shim_count(AppMcHandle* handle);

#ifdef __cplusplus
}
#endif

#endif // APPROXMC_SHIM_H
