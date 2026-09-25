//! Batchable, checkpointed exploration of easiest-move solve graphs.

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use demystify::problem::{
    parse::{PuzzleParse, mystify_puzzle_assignment, parse_essence},
    solver::{MusConfig, PuzzleSolver, SolverConfig, Strategy},
    solvetree::analysis::{NodeStatus, SearchSettings, SolveGraph},
    util::exec::{RunMethod, set_run_method},
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[derive(Parser)]
#[command(about = "Build, resume and study persistent solve graphs for small puzzles.")]
struct Opt {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Explore every easiest-move route, propagate discovered MUSes, and save JSON.
    Build {
        #[arg(
            long,
            required_unless_present = "load_parsed",
            conflicts_with = "load_parsed"
        )]
        model: Option<PathBuf>,
        /// One or more parameter files. Shell globs work; failures do not stop the batch.
        #[arg(long, num_args = 1.., required_unless_present = "load_parsed", conflicts_with = "load_parsed")]
        param: Vec<PathBuf>,
        #[arg(long)]
        load_parsed: Option<PathBuf>,
        /// Output file for a single puzzle.
        #[arg(long, conflicts_with = "out_dir", required_unless_present = "out_dir")]
        out: Option<PathBuf>,
        /// Output directory for a batch; includes batch-summary.json.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        #[arg(long)]
        pin_assignment: Option<PathBuf>,
        /// Target assignments only, excluding individual candidate eliminations.
        #[arg(long)]
        only_assign: bool,
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..))]
        repeats: u32,
        #[arg(long, value_enum, default_value_t = SearchStrategy::Dynamic)]
        strategy: SearchStrategy,
        /// Budget for discovery SAT calls. Propagation checks are always definitive.
        #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(i64).range(0..))]
        conflict_limit: i64,
        #[arg(long, value_enum)]
        conjure: Option<RunMethod>,
        #[arg(long)]
        overwrite: bool,
        #[command(flatten)]
        runtime: Runtime,
    },
    /// Finish an interrupted archive, optionally requesting more MUS discovery.
    Resume {
        #[arg(long)]
        input: PathBuf,
        /// Defaults to replacing the input atomically.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Start a new discovery round at every retained state.
        #[arg(long)]
        search_more: bool,
        /// Also starts a new round; otherwise preserves the previous setting.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        repeats: Option<u32>,
        #[arg(long, value_enum)]
        strategy: Option<SearchStrategy>,
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        conflict_limit: Option<i64>,
        #[command(flatten)]
        runtime: Runtime,
    },
    /// Export a self-contained interactive HTML viewer; no solving or server needed.
    Render {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Print machine-readable summary statistics without running the solver.
    Inspect {
        #[arg(long)]
        input: PathBuf,
    },
}

#[derive(clap::Args)]
struct Runtime {
    /// Save after a unit of work when this interval has elapsed (0 = every unit).
    #[arg(long, default_value_t = 30)]
    checkpoint_seconds: u64,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    threads: Option<u32>,
}

#[derive(Clone, Copy, ValueEnum)]
enum SearchStrategy {
    Quick,
    Slice,
    Cake,
    Dynamic,
}
impl From<SearchStrategy> for Strategy {
    fn from(value: SearchStrategy) -> Self {
        match value {
            SearchStrategy::Quick => Self::Quick,
            SearchStrategy::Slice => Self::Slice,
            SearchStrategy::Cake => Self::Cake,
            SearchStrategy::Dynamic => Self::Dynamic,
        }
    }
}

fn output_stem(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .context("Input has no filename")?
        .to_string_lossy();
    if let Some(stem) = name.strip_suffix(".json.zst") {
        return Ok(stem.to_owned());
    }
    Ok(path
        .file_stem()
        .context("Input has no filename stem")?
        .to_string_lossy()
        .into_owned())
}

