//! Usage: deep_mus_search SCRIPT.toml search SECONDS SEED
//!        deep_mus_search SCRIPT.toml list SUBSTRING
//!        deep_mus_search SCRIPT.toml check CONSTRAINT...
//!
//! Diversified search for small MUSes of one target literal.  SCRIPT is a
//! walkthrough whose last step is `show_smallest_muses`; every earlier step
//! is run to build the state.  Each iteration keeps a random fraction of the
//! constraints, sometimes first removing one or two constraints of an
//! already-found smallest MUS (so the search must find a different one),
//! takes a SAT core and deletion-minimises it in random order.  SAT calls
//! have no conflict limit, so every reported set is a genuine MUS.
//! Prints one JSON object: every distinct MUS of the smallest size found,
//! and the iterations at which the smallest size improved.
//!
//! `list` prints the constraints whose description contains SUBSTRING;
//! `check` reports whether the named constraints (by description) form a
//! MUS for the target, i.e. prove it and none can be dropped.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

use demystify::problem::musdict::MusContext;
use demystify::problem::{parse::parse_essence, planner::PuzzlePlanner, solver::PuzzleSolver};
use demystify::satcore::SatCore;
use demystify::walkthrough::{Executor, Script, Step, parse_lit};
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};
use rustsat::types::Lit;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 2,
        "usage: SCRIPT.toml (search SECONDS SEED | list SUBSTRING | check CONSTRAINT...)"
    );
    let mut script: Script = toml::from_str(&std::fs::read_to_string(&args[0])?)?;
    let last = script.step.pop().expect("script has no steps");
    let Step::ShowSmallestMuses { lit: target, .. } = last else {
        panic!("last step must be show_smallest_muses, got {last:?}");
    };
    let puz = parse_essence(&script.model, &script.param)?;
    let mut planner = PuzzlePlanner::new(PuzzleSolver::new(Arc::new(puz))?);
    Executor::new(&mut planner).run(&script)?;

    let lit_def = parse_lit(&target)?.lit_def();
    let solver = planner.solver();
    let provable: Vec<Lit> = solver
        .get_provable_varlits()
        .clone()
        .into_iter()
        .filter(|l| {
            solver.lit_to_puzlit(l).iter().any(|p| {
                let mut d = p.var().indices().clone();
                d.push(p.val());
                d == lit_def
            })
        })
        .collect();
    assert!(
        provable.len() == 1,
        "target {target} matches {} provable lits",
        provable.len()
    );
    let t = provable[0];

    demystify::satcore::set_global_conflict_limit(0);
    let parse = solver.puzzleparse_arc();
    let cons: Vec<Lit> = parse.constraints.lits().iter().copied().collect();
    let con_set: BTreeSet<Lit> = cons.iter().copied().collect();
    let known = planner.get_all_known_lits().clone();
    let sat = SatCore::new(parse.cnf.clone().unwrap())?;
    // UNSAT core of `subset` + !t, restricted to constraints; None if SAT.
    let core = |subset: &[Lit]| -> Option<Vec<Lit>> {
        let mut a = subset.to_vec();
        a.push(!t);
        sat.assumption_solve_with_core(&known, &a)
            .expect("unlimited solve cannot time out")
            .map(|c| c.into_iter().filter(|x| con_set.contains(x)).collect())
    };
    assert!(
        core(&cons).is_some(),
        "target {target} is not provable from all constraints"
    );

    if args[1] == "list" || args[1] == "check" {
        let mut by_name: BTreeMap<String, Lit> = BTreeMap::new();
        for &c in &cons {
            let u = planner.mus_to_user_mus(&MusContext::new(t, BTreeSet::from([c])));
            assert!(
                u.constraints.len() == 1,
                "constraint renders as {:?}",
                u.constraints
            );
            let prev = by_name.insert(u.constraints[0].clone(), c);
            assert!(
                prev.is_none(),
                "two constraints share the description {:?}",
                u.constraints[0]
            );
        }
        if args[1] == "list" {
            assert!(args.len() == 3, "list takes one SUBSTRING");
            for name in by_name.keys().filter(|n| n.contains(&args[2])) {
                println!("{name}");
            }
            return Ok(());
        }
        let set: Vec<Lit> = args[2..]
            .iter()
            .map(|n| {
                *by_name
                    .get(n)
                    .unwrap_or_else(|| panic!("no constraint described as {n:?}"))
            })
            .collect();
        let proves = core(&set).is_some();
        let droppable: Vec<&String> = args[2..]
            .iter()
            .zip(&set)
            .filter(|&(_, &c)| {
                core(&set.iter().copied().filter(|&x| x != c).collect::<Vec<_>>()).is_some()
            })
            .map(|(n, _)| n)
            .collect();
        println!(
            "{}",
            serde_json::json!({"script": args[0], "target": target, "size": set.len(),
                "proves": proves, "droppable": droppable, "is_mus": proves && droppable.is_empty()})
        );
        return Ok(());
    }
    assert!(
        args[1] == "search" && args.len() == 4,
        "search takes SECONDS SEED"
    );
    let secs: f64 = args[2].parse()?;
    let seed: u64 = args[3].parse()?;

    let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(seed);
    let mut best = usize::MAX;
    let mut found: BTreeSet<BTreeSet<Lit>> = BTreeSet::new();
    let mut improvements = vec![];
    let (mut iters, mut unsat_iters) = (0u64, 0u64);
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < secs {
        iters += 1;
        let mut pool: Vec<Lit> = cons.clone();
        if !found.is_empty() && rng.random_bool(0.5) {
            let m: Vec<Lit> = found
                .iter()
                .nth(rng.random_range(0..found.len()))
                .unwrap()
                .iter()
                .copied()
                .collect();
            let n = rng.random_range(1..=2.min(m.len()));
            let drop: BTreeSet<Lit> = m.sample(&mut rng, n).copied().collect();
            pool.retain(|x| !drop.contains(x));
        }
        let keep = *[1.0, 0.7, 0.5, 0.3, 0.2, 0.1].choose(&mut rng).unwrap();
        pool.shuffle(&mut rng);
        pool.truncate(((pool.len() as f64) * keep).ceil() as usize);
        let Some(mut q) = core(&pool) else { continue };
        unsat_iters += 1;
        q.shuffle(&mut rng);
        let order = q.clone();
        let mut needed: BTreeSet<Lit> = BTreeSet::new();
        let mut aborted = false;
        for c in order {
            if !q.contains(&c) {
                continue;
            }
            let rest: Vec<Lit> = q.iter().copied().filter(|&x| x != c).collect();
            match core(&rest) {
                Some(k) => q = k,
                None => {
                    needed.insert(c);
                    if needed.len() > best {
                        aborted = true;
                        break;
                    }
                }
            }
        }
        if aborted {
            continue;
        }
        let mus: BTreeSet<Lit> = q.iter().copied().collect();
        assert_eq!(
            mus, needed,
            "minimised core is not the set of necessary constraints"
        );
        if mus.len() < best {
            best = mus.len();
            found.clear();
            improvements.push((iters, best));
        }
        found.insert(mus);
    }

    let muses: Vec<serde_json::Value> = found
        .iter()
        .map(|m| {
            let u = planner.mus_to_user_mus(&MusContext::new(t, m.clone()));
            serde_json::json!({"size": m.len(), "cons": u.constraints})
        })
        .collect();
    let out: BTreeMap<&str, serde_json::Value> = BTreeMap::from([
        ("script", serde_json::json!(args[0])),
        ("seed", serde_json::json!(seed)),
        ("secs", serde_json::json!(start.elapsed().as_secs_f64())),
        ("iters", serde_json::json!(iters)),
        ("unsat_iters", serde_json::json!(unsat_iters)),
        ("improvements", serde_json::json!(improvements)),
        ("muses", serde_json::json!(muses)),
    ]);
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}
