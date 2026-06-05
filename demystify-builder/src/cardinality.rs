//! CNF encoding of `sum(lits) ≥ k` / `sum(lits) ≤ k`, optionally
//! guarded by a *conjunction* of literals — i.e. the encoded constraint
//! reads `(g₁ ∧ … ∧ gₙ) -> (sum … k)`.
//!
//! Uses `rustsat::encodings::card::Totalizer` for `n ≥ 2`.  Small cases
//! are hand-rolled so the resulting CNF is human-readable when reasoned
//! about (and to avoid pulling the totalizer machinery for trivial 1- and
//! 2-literal constraints).
//!
//! The gates are applied by emitting every clause `(c₁ ∨ … ∨ cₖ)` as
//! `(¬g₁ ∨ … ∨ ¬gₙ ∨ c₁ ∨ … ∨ cₖ)`, which is exactly
//! `(g₁ ∧ … ∧ gₙ) -> (c₁ ∨ … ∨ cₖ)`.  An empty `gates` slice means
//! the constraint is unconditional.  A single-element slice recovers
//! the previous "guarded by one literal" behaviour.

use rustsat::encodings::card::{BoundLower, BoundUpper, Totalizer};
use rustsat::instances::{Cnf, SatInstance};
use rustsat::types::{Clause, Lit};

/// Encode `sum(lits) ≥ k`.  If `gates` is non-empty, the encoded
/// constraint is `(g₁ ∧ … ∧ gₙ) -> (sum ≥ k)`; otherwise it is
/// unconditional.
///
/// Returns an error only if the encoder runs out of memory.
pub(crate) fn encode_sum_ge(
    sat: &mut SatInstance,
    lits: &[Lit],
    k: i64,
    gates: &[Lit],
) -> Result<(), rustsat::OutOfMemory> {
    let n = lits.len();
    // Normalise k against the bounds of the sum.
    if k <= 0 {
        // Trivially true.
        return Ok(());
    }
    if (k as usize) > n {
        // Unsatisfiable.  Under gates, emit the disjunction of negated
        // gates (i.e. assert at least one gate is false); with no gates,
        // emit an empty clause.
        emit_under_gates(sat, &[], gates);
        return Ok(());
    }
    let k = k as usize;

    // Quick paths for tiny constraints.  Avoids spinning up a totalizer for
    // single-cell row/col sums (common in star battle) and for the 2-cell
    // adjacency constraint.
    if k == n {
        // All literals must be true: one unit per literal.
        for &l in lits {
            emit_under_gates(sat, &[l], gates);
        }
        return Ok(());
    }
    if k == 1 {
        // At least one literal must be true: a single clause.
        emit_under_gates(sat, lits, gates);
        return Ok(());
    }

    encode_via_totalizer(sat, lits, k, gates, Bound::Lower)
}

/// Encode `sum(lits) = k`.  Same gate semantics as [`encode_sum_ge`].
///
/// Implemented as `encode_sum_ge(..., k)` plus `encode_sum_le(..., k)`,
/// short-circuiting the two cases (`k < 0`, `k > n`) where both halves
/// would individually emit an unsat clause — we only want one.
pub(crate) fn encode_sum_eq(
    sat: &mut SatInstance,
    lits: &[Lit],
    k: i64,
    gates: &[Lit],
) -> Result<(), rustsat::OutOfMemory> {
    let n = lits.len();
    if k < 0 || (k as usize) > n {
        emit_under_gates(sat, &[], gates);
        return Ok(());
    }
    encode_sum_ge(sat, lits, k, gates)?;
    encode_sum_le(sat, lits, k, gates)?;
    Ok(())
}

/// Encode `sum(lits) ≤ k`.  Same gate semantics as [`encode_sum_ge`].
pub(crate) fn encode_sum_le(
    sat: &mut SatInstance,
    lits: &[Lit],
    k: i64,
    gates: &[Lit],
) -> Result<(), rustsat::OutOfMemory> {
    let n = lits.len();
    if k < 0 {
        // Unsatisfiable.
        emit_under_gates(sat, &[], gates);
        return Ok(());
    }
    if (k as usize) >= n {
        // Trivially true: sum is at most n anyway.
        return Ok(());
    }
    let k = k as usize;

    // Quick paths.
    if k == 0 {
        // All literals must be false: one unit clause per literal.
        for &l in lits {
            emit_under_gates(sat, &[!l], gates);
        }
        return Ok(());
    }
    if k + 1 == n {
        // At most n-1 true ⟺ at least one false ⟺ disjunction of negations.
        let neg: Vec<Lit> = lits.iter().map(|&l| !l).collect();
        emit_under_gates(sat, &neg, gates);
        return Ok(());
    }

    encode_via_totalizer(sat, lits, k, gates, Bound::Upper)
}

#[derive(Copy, Clone)]
enum Bound {
    Upper,
    Lower,
}

