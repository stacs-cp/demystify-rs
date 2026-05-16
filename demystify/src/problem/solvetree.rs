use std::collections::{BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{IsTerminal, Write};
use std::sync::Arc;
use web_time::Instant;

use rustsat::types::Lit;
use serde::Serialize;
use tracing::info;

use super::musdict::{MusContext, merge_muscontexts};
use super::parse::PuzzleParse;
use super::planner::{PlannerConfig, PuzzlePlanner};
use super::solver::{MusConfig, PuzzleSolver, SolverConfig};

type StateHash = u64;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum MergeStrategy {
    #[default]
    None,
    Greedy,
    Minimal,
}

#[derive(Clone)]
pub struct SolveTreeConfig {
    pub mus_config: MusConfig,
    pub solver_config: SolverConfig,
    pub merge_strategy: MergeStrategy,
    pub merge_mus_size: usize,
}

impl Default for SolveTreeConfig {
    fn default() -> Self {
        Self {
            mus_config: MusConfig {
                find_one: false,
                ..MusConfig::new_with_repeats(5)
            },
            solver_config: SolverConfig::default(),
            merge_strategy: MergeStrategy::default(),
            merge_mus_size: 1,
        }
    }
}

#[derive(Clone)]
pub struct SolveTreeNode {
    pub state_hash: StateHash,
    pub known_lits: Vec<Lit>,
    pub remaining_count: usize,
    pub min_mus_count: usize,
    pub min_mus_size: usize,
    pub is_terminal: bool,
    pub children: Vec<SolveTreeEdge>,
    pub depth: usize,
}

#[derive(Clone)]
pub struct SolveTreeEdge {
    pub mus: BTreeSet<Lit>,
    pub deduced_lits: BTreeSet<Lit>,
    pub description: String,
    pub target_hash: StateHash,
    pub merged: Option<Vec<MergedMusInfo>>,
}

#[derive(Clone, Serialize)]
pub struct MergedMusInfo {
    pub description: String,
    pub deduced_count: usize,
    pub known_lits_after: usize,
}

struct LatticeEntry {
    lower: BTreeSet<Lit>,
    delta: BTreeSet<Lit>,
    lower_size: usize,
    upper_size: usize,
    upper_hash: StateHash,
}

pub struct SolveTree {
    pub nodes: HashMap<StateHash, SolveTreeNode>,
    pub root_hash: StateHash,
    pub lattice_hits: usize,
}

fn hash_lits(lits: &[Lit]) -> StateHash {
    let mut sorted: Vec<Lit> = lits.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut hasher = DefaultHasher::new();
    sorted.hash(&mut hasher);
    hasher.finish()
}

fn get_provable_at(
    puzzleparse: &Arc<PuzzleParse>,
    known_lits: &[Lit],
    solver_config: SolverConfig,
) -> anyhow::Result<BTreeSet<Lit>> {
    let mut solver =
        PuzzleSolver::fork_with_known_lits(puzzleparse.clone(), known_lits, solver_config)?;
    Ok(solver.get_provable_varlits().clone())
}

fn child_known_lits(parent_known: &[Lit], deduced: &BTreeSet<Lit>) -> Vec<Lit> {
    let mut child = parent_known.to_vec();
    for &lit in deduced {
        if !child.contains(&lit) {
            child.push(lit);
        }
    }
    child
}

fn check_lattice(known_lits: &[Lit], lattice_cache: &[LatticeEntry]) -> Option<StateHash> {
    let s_size = known_lits.len();
    let s_hash = hash_lits(known_lits);

    if lattice_cache.iter().any(|entry| entry.upper_hash == s_hash) {
        return None;
    }

    let s_set: BTreeSet<Lit> = known_lits.iter().copied().collect();

    for entry in lattice_cache {
        if s_size < entry.lower_size || s_size > entry.upper_size {
            continue;
        }
        if !entry.lower.is_subset(&s_set) {
            continue;
        }
        if s_set
            .difference(&entry.lower)
            .all(|l| entry.delta.contains(l))
        {
            return Some(entry.upper_hash);
        }
    }
    None
}

fn is_already_covered(
    child_hash: StateHash,
    child_known: &[Lit],
    nodes: &HashMap<StateHash, SolveTreeNode>,
    lattice_cache: &[LatticeEntry],
) -> Option<StateHash> {
    if nodes.contains_key(&child_hash) {
        return Some(child_hash);
    }
    check_lattice(child_known, lattice_cache)
}

fn combined_is_progressing(
    parent_known: &[Lit],
    indices: &[usize],
    candidates: &[ChildCandidate],
    parent_provable: &BTreeSet<Lit>,
    puzzleparse: &Arc<PuzzleParse>,
    solver_config: SolverConfig,
) -> anyhow::Result<bool> {
    let combined = combined_known(parent_known, indices, candidates);
    let combined_provable = get_provable_at(puzzleparse, &combined, solver_config)?;
    Ok(combined_provable
        .iter()
        .any(|l| !parent_provable.contains(l)))
}

fn combined_known(
    parent_known: &[Lit],
    indices: &[usize],
    candidates: &[ChildCandidate],
) -> Vec<Lit> {
    let mut combined = parent_known.to_vec();
    for &i in indices {
        for &lit in &candidates[i].mus_ctx.lits {
            if !combined.contains(&lit) {
                combined.push(lit);
            }
        }
    }
    combined
}

struct ChildCandidate {
    mus_ctx: MusContext,
    child_known: Vec<Lit>,
    child_hash: StateHash,
    description: String,
    provable: Option<BTreeSet<Lit>>,
    is_progressing: Option<bool>,
}

impl SolveTree {
    pub fn build(
        puzzleparse: Arc<PuzzleParse>,
        config: &SolveTreeConfig,
    ) -> anyhow::Result<SolveTree> {
        let planner_config = PlannerConfig {
            mus_config: config.mus_config,
            merge_small_threshold: -1,
            skip_small_threshold: -1,
            expand_to_all_deductions: false,
            max_steps: None,
            mus_method: Default::default(),
            verbose: false,
        };

        let root_solver = PuzzleSolver::new_with_config(puzzleparse.clone(), config.solver_config)?;
        let root_planner = PuzzlePlanner::new_with_config(root_solver, planner_config);
        let root_known = root_planner.get_all_known_lits().clone();
        let root_hash = hash_lits(&root_known);

        let mut nodes: HashMap<StateHash, SolveTreeNode> = HashMap::new();
        let mut lattice_cache: Vec<LatticeEntry> = Vec::new();
        let mut lattice_hits: usize = 0;

        let mut stack: Vec<(Vec<Lit>, StateHash, usize)> = vec![(root_known, root_hash, 0)];

        let show_progress = std::io::stderr().is_terminal();
        let build_start = Instant::now();
        let mut nodes_processed: usize = 0;

        while let Some((known_lits, state_hash, depth)) = stack.pop() {
            if nodes.contains_key(&state_hash) {
                continue;
            }
            if check_lattice(&known_lits, &lattice_cache).is_some() {
                lattice_hits += 1;
                continue;
            }

            let solver = PuzzleSolver::fork_with_known_lits(
                puzzleparse.clone(),
                &known_lits,
                config.solver_config,
            )?;
            let mut planner = PuzzlePlanner::new_with_config(solver, planner_config);

            let parent_provable = planner.solver().get_provable_varlits().clone();
            let remaining = parent_provable.len();

            let muses = planner.smallest_muses();
            let muses = merge_muscontexts(&muses);

            if muses.is_empty() {
                nodes.insert(
                    state_hash,
                    SolveTreeNode {
                        state_hash,
                        known_lits,
                        remaining_count: remaining,
                        min_mus_count: 0,
                        min_mus_size: 0,
                        is_terminal: true,
                        children: vec![],
                        depth,
                    },
                );
                nodes_processed += 1;
                if show_progress {
                    let elapsed = build_start.elapsed().as_secs_f64();
                    let secs_per_node = elapsed / nodes_processed as f64;
                    eprint!(
                        "\r\x1b[K[solvetree] nodes: {} | queue: {} | depth: {} (T) | lattice: {} | {:.2}s/node | {:.1}s elapsed",
                        nodes.len(),
                        stack.len(),
                        depth,
                        lattice_cache.len(),
                        secs_per_node,
                        elapsed,
                    );
                    let _ = std::io::stderr().flush();
                }
                info!(
                    target: "solvetree",
                    "Terminal node at depth {depth}, {remaining} remaining, \
                     {} nodes, {} lattices so far",
                    nodes.len(),
                    lattice_cache.len()
                );
                continue;
            }

            let min_mus_size = muses[0].mus_len();
            let min_mus_count = muses.len();

            let mut candidates: Vec<ChildCandidate> = muses
                .iter()
                .map(|mus_ctx| {
                    let child_known = child_known_lits(&known_lits, &mus_ctx.lits);
                    let child_hash = hash_lits(&child_known);
                    let user_mus = planner.mus_to_user_mus(mus_ctx);
                    let description = format!(
                        "{} because {}",
                        user_mus
                            .lits
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                        user_mus.constraints.join(", ")
                    );
                    ChildCandidate {
                        mus_ctx: mus_ctx.clone(),
                        child_known,
                        child_hash,
                        description,
                        provable: None,
                        is_progressing: None,
                    }
                })
                .collect();

            let children = if min_mus_size <= config.merge_mus_size {
                let all_indices: Vec<usize> = (0..candidates.len()).collect();
                self_merge_all_and_register(
                    &all_indices,
                    &candidates,
                    &known_lits,
                    &nodes,
                    &mut lattice_cache,
                    &mut lattice_hits,
                    &mut stack,
                    depth,
                )?
            } else {
                match config.merge_strategy {
                    MergeStrategy::None => Self::build_edges_no_merge(
                        &candidates,
                        &nodes,
                        &lattice_cache,
                        &mut lattice_hits,
                        &mut stack,
                        depth,
                    ),
                    MergeStrategy::Greedy => Self::build_edges_greedy(
                        &mut candidates,
                        &parent_provable,
                        &known_lits,
                        &puzzleparse,
                        config.solver_config,
                        &nodes,
                        &mut lattice_cache,
                        &mut lattice_hits,
                        &mut stack,
                        depth,
                    )?,
                    MergeStrategy::Minimal => Self::build_edges_minimal(
                        &mut candidates,
                        &parent_provable,
                        &known_lits,
                        &puzzleparse,
                        config.solver_config,
                        &nodes,
                        &mut lattice_cache,
                        &mut lattice_hits,
                        &mut stack,
                        depth,
                    )?,
                }
            };

            info!(
                target: "solvetree",
                "Node at depth {depth}: {min_mus_count} MUSes of size {min_mus_size}, \
                 {} children after merge, {remaining} remaining, \
                 {} nodes, {} lattices so far",
                children.len(),
                nodes.len(),
                lattice_cache.len()
            );

            nodes.insert(
                state_hash,
                SolveTreeNode {
                    state_hash,
                    known_lits,
                    remaining_count: remaining,
                    min_mus_count,
                    min_mus_size,
                    is_terminal: false,
                    children,
                    depth,
                },
            );

            nodes_processed += 1;
            if show_progress {
                let elapsed = build_start.elapsed().as_secs_f64();
                let secs_per_node = if nodes_processed > 0 {
                    elapsed / nodes_processed as f64
                } else {
                    0.0
                };
                eprint!(
                    "\r\x1b[K[solvetree] nodes: {} | queue: {} | depth: {} | lattice: {} | {:.2}s/node | {:.1}s elapsed",
                    nodes.len(),
                    stack.len(),
                    depth,
                    lattice_cache.len(),
                    secs_per_node,
                    elapsed,
                );
                let _ = std::io::stderr().flush();
            }
        }

        if show_progress {
            let elapsed = build_start.elapsed().as_secs_f64();
            eprintln!(
                "\r\x1b[K[solvetree] done: {} nodes, {} lattice entries, {} lattice hits, {:.1}s total",
                nodes.len(),
                lattice_cache.len(),
                lattice_hits,
                elapsed,
            );
        }

        info!(
            target: "solvetree",
            "Solve tree complete: {} nodes, {} lattice entries, {} lattice hits",
            nodes.len(),
            lattice_cache.len(),
            lattice_hits,
        );

        Ok(SolveTree {
            nodes,
            root_hash,
            lattice_hits,
        })
    }

    fn build_edges_no_merge(
        candidates: &[ChildCandidate],
        nodes: &HashMap<StateHash, SolveTreeNode>,
        lattice_cache: &[LatticeEntry],
        lattice_hits: &mut usize,
        stack: &mut Vec<(Vec<Lit>, StateHash, usize)>,
        depth: usize,
    ) -> Vec<SolveTreeEdge> {
        let mut edges = Vec::with_capacity(candidates.len());
        for c in candidates {
            let target =
                match is_already_covered(c.child_hash, &c.child_known, nodes, lattice_cache) {
                    Some(redirect) => {
                        *lattice_hits += usize::from(redirect != c.child_hash);
                        redirect
                    }
                    None => {
                        stack.push((c.child_known.clone(), c.child_hash, depth + 1));
                        c.child_hash
                    }
                };
            edges.push(SolveTreeEdge {
                mus: c.mus_ctx.mus.clone(),
                deduced_lits: c.mus_ctx.lits.clone(),
                description: c.description.clone(),
                target_hash: target,
                merged: None,
            });
        }
        edges
    }

    #[allow(clippy::too_many_arguments)]
    fn build_edges_greedy(
        candidates: &mut [ChildCandidate],
        parent_provable: &BTreeSet<Lit>,
        parent_known: &[Lit],
        puzzleparse: &Arc<PuzzleParse>,
        solver_config: SolverConfig,
        nodes: &HashMap<StateHash, SolveTreeNode>,
        lattice_cache: &mut Vec<LatticeEntry>,
        lattice_hits: &mut usize,
        stack: &mut Vec<(Vec<Lit>, StateHash, usize)>,
        depth: usize,
    ) -> anyhow::Result<Vec<SolveTreeEdge>> {
        let all_indices: Vec<usize> = (0..candidates.len()).collect();

        let all_progressing = combined_is_progressing(
            parent_known,
            &all_indices,
            candidates,
            parent_provable,
            puzzleparse,
            solver_config,
        )?;

        if !all_progressing {
            return self_merge_all_and_register(
                &all_indices,
                candidates,
                parent_known,
                nodes,
                lattice_cache,
                lattice_hits,
                stack,
                depth,
            );
        }

        Self::classify_candidates(candidates, parent_provable, puzzleparse, solver_config)?;

        let non_prog: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_progressing == Some(false))
            .map(|(i, _)| i)
            .collect();
        let prog: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_progressing == Some(true))
            .map(|(i, _)| i)
            .collect();

        let mut edges = Vec::new();
        push_individual_edges(
            &prog,
            candidates,
            nodes,
            lattice_cache,
            lattice_hits,
            stack,
            depth,
            &mut edges,
        );

        if non_prog.len() <= 1 {
            push_individual_edges(
                &non_prog,
                candidates,
                nodes,
                lattice_cache,
                lattice_hits,
                stack,
                depth,
                &mut edges,
            );
        } else {
            push_merged_edge_and_register(
                &non_prog,
                candidates,
                parent_known,
                nodes,
                lattice_cache,
                lattice_hits,
                stack,
                depth,
                &mut edges,
            )?;
        }

        Ok(edges)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_edges_minimal(
        candidates: &mut [ChildCandidate],
        parent_provable: &BTreeSet<Lit>,
        parent_known: &[Lit],
        puzzleparse: &Arc<PuzzleParse>,
        solver_config: SolverConfig,
        nodes: &HashMap<StateHash, SolveTreeNode>,
        lattice_cache: &mut Vec<LatticeEntry>,
        lattice_hits: &mut usize,
        stack: &mut Vec<(Vec<Lit>, StateHash, usize)>,
        depth: usize,
    ) -> anyhow::Result<Vec<SolveTreeEdge>> {
        let all_indices: Vec<usize> = (0..candidates.len()).collect();

        let all_progressing = combined_is_progressing(
            parent_known,
            &all_indices,
            candidates,
            parent_provable,
            puzzleparse,
            solver_config,
        )?;

        if !all_progressing {
            return self_merge_all_and_register(
                &all_indices,
                candidates,
                parent_known,
                nodes,
                lattice_cache,
                lattice_hits,
                stack,
                depth,
            );
        }

        Self::classify_candidates(candidates, parent_provable, puzzleparse, solver_config)?;

        let mut edges = Vec::new();
        let mut used = vec![false; candidates.len()];

        for (i, c) in candidates.iter().enumerate() {
            if c.is_progressing == Some(true) {
                used[i] = true;
                let target =
                    match is_already_covered(c.child_hash, &c.child_known, nodes, lattice_cache) {
                        Some(redirect) => {
                            *lattice_hits += usize::from(redirect != c.child_hash);
                            redirect
                        }
                        None => {
                            stack.push((c.child_known.clone(), c.child_hash, depth + 1));
                            c.child_hash
                        }
                    };
                edges.push(SolveTreeEdge {
                    mus: c.mus_ctx.mus.clone(),
                    deduced_lits: c.mus_ctx.lits.clone(),
                    description: c.description.clone(),
                    target_hash: target,
                    merged: None,
                });
            }
        }

        loop {
            let remaining_np: Vec<usize> = (0..candidates.len())
                .filter(|&i| !used[i] && candidates[i].is_progressing == Some(false))
                .collect();
            if remaining_np.is_empty() {
                break;
            }

            let mut merge_set = vec![remaining_np[0]];
            used[remaining_np[0]] = true;

            for &j in &remaining_np[1..] {
                let mut test_set = merge_set.clone();
                test_set.push(j);
                let prog = combined_is_progressing(
                    parent_known,
                    &test_set,
                    candidates,
                    parent_provable,
                    puzzleparse,
                    solver_config,
                )?;
                if !prog {
                    merge_set.push(j);
                    used[j] = true;
                }
            }

            if merge_set.len() == 1 {
                let i = merge_set[0];
                let c = &candidates[i];
                let target =
                    match is_already_covered(c.child_hash, &c.child_known, nodes, lattice_cache) {
                        Some(redirect) => {
                            *lattice_hits += usize::from(redirect != c.child_hash);
                            redirect
                        }
                        None => {
                            stack.push((c.child_known.clone(), c.child_hash, depth + 1));
                            c.child_hash
                        }
                    };
                edges.push(SolveTreeEdge {
                    mus: c.mus_ctx.mus.clone(),
                    deduced_lits: c.mus_ctx.lits.clone(),
                    description: c.description.clone(),
                    target_hash: target,
                    merged: None,
                });
            } else {
                push_merged_edge_and_register(
                    &merge_set,
                    candidates,
                    parent_known,
                    nodes,
                    lattice_cache,
                    lattice_hits,
                    stack,
                    depth,
                    &mut edges,
                )?;
            }
        }

        Ok(edges)
    }

    fn classify_candidates(
        candidates: &mut [ChildCandidate],
        parent_provable: &BTreeSet<Lit>,
        puzzleparse: &Arc<PuzzleParse>,
        solver_config: SolverConfig,
    ) -> anyhow::Result<()> {
        for c in candidates.iter_mut() {
            if c.provable.is_some() {
                continue;
            }
            let child_provable = get_provable_at(puzzleparse, &c.child_known, solver_config)?;
            let is_progressing = child_provable.iter().any(|l| !parent_provable.contains(l));
            c.provable = Some(child_provable);
            c.is_progressing = Some(is_progressing);
        }
        Ok(())
    }

    pub fn to_d3_json(&self) -> SolveTreeJson {
        let mut json_nodes = Vec::new();
        let mut json_links = Vec::new();

        for node in self.nodes.values() {
            json_nodes.push(SolveTreeJsonNode {
                id: format!("{:016x}", node.state_hash),
                depth: node.depth,
                remaining: node.remaining_count,
                min_mus_count: node.min_mus_count,
                min_mus_size: node.min_mus_size,
                is_terminal: node.is_terminal,
                known_lits_count: node.known_lits.len(),
            });

            for edge in &node.children {
                json_links.push(SolveTreeJsonLink {
                    source: format!("{:016x}", node.state_hash),
                    target: format!("{:016x}", edge.target_hash),
                    mus_size: edge.mus.len(),
                    deduced_count: edge.deduced_lits.len(),
                    description: edge.description.clone(),
                    merged_count: edge.merged.as_ref().map(|m| m.len()),
                    merged: edge.merged.clone(),
                });
            }
        }

        let terminal_nodes = self.nodes.values().filter(|n| n.is_terminal).count();
        let max_depth = self.nodes.values().map(|n| n.depth).max().unwrap_or(0);
        let total_edges = json_links.len();
        let merged_edges = json_links
            .iter()
            .filter(|l| l.merged_count.is_some())
            .count();

        SolveTreeJson {
            nodes: json_nodes,
            links: json_links,
            stats: SolveTreeJsonStats {
                total_nodes: self.nodes.len(),
                total_edges,
                max_depth,
                terminal_nodes,
                merged_edges,
                lattice_hits: self.lattice_hits,
                root_id: format!("{:016x}", self.root_hash),
            },
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_individual_edges(
    indices: &[usize],
    candidates: &[ChildCandidate],
    nodes: &HashMap<StateHash, SolveTreeNode>,
    lattice_cache: &[LatticeEntry],
    lattice_hits: &mut usize,
    stack: &mut Vec<(Vec<Lit>, StateHash, usize)>,
    depth: usize,
    edges: &mut Vec<SolveTreeEdge>,
) {
    for &i in indices {
        let c = &candidates[i];
        let target = match is_already_covered(c.child_hash, &c.child_known, nodes, lattice_cache) {
            Some(redirect) => {
                *lattice_hits += usize::from(redirect != c.child_hash);
                redirect
            }
            None => {
                stack.push((c.child_known.clone(), c.child_hash, depth + 1));
                c.child_hash
            }
        };
        edges.push(SolveTreeEdge {
            mus: c.mus_ctx.mus.clone(),
            deduced_lits: c.mus_ctx.lits.clone(),
            description: c.description.clone(),
            target_hash: target,
            merged: None,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_merged_edge_and_register(
    indices: &[usize],
    candidates: &[ChildCandidate],
    parent_known: &[Lit],
    nodes: &HashMap<StateHash, SolveTreeNode>,
    lattice_cache: &mut Vec<LatticeEntry>,
    lattice_hits: &mut usize,
    stack: &mut Vec<(Vec<Lit>, StateHash, usize)>,
    depth: usize,
    edges: &mut Vec<SolveTreeEdge>,
) -> anyhow::Result<()> {
    let merged_edge = make_merged_edge(indices, candidates, parent_known);

    let combined = combined_known(parent_known, indices, candidates);
    let target = match is_already_covered(merged_edge.target_hash, &combined, nodes, lattice_cache)
    {
        Some(redirect) => {
            *lattice_hits += usize::from(redirect != merged_edge.target_hash);
            redirect
        }
        None => {
            stack.push((combined, merged_edge.target_hash, depth + 1));
            merged_edge.target_hash
        }
    };

    let lower: BTreeSet<Lit> = parent_known.iter().copied().collect();
    let mut delta = BTreeSet::new();
    for &i in indices {
        delta.extend(&candidates[i].mus_ctx.lits);
    }
    let lower_size = lower.len();
    let upper_size = lower_size + delta.difference(&lower).count();
    lattice_cache.push(LatticeEntry {
        lower,
        delta,
        lower_size,
        upper_size,
        upper_hash: merged_edge.target_hash,
    });

    edges.push(SolveTreeEdge {
        target_hash: target,
        ..merged_edge
    });

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn self_merge_all_and_register(
    indices: &[usize],
    candidates: &[ChildCandidate],
    parent_known: &[Lit],
    nodes: &HashMap<StateHash, SolveTreeNode>,
    lattice_cache: &mut Vec<LatticeEntry>,
    lattice_hits: &mut usize,
    stack: &mut Vec<(Vec<Lit>, StateHash, usize)>,
    depth: usize,
) -> anyhow::Result<Vec<SolveTreeEdge>> {
    let mut edges = Vec::new();
    if indices.len() <= 1 {
        push_individual_edges(
            indices,
            candidates,
            nodes,
            lattice_cache,
            lattice_hits,
            stack,
            depth,
            &mut edges,
        );
    } else {
        push_merged_edge_and_register(
            indices,
            candidates,
            parent_known,
            nodes,
            lattice_cache,
            lattice_hits,
            stack,
            depth,
            &mut edges,
        )?;
    }
    Ok(edges)
}

fn make_merged_edge(
    indices: &[usize],
    candidates: &[ChildCandidate],
    parent_known: &[Lit],
) -> SolveTreeEdge {
    let combined = combined_known(parent_known, indices, candidates);
    let combined_hash = hash_lits(&combined);

    let mut all_deduced = BTreeSet::new();
    let mut merged_info = Vec::new();

    for &i in indices {
        let c = &candidates[i];
        all_deduced.extend(&c.mus_ctx.lits);
        merged_info.push(MergedMusInfo {
            description: c.description.clone(),
            deduced_count: c.mus_ctx.lits.len(),
            known_lits_after: c.child_known.len(),
        });
    }

    let description = format!(
        "Merged {} non-progressing MUSes ({} total deductions)",
        indices.len(),
        all_deduced.len()
    );

    SolveTreeEdge {
        mus: BTreeSet::new(),
        deduced_lits: all_deduced,
        description,
        target_hash: combined_hash,
        merged: Some(merged_info),
    }
}

#[derive(Serialize)]
pub struct SolveTreeJson {
    pub nodes: Vec<SolveTreeJsonNode>,
    pub links: Vec<SolveTreeJsonLink>,
    pub stats: SolveTreeJsonStats,
}

#[derive(Serialize)]
pub struct SolveTreeJsonNode {
    pub id: String,
    pub depth: usize,
    pub remaining: usize,
    pub min_mus_count: usize,
    pub min_mus_size: usize,
    pub is_terminal: bool,
    pub known_lits_count: usize,
}

#[derive(Serialize, Clone)]
pub struct SolveTreeJsonLink {
    pub source: String,
    pub target: String,
    pub mus_size: usize,
    pub deduced_count: usize,
    pub description: String,
    pub merged_count: Option<usize>,
    pub merged: Option<Vec<MergedMusInfo>>,
}

#[derive(Serialize)]
pub struct SolveTreeJsonStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub max_depth: usize,
    pub terminal_nodes: usize,
    pub merged_edges: usize,
    pub lattice_hits: usize,
    pub root_id: String,
}
