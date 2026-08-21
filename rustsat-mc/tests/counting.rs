use num_bigint::BigUint;
use rustsat::instances::Cnf;
use rustsat::types::{Lit, Var};
use rustsat_mc::{ApproxMcConfig, ApproxMcEngine, CountingError, ModelCounter};

fn engine() -> ApproxMcEngine {
    ApproxMcEngine::new(ApproxMcConfig::default())
}

fn p(idx: u32) -> Lit {
    Lit::positive(idx)
}

fn n(idx: u32) -> Lit {
    Lit::negative(idx)
}

/// Three variables constrained only by tautological clauses: all 2^3 = 8
/// assignments satisfy the formula.
#[test]
fn exact_count_all_sat_tautology() {
    let mut cnf = Cnf::new();
    for v in 0..3 {
        cnf.add_nary(&[p(v), n(v)]);
    }

    let bounds = engine().count(&cnf).expect("count");
    assert_eq!(bounds.point_estimate, BigUint::from(8u32));
}

/// Unit clauses that fix every variable leave exactly one model.
#[test]
fn exact_count_single_cube() {
    let mut cnf = Cnf::new();
    cnf.add_nary(&[p(0)]);
    cnf.add_nary(&[n(1)]);
    cnf.add_nary(&[p(2)]);

    let bounds = engine().count(&cnf).expect("count");
    assert_eq!(bounds.point_estimate, BigUint::from(1u32));
}

/// A trivially contradictory formula is reported as unsatisfiable.
#[test]
fn unsat_formula_reports_unsatisfiable() {
    let mut cnf = Cnf::new();
    cnf.add_nary(&[p(0)]);
    cnf.add_nary(&[n(0)]);

    match engine().count(&cnf) {
        Err(CountingError::Unsatisfiable) => {}
        other => panic!("expected Unsatisfiable, got {other:?}"),
    }
}

/// Projected counting over an exactly-one constraint with an extra free
/// variable: the full count doubles for the free variable, the projected
/// count does not.
#[test]
fn projected_vs_full_count() {
    let mut cnf = Cnf::new();
    // Exactly-one over x0, x1.
    cnf.add_nary(&[p(0), p(1)]);
    cnf.add_nary(&[n(0), n(1)]);
    // Free variable x2 kept alive by a tautology.
    cnf.add_nary(&[p(2), n(2)]);

    let mut eng = engine();

    let full = eng.count(&cnf).expect("full count");
    assert_eq!(full.point_estimate, BigUint::from(4u32));

    let projected = eng
        .count_projected(&cnf, &[Var::new(0), Var::new(1)])
        .expect("projected count");
    assert_eq!(projected.point_estimate, BigUint::from(2u32));
}

/// The exact count must fall inside the reported PAC interval, and the point
/// estimate must lie between the bounds.
#[test]
fn pac_bounds_contain_exact() {
    let mut cnf = Cnf::new();
    for v in 0..4 {
        cnf.add_nary(&[p(v), n(v)]);
    }
    let exact = BigUint::from(16u32);

    let bounds = engine().count(&cnf).expect("count");

    assert!(bounds.lower_bound <= bounds.point_estimate);
    assert!(bounds.point_estimate <= bounds.upper_bound);
    assert!(bounds.lower_bound <= exact);
    assert!(exact <= bounds.upper_bound);
    assert_eq!(bounds.confidence, 1.0 - bounds.delta);
}

/// Invalid parameters are rejected before touching the solver.
#[test]
fn invalid_parameters_rejected() {
    let mut cnf = Cnf::new();
    cnf.add_nary(&[p(0), n(0)]);

    let mut eng = ApproxMcEngine::new(ApproxMcConfig {
        epsilon: 0.0,
        delta: 0.05,
        seed: 1,
    });
    assert!(matches!(
        eng.count(&cnf),
        Err(CountingError::InvalidParameter(_))
    ));

    let mut eng = ApproxMcEngine::new(ApproxMcConfig {
        epsilon: 0.2,
        delta: 1.5,
        seed: 1,
    });
    assert!(matches!(
        eng.count(&cnf),
        Err(CountingError::InvalidParameter(_))
    ));
}

/// The `Display` impl renders values in `m.mm × 10^e` notation.
#[test]
fn display_uses_scientific_notation() {
    let mut cnf = Cnf::new();
    for v in 0..4 {
        cnf.add_nary(&[p(v), n(v)]);
    }

    let bounds = engine().count(&cnf).expect("count");
    let shown = bounds.to_string();

    assert!(shown.contains("× 10^"), "unexpected format: {shown}");
    assert!(shown.contains('['), "missing bounds: {shown}");
    // 16 models -> 1.60 × 10^1.
    assert!(
        shown.starts_with("1.60 × 10^1"),
        "unexpected value: {shown}"
    );
}
