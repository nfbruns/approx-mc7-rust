#include "shim.h"

#include <memory>
#include <vector>

#include <approxmc/approxmc.h>
#include <arjun/arjun.h>
#include <cryptominisat5/cryptominisat.h>

// Wraps AppMC together with the FieldGen it borrows for its lifetime. The
// AppMC constructor takes the FieldGen by const reference, so the generator
// must outlive the counter; keeping both here guarantees that.
struct AppMcHandle {
    std::unique_ptr<CMSat::FieldGen> fg;
    std::unique_ptr<ApproxMC::AppMC> appmc;

    AppMcHandle()
        : fg(std::make_unique<ArjunNS::FGenMpq>()),
          appmc(std::make_unique<ApproxMC::AppMC>(fg)) {}
};

extern "C" {

AppMcHandle* approxmc_shim_new(void) {
    return new AppMcHandle();
}

void approxmc_shim_free(AppMcHandle* handle) {
    delete handle;
}

void approxmc_shim_set_epsilon(AppMcHandle* handle, double epsilon) {
    handle->appmc->set_epsilon(epsilon);
}

void approxmc_shim_set_delta(AppMcHandle* handle, double delta) {
    handle->appmc->set_delta(delta);
}

void approxmc_shim_set_seed(AppMcHandle* handle, uint32_t seed) {
    handle->appmc->set_seed(seed);
}

uint32_t approxmc_shim_nvars(AppMcHandle* handle) {
    return handle->appmc->nVars();
}

void approxmc_shim_new_vars(AppMcHandle* handle, uint32_t n) {
    handle->appmc->new_vars(n);
}

int approxmc_shim_add_clause(AppMcHandle* handle, const int32_t* lits, size_t len) {
    std::vector<CMSat::Lit> clause;
    clause.reserve(len);

    uint32_t max_var = 0;
    bool has_lit = false;
    for (size_t i = 0; i < len; ++i) {
        int32_t l = lits[i];
        uint32_t var = static_cast<uint32_t>(l < 0 ? -l : l) - 1;
        if (!has_lit || var > max_var) {
            max_var = var;
            has_lit = true;
        }
    }
    if (has_lit && max_var >= handle->appmc->nVars()) {
        handle->appmc->new_vars(max_var + 1 - handle->appmc->nVars());
    }

    for (size_t i = 0; i < len; ++i) {
        int32_t l = lits[i];
        uint32_t var = static_cast<uint32_t>(l < 0 ? -l : l) - 1;
        clause.emplace_back(var, l < 0);
    }

    return handle->appmc->add_clause(clause) ? 1 : 0;
}

void approxmc_shim_set_sampling_vars(
    AppMcHandle* handle, const uint32_t* vars, size_t len, int projected) {
    std::vector<uint32_t> sampl(vars, vars + len);
    handle->appmc->set_sampl_vars(sampl);
    handle->appmc->set_projected(projected != 0);
}

AppMcCount approxmc_shim_count(AppMcHandle* handle) {
    ApproxMC::SolCount sc = handle->appmc->count();
    AppMcCount out;
    out.valid = sc.valid ? 1 : 0;
    out.hash_count = sc.hashCount;
    out.cell_sol_count = sc.cellSolCount;
    return out;
}

} // extern "C"
