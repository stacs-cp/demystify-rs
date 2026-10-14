//! Usage: mus_diversity_bench SCRIPT.toml CONFIG.json SEED|none
//!
//! Runs the `show_smallest_muses` alternatives search
//! (`all_muses_for_literal_with_repeats`) for the target of SCRIPT's last
//! step under the MusConfig knobs in CONFIG, and prints one JSON line:
//! every MUS found (each checked with `verify_mus`), SAT calls and
//! conflicts, MUS-search outcome counts, and back-off records.
//!
//! CONFIG keys (all required): repeats, slice_backoff ([start, add_back] or
//! null), shuffle_core, slack, grow, conflict_limit.  SEED `none` leaves the
//! search unseeded (today's behaviour).
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use demystify::problem::solver::SliceBackoff;
use demystify::problem::{parse::parse_essence, planner::PuzzlePlanner, solver::PuzzleSolver};
use demystify::walkthrough::{Executor, Script, Step, parse_lit};
use rustsat::types::Lit;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        args.len() == 3,
        "usage: mus_diversity_bench SCRIPT.toml CONFIG.json SEED|none"
    );
    let cfg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&args[1])?)?;
    let get = |k: &str| {
        cfg.get(k)
            .unwrap_or_else(|| panic!("config is missing key {k:?}"))
    };
    let repeats = get("repeats").as_i64().expect("repeats: integer");
    let backoff = match get("slice_backoff") {
        serde_json::Value::Null => None,
        v => {
            let a = v
                .as_array()
                .expect("slice_backoff: [start, add_back] or null");
            assert!(a.len() == 2, "slice_backoff: [start, add_back]");
            Some(SliceBackoff {
                start: a[0].as_f64().unwrap(),
                add_back: a[1].as_f64().unwrap(),
            })
        }
    };
    let shuffle_core = get("shuffle_core").as_bool().expect("shuffle_core: bool");
    let slack = get("slack").as_i64().expect("slack: integer");
    let grow = get("grow").as_bool().expect("grow: bool");
    let conflict_limit = get("conflict_limit")
        .as_i64()
        .expect("conflict_limit: integer");
    let seed: Option<u64> = if args[2] == "none" {
        None
    } else {
        Some(args[2].parse()?)
    };

    let mut script: Script = toml::from_str(&std::fs::read_to_string(&args[0])?)?;
    let last = script.step.pop().expect("script has no steps");
    let Step::ShowSmallestMuses {
        lit: target,
        strategy,
        ..
    } = last
    else {
        panic!("last step must be show_smallest_muses, got {last:?}");
    };
    assert!(strategy.is_none(), "target step overrides the strategy");
    assert!(script.strategy.is_none(), "script overrides the strategy");
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

    {
        let mc = &mut planner.config_mut().mus_config;
        mc.slice_backoff = backoff;
        mc.shuffle_core = shuffle_core;
        mc.find_bigger_slack = slack;
        mc.find_bigger_grow = grow;
        mc.seed = seed;
    }
    demystify::satcore::set_global_conflict_limit(conflict_limit);
    demystify::stats::reset_sat_stats();
    demystify::stats::reset_mus_stats();
    demystify::stats::take_backoff_records();
    let start = Instant::now();
    let muses = planner.all_muses_for_literal_with_repeats(lit_def, repeats);
    let secs = start.elapsed().as_secs_f64();
    let (sat_calls, _, conflicts) = demystify::stats::sat_stats_snapshot();
    let (searches, found, not_found, timeouts) = demystify::stats::mus_outcome_snapshot();
    let backoff_records = demystify::stats::take_backoff_records();

    let out_muses: Vec<serde_json::Value> = muses
        .iter()
        .map(|m| {
            assert!(
                m.lits.len() == 1 && m.lits.contains(&t),
                "MUS for another literal"
            );
            let cons: Vec<Lit> = m.mus.iter().copied().collect();
            planner.solver().verify_mus(t, &cons);
            let u = planner.mus_to_user_mus(m);
            assert!(
                u.constraints.len() == cons.len(),
                "constraint descriptions are not one-to-one: {} lits, {:?}",
                cons.len(),
                u.constraints
            );
            serde_json::json!({"size": cons.len(), "cons": u.constraints})
        })
        .collect();
    let out: BTreeMap<&str, serde_json::Value> = BTreeMap::from([
        ("script", serde_json::json!(args[0])),
        ("config", cfg.clone()),
        ("seed", serde_json::json!(seed)),
        ("secs", serde_json::json!(secs)),
        ("sat_calls", serde_json::json!(sat_calls)),
        ("conflicts", serde_json::json!(conflicts)),
        ("searches", serde_json::json!(searches)),
        ("found", serde_json::json!(found)),
        ("not_found", serde_json::json!(not_found)),
        ("timeouts", serde_json::json!(timeouts)),
        ("backoff", serde_json::json!(backoff_records)),
        ("muses", serde_json::json!(out_muses)),
    ]);
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}
