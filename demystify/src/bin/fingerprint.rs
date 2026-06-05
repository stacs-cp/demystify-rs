//! `demystify-fingerprint` — single-puzzle MUS fingerprint frequency dump.
//!
//! Solves one puzzle and reports a frequency table over the canonical
//! fingerprints of the chosen MUSes (one per deduction step). With
//! `--alternatives`, additionally enumerates *all* minimal MUSes available at
//! each state via [`PuzzlePlanner::all_muses_with_larger`] and reports a
//! second table covering alternatives the planner did not pick.
//!
//! This is the per-puzzle DB-curation tool: feed a puzzle through it, see
//! which fingerprints repeat, hand-name them in
//! `demystify/named-strategies/<Kind>.toml`. The corpus-scale aggregator
//! lives in `mystify` (`../ms/`) and reuses the same primitives.

use clap::Parser;
use demystify::{
    named_strategy::{self, Database},
    problem::{
        self,
        musdict::MusContext,
        planner::{PlannerConfig, PuzzlePlanner, UserMus},
        solver::{PuzzleSolver, SolverConfig},
        util::exec::{RunMethod, set_run_method},
    },
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(clap::Parser, Debug)]
#[command(
    about = "Dump fingerprint frequencies for a single puzzle's solve trace.\n\n\
With --alternatives also runs an all-MUS pass at each state — slow, but shows \
which techniques were available but not picked."
)]
struct Opt {
    #[arg(long)]
    model: String,

    #[arg(long)]
    param: String,

    #[arg(
        long,
        help = "Also enumerate all minimal MUSes at each state (slow). Reports a \
second frequency table over alternatives the planner did not pick."
    )]
    alternatives: bool,

    #[arg(
        long,
        value_enum,
        help = "Specify how to run conjure (Native, Docker, Podman)."
    )]
    conjure: Option<RunMethod>,

    #[arg(
        long,
        help = "Path to the named-strategy database directory. Defaults to \
<crate>/named-strategies."
    )]
    strategy_db: Option<PathBuf>,

    #[arg(
        long,
        default_value_t = 1000,
        help = "Per-SAT-call conflict limit (0 = no limit). Default 1000."
    )]
    conflict_limit: i64,
}

#[derive(Default)]
struct Bucket {
    count: usize,
    name: Option<String>,
    sample_constraints: Option<String>,
}

fn record(table: &mut BTreeMap<String, Bucket>, um: &UserMus) {
    let entry = table.entry(um.fingerprint.clone()).or_default();
    entry.count += 1;
    if entry.name.is_none() {
        entry.name.clone_from(&um.name);
    }
    if entry.sample_constraints.is_none() {
        entry.sample_constraints = Some(summarise_constraints(&um.constraints));
    }
}

fn summarise_constraints(constraints: &[String]) -> String {
    const MAX_LEN: usize = 120;
    let joined = constraints.join(" | ");
    if joined.len() <= MAX_LEN {
        joined
    } else {
        format!("{}…", &joined[..MAX_LEN])
    }
}

fn print_table(title: &str, table: &BTreeMap<String, Bucket>) {
    println!("# {title}");
    println!("count\tname\tfingerprint\tsample_constraints");
    let mut rows: Vec<(&String, &Bucket)> = table.iter().collect();
    rows.sort_by(|a, b| b.1.count.cmp(&a.1.count).then_with(|| a.0.cmp(b.0)));
    for (fp, b) in rows {
        let name = b.name.as_deref().unwrap_or("-");
        let sample = b.sample_constraints.as_deref().unwrap_or("");
        println!("{}\t{}\t{}\t{}", b.count, name, fp, sample);
    }
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    if let Some(method) = opt.conjure {
        set_run_method(method);
    }
    demystify::satcore::set_global_conflict_limit(opt.conflict_limit);

    let puzzle =
        problem::parse::parse_essence(&PathBuf::from(&opt.model), &PathBuf::from(&opt.param))?;
    let puzzle = Arc::new(puzzle);

    let solver = PuzzleSolver::new_with_config(
        puzzle,
        SolverConfig {
            only_assignments: false,
        },
    )?;

    let strategy_db = named_strategy::load_or_discover(opt.strategy_db.as_deref())?;

    if opt.alternatives {
        run_with_alternatives(solver, strategy_db)
    } else {
        run_chosen_only(solver, strategy_db)
    }
}

fn run_chosen_only(solver: PuzzleSolver, strategy_db: Arc<Database>) -> anyhow::Result<()> {
    let planner_config = PlannerConfig::default();
    let mut planner =
        PuzzlePlanner::new_with_config(solver, planner_config).with_database(strategy_db);

    let mut chosen: BTreeMap<String, Bucket> = BTreeMap::new();
    for step in planner.quick_solve() {
        for um in step {
            record(&mut chosen, &um);
        }
    }

    print_table("Chosen MUSes", &chosen);
    Ok(())
}

fn run_with_alternatives(solver: PuzzleSolver, strategy_db: Arc<Database>) -> anyhow::Result<()> {
    // Step manually so we can call all_muses_with_larger before each apply.
    let planner_config = PlannerConfig::default();
    let mut planner =
        PuzzlePlanner::new_with_config(solver, planner_config).with_database(strategy_db);

    let mut chosen: BTreeMap<String, Bucket> = BTreeMap::new();
    let mut alternatives: BTreeMap<String, Bucket> = BTreeMap::new();

    while !planner.solver().get_provable_varlits().is_empty() {
        let all = planner.all_muses_with_larger();
        if all.muses().is_empty() {
            break;
        }

        // Per-literal: smallest MUS is the "chosen", everything else "alternative".
        let mut to_apply: Vec<MusContext> = Vec::new();
        for set in all.muses().values() {
            let mut sorted: Vec<&MusContext> = set.iter().collect();
            sorted.sort_by_key(|mc| mc.mus_len());
            if let Some((first, rest)) = sorted.split_first() {
                let chosen_um = planner.mus_to_user_mus(first);
                record(&mut chosen, &chosen_um);
                to_apply.push((*first).clone());
                for mc in rest {
                    let alt_um = planner.mus_to_user_mus(mc);
                    record(&mut alternatives, &alt_um);
                }
            }
        }

        // Apply all chosen MUSes for this state, then loop.
        for mc in &to_apply {
            for lit in &mc.lits {
                planner.mark_lit_as_deduced(lit);
            }
        }
    }

    print_table("Chosen MUSes", &chosen);
    println!();
    print_table("Alternative MUSes (not picked at any step)", &alternatives);
    Ok(())
}