#[derive(Serialize)]
struct Summary {
    source: String,
    archive: PathBuf,
    fixed_point: bool,
    active_nodes: usize,
    retained_nodes: usize,
    active_edges: usize,
    proofs: usize,
    solved_leaves: usize,
    underdetermined_leaves: usize,
    search_incomplete_leaves: usize,
    propagation_checks: usize,
    difficulty_max: usize,
    opening_width: usize,
    opening_mus: usize,
    saved_solver_config: SolverConfig,
    saved_searches: Vec<SearchSettings>,
}

fn summary(graph: &SolveGraph, path: &Path) -> Summary {
    let depths = graph.depths();
    let active: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(i, _)| depths[*i].is_some())
        .map(|(_, n)| n)
        .collect();
    let active_edges = active
        .iter()
        .flat_map(|node| node.edges.iter().filter(|edge| edge.active));
    let difficulty_max = active_edges
        .clone()
        .map(|edge| graph.proofs[edge.proof].constraints.len())
        .max()
        .unwrap_or(0);
    let opening_edges: Vec<_> = graph.nodes[graph.root]
        .edges
        .iter()
        .filter(|edge| edge.active)
        .collect();
    Summary {
        source: graph.source.clone(),
        archive: path.into(),
        fixed_point: graph.fixed_point,
        active_nodes: active.len(),
        retained_nodes: graph.nodes.len(),
        active_edges: active
            .iter()
            .map(|n| n.edges.iter().filter(|e| e.active).count())
            .sum(),
        proofs: graph.proofs.len(),
        solved_leaves: active
            .iter()
            .filter(|n| n.status == NodeStatus::Solved)
            .count(),
        underdetermined_leaves: active
            .iter()
            .filter(|n| n.status == NodeStatus::Underdetermined)
            .count(),
        search_incomplete_leaves: active
            .iter()
            .filter(|n| n.status == NodeStatus::SearchIncomplete)
            .count(),
        propagation_checks: graph.propagation_checks,
        difficulty_max,
        opening_width: opening_edges
            .iter()
            .map(|edge| edge.target)
            .collect::<BTreeSet<_>>()
            .len(),
        opening_mus: opening_edges
            .iter()
            .map(|edge| graph.proofs[edge.proof].constraints.len())
            .min()
            .unwrap_or(0),
        saved_solver_config: graph.solver_config,
        saved_searches: graph.searches.clone(),
    }
}

fn setup(runtime: &Runtime) -> Result<()> {
    if let Some(threads) = runtime.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads as usize)
            .build_global()?;
    }
    Ok(())
}

fn run(graph: &mut SolveGraph, output: &Path, runtime: &Runtime) -> Result<Summary> {
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    demystify::satcore::set_global_conflict_limit(graph.searches.last().unwrap().conflict_limit);
    graph.save(output)?;
    let mut last_save = Instant::now();
    graph.run(|g| {
        if g.fixed_point || last_save.elapsed().as_secs() >= runtime.checkpoint_seconds {
            g.save(output)?;
            eprintln!(
                "{}: {} retained states, {} proofs, {} propagation checks{}",
                output.display(),
                g.nodes.len(),
                g.proofs.len(),
                g.propagation_checks,
                if g.fixed_point { ", fixed point" } else { "" }
            );
            last_save = Instant::now();
        }
        Ok(())
    })?;
    graph.save(output)?;
    Ok(summary(graph, output))
}