fn encode_via_totalizer(
    sat: &mut SatInstance,
    lits: &[Lit],
    k: usize,
    gates: &[Lit],
    bound: Bound,
) -> Result<(), rustsat::OutOfMemory> {
    let mut scratch = Cnf::new();
    let mut enc = Totalizer::from_iter(lits.iter().copied());
    let var_manager = sat.var_manager_mut();
    let units: Vec<Lit> = match bound {
        Bound::Lower => {
            enc.encode_lb(k..=k, &mut scratch, var_manager)?;
            // The callers guarantee 0 < k < n and we just encoded `k..=k`, so
            // the bound is both encoded and satisfiable — `enforce_lb` cannot
            // return `NotEncoded` or `Unsat`. Crash loudly if that invariant
            // is ever broken rather than silently dropping the enforcing
            // literals (which would leave the cardinality bound unenforced).
            enc.enforce_lb(k)
                .expect("enforce_lb on a just-encoded, in-range (0<k<n) bound")
        }
        Bound::Upper => {
            enc.encode_ub(k..=k, &mut scratch, var_manager)?;
            enc.enforce_ub(k)
                .expect("enforce_ub on a just-encoded, in-range (0<k<n) bound")
        }
    };

    // Move scratch clauses into the SatInstance under the gates.
    for clause in scratch.iter() {
        let body: Vec<Lit> = clause.iter().copied().collect();
        emit_under_gates(sat, &body, gates);
    }
    for unit in units {
        emit_under_gates(sat, &[unit], gates);
    }
    Ok(())
}

/// Emit `(¬g₁ ∨ … ∨ ¬gₙ ∨ body...)` into `sat`.  With empty `gates`,
/// emits the bare `body` clause; with a single gate, recovers the
/// previous `(¬g ∨ body...)` shape.  An empty `body` with no gates
/// produces an empty (unsat) clause; an empty `body` *with* gates
/// produces `(¬g₁ ∨ … ∨ ¬gₙ)` (i.e. asserts at least one gate is
/// false), which is the right CNF for "this conjunction can never be
/// fully active".
/// Forbid one tuple of a table constraint: emit
/// `(g₁ ∧ … ∧ gₙ) -> ¬(l₁ ∧ … ∧ lₘ)`, i.e. the clause
/// `(¬g₁ ∨ … ∨ ¬gₙ ∨ ¬l₁ ∨ … ∨ ¬lₘ)`.  `tuple_lits` are the literals
/// (typically one-hot value-lits) whose simultaneous truth is disallowed.
/// An empty `tuple_lits` with no gates yields the empty (unsat) clause; with
/// gates it asserts the gates can't all hold.
pub(crate) fn encode_forbidden_tuple(sat: &mut SatInstance, tuple_lits: &[Lit], gates: &[Lit]) {
    let body: Vec<Lit> = tuple_lits.iter().map(|&l| !l).collect();
    emit_under_gates(sat, &body, gates);
}