fn main() -> Result<()> {
    match Opt::parse().command {
        Command::Build {
            model,
            param,
            load_parsed,
            out,
            out_dir,
            pin_assignment,
            only_assign,
            repeats,
            strategy,
            conflict_limit,
            conjure,
            overwrite,
            runtime,
        } => {
            setup(&runtime)?;
            if let Some(method) = conjure {
                set_run_method(method);
            }
            let inputs = if let Some(parsed) = &load_parsed {
                vec![parsed.clone()]
            } else {
                param
            };
            ensure!(
                out.is_none() || inputs.len() == 1,
                "Use --out-dir for multiple puzzles"
            );
            let mut names = BTreeSet::new();
            let mut jobs = vec![];
            for input in inputs {
                let output = match &out {
                    Some(path) => path.clone(),
                    None => {
                        let stem = output_stem(&input)?;
                        out_dir
                            .as_ref()
                            .unwrap()
                            .join(format!("{stem}.graph.json.zst"))
                    }
                };
                ensure!(
                    names.insert(output.clone()),
                    "Batch inputs have colliding output names: {}",
                    output.display()
                );
                jobs.push((input, output));
            }
            let mut results = vec![];
            let mut failures = 0;
            for (input, output) in jobs {
                let result = (|| -> Result<Summary> {
                    ensure!(
                        overwrite || !output.exists(),
                        "{} already exists; use resume or --overwrite",
                        output.display()
                    );
                    eprintln!("Building {}", input.display());
                    demystify::satcore::set_global_conflict_limit(conflict_limit);
                    let puzzle = if load_parsed.is_some() {
                        PuzzleParse::load_from_json(&input)?
                    } else {
                        parse_essence(model.as_ref().unwrap(), &input)?
                    };
                    let mut solver = PuzzleSolver::new_with_config(
                        Arc::new(puzzle),
                        SolverConfig {
                            only_assignments: only_assign,
                        },
                    )?;
                    if let Some(pin) = &pin_assignment {
                        let value: serde_json::Value = serde_json::from_reader(File::open(pin)?)?;
                        let assignment = if value.get("puzzle").is_some() {
                            mystify_puzzle_assignment(&value)?
                        } else {
                            &value
                        };
                        solver.pin_assignment(assignment)?;
                    }
                    let source = match &model {
                        Some(m) => format!("{} | {}", m.display(), input.display()),
                        None => input.display().to_string(),
                    };
                    let mut mus = MusConfig::new_with_repeats(i64::from(repeats));
                    mus.strategy = strategy.into();
                    let mut graph = SolveGraph::new(
                        solver,
                        source,
                        SearchSettings {
                            mus,
                            conflict_limit,
                        },
                    )?;
                    run(&mut graph, &output, &runtime)
                })();
                match result {
                    Ok(s) => {
                        println!("{}", serde_json::to_string(&s)?);
                        results.push(serde_json::to_value(s)?);
                    }
                    Err(error) => {
                        failures += 1;
                        eprintln!("{}: {error:#}", input.display());
                        results.push(serde_json::json!({"input": input, "archive": output, "error": format!("{error:#}")}));
                    }
                }
                if let Some(directory) = &out_dir {
                    std::fs::create_dir_all(directory)?;
                    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
                    serde_json::to_writer_pretty(&mut temp, &results)?;
                    temp.as_file().sync_all()?;
                    temp.persist(directory.join("batch-summary.json"))?;
                }
            }
            ensure!(
                failures == 0,
                "{failures} puzzle(s) failed; other batch outputs have been saved"
            );
        }
        Command::Resume {
            input,
            out,
            search_more,
            repeats,
            strategy,
            conflict_limit,
            runtime,
        } => {
            setup(&runtime)?;
            let mut graph = SolveGraph::load(&input)?;
            if search_more || repeats.is_some() || strategy.is_some() || conflict_limit.is_some() {
                let mut settings = graph.searches.last().unwrap().clone();
                if let Some(n) = repeats {
                    settings.mus.repeats = i64::from(n);
                }
                if let Some(s) = strategy {
                    settings.mus.strategy = s.into();
                }
                if let Some(n) = conflict_limit {
                    settings.conflict_limit = n;
                }
                graph.search_more(settings)?;
            }
            let s = run(&mut graph, out.as_ref().unwrap_or(&input), &runtime)?;
            println!("{}", serde_json::to_string(&s)?);
        }
        Command::Render { input, out } => {
            let graph = SolveGraph::load(&input)?;
            std::fs::write(&out, graph.to_html()?)?;
            eprintln!("Wrote {}", out.display());
        }
        Command::Inspect { input } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&summary(&SolveGraph::load(&input)?, &input))?
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::output_stem;
    use std::path::Path;

    #[test]
    fn batch_output_stem_handles_compressed_json_as_one_suffix() {
        assert_eq!(output_stem(Path::new("puzzle.json.zst")).unwrap(), "puzzle");
        assert_eq!(output_stem(Path::new("puzzle.param")).unwrap(), "puzzle");
    }
}