fn emit_under_gates(sat: &mut SatInstance, body: &[Lit], gates: &[Lit]) {
    let mut clause = Clause::with_capacity(body.len() + gates.len());
    for &g in gates {
        clause.add(!g);
    }
    for &l in body {
        clause.add(l);
    }
    sat.add_clause(clause);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustsat::instances::SatInstance;

    fn fresh_lits(sat: &mut SatInstance, n: usize) -> Vec<Lit> {
        (0..n).map(|_| sat.new_lit()).collect()
    }

    fn clauses(sat: &SatInstance) -> Vec<Vec<i32>> {
        sat.cnf()
            .iter()
            .map(|c| c.iter().map(|l| l.to_ipasir()).collect())
            .collect()
    }

    #[test]
    fn sum_ge_zero_is_noop() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_ge(&mut sat, &lits, 0, &[]).unwrap();
        encode_sum_ge(&mut sat, &lits, -5, &[]).unwrap();
        assert!(clauses(&sat).is_empty());
    }

    #[test]
    fn sum_ge_too_big_is_unsat() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_ge(&mut sat, &lits, 4, &[]).unwrap();
        assert_eq!(clauses(&sat), vec![Vec::<i32>::new()]);
    }

    #[test]
    fn sum_ge_too_big_guarded_emits_neg_guard_unit() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        let g = sat.new_lit();
        encode_sum_ge(&mut sat, &lits, 4, &[g]).unwrap();
        assert_eq!(clauses(&sat), vec![vec![(!g).to_ipasir()]]);
    }

    #[test]
    fn sum_ge_too_big_two_gates_emits_neg_disjunction() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        let g1 = sat.new_lit();
        let g2 = sat.new_lit();
        encode_sum_ge(&mut sat, &lits, 4, &[g1, g2]).unwrap();
        // With two gates, "unsat under the gates' conjunction" means
        // "at least one gate is false": (¬g₁ ∨ ¬g₂).
        let c = clauses(&sat);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].len(), 2);
        assert!(c[0].contains(&(!g1).to_ipasir()));
        assert!(c[0].contains(&(!g2).to_ipasir()));
    }

    #[test]
    fn sum_ge_one_is_disjunction() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_ge(&mut sat, &lits, 1, &[]).unwrap();
        let c = clauses(&sat);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].len(), 3);
    }

    #[test]
    fn sum_ge_one_guarded_prepends_neg_guard() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        let g = sat.new_lit();
        encode_sum_ge(&mut sat, &lits, 1, &[g]).unwrap();
        let c = clauses(&sat);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].len(), 4);
        assert!(c[0].contains(&(!g).to_ipasir()));
    }

    #[test]
    fn sum_ge_one_two_gates_prepends_both_neg_gates() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        let g1 = sat.new_lit();
        let g2 = sat.new_lit();
        encode_sum_ge(&mut sat, &lits, 1, &[g1, g2]).unwrap();
        let c = clauses(&sat);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].len(), 5);
        assert!(c[0].contains(&(!g1).to_ipasir()));
        assert!(c[0].contains(&(!g2).to_ipasir()));
    }

    #[test]
    fn sum_ge_all_is_units() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_ge(&mut sat, &lits, 3, &[]).unwrap();
        let c = clauses(&sat);
        assert_eq!(c.len(), 3);
        for clause in &c {
            assert_eq!(clause.len(), 1);
        }
    }

    #[test]
    fn sum_le_zero_is_units_of_negation() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_le(&mut sat, &lits, 0, &[]).unwrap();
        let c = clauses(&sat);
        assert_eq!(c.len(), 3);
        for (clause, lit) in c.iter().zip(&lits) {
            assert_eq!(clause, &vec![(!*lit).to_ipasir()]);
        }
    }

    #[test]
    fn sum_le_nm1_is_disjunction_of_negations() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_le(&mut sat, &lits, 2, &[]).unwrap();
        let c = clauses(&sat);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].len(), 3);
    }

    #[test]
    fn sum_le_negative_is_unsat() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_le(&mut sat, &lits, -1, &[]).unwrap();
        assert_eq!(clauses(&sat), vec![Vec::<i32>::new()]);
    }

    #[test]
    fn sum_le_n_or_more_is_noop() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_le(&mut sat, &lits, 3, &[]).unwrap();
        encode_sum_le(&mut sat, &lits, 10, &[]).unwrap();
        assert!(clauses(&sat).is_empty());
    }

    #[test]
    fn sum_eq_too_big_emits_single_unsat_clause() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_eq(&mut sat, &lits, 4, &[]).unwrap();
        assert_eq!(clauses(&sat), vec![Vec::<i32>::new()]);
    }

    #[test]
    fn sum_eq_negative_emits_single_unsat_clause() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_eq(&mut sat, &lits, -1, &[]).unwrap();
        assert_eq!(clauses(&sat), vec![Vec::<i32>::new()]);
    }

    #[test]
    fn sum_eq_zero_forces_all_false() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_eq(&mut sat, &lits, 0, &[]).unwrap();
        let c = clauses(&sat);
        assert_eq!(c.len(), 3);
        for (clause, lit) in c.iter().zip(&lits) {
            assert_eq!(clause, &vec![(!*lit).to_ipasir()]);
        }
    }

    #[test]
    fn sum_eq_n_forces_all_true() {
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_eq(&mut sat, &lits, 3, &[]).unwrap();
        let c = clauses(&sat);
        assert_eq!(c.len(), 3);
        for (clause, lit) in c.iter().zip(&lits) {
            assert_eq!(clause, &vec![lit.to_ipasir()]);
        }
    }

    #[test]
    fn sum_eq_one_is_at_least_one_plus_at_most_one() {
        // sum=1 over 3 lits ⟹ one big disjunction (sum_ge=1) plus
        // pairwise-at-most-one expressed via the totalizer (sum_le=1).
        // We just check that the encoding is satisfiable iff exactly one
        // lit is true by feeding it to BatSat.
        use rustsat::solvers::Solve;
        use rustsat::solvers::SolveIncremental;
        use rustsat::types::TernaryVal;
        let mut sat = SatInstance::new();
        let lits = fresh_lits(&mut sat, 3);
        encode_sum_eq(&mut sat, &lits, 1, &[]).unwrap();
        let cnf = sat.cnf().clone();
        // For each of the 8 assignments, check satisfiability with the
        // unit-clause assumptions of that assignment.
        for mask in 0u32..8 {
            let mut solver = rustsat_batsat::BasicSolver::default();
            for clause in cnf.iter() {
                solver
                    .add_clause(clause.clone())
                    .expect("add_clause batsat");
            }
            let assumps: Vec<Lit> = (0..3)
                .map(|i| {
                    if (mask >> i) & 1 == 1 {
                        lits[i]
                    } else {
                        !lits[i]
                    }
                })
                .collect();
            let res = solver
                .solve_assumps(&assumps)
                .expect("batsat solve_assumps");
            let expected_sat = mask.count_ones() == 1;
            assert_eq!(
                res == rustsat::solvers::SolverResult::Sat,
                expected_sat,
                "mask {mask:03b}: expected sat={expected_sat}, got {res:?}"
            );
            let _ = TernaryVal::True; // silence unused-import warning if any
        }
    }
}
