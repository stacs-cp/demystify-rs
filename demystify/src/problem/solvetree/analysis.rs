//! Research solve graphs. Search only for the easiest moves, then push the
//! explanations discovered there through earlier and alternative states.
//!
//! Evidence is monotone; the *active* easiest-move graph need not be. Superseded
//! edges and unreachable states remain in the archive. Canonical fact vectors,
//! rather than hashes alone, identify states. No size-1/lattice merging is used.
//!
//! A fixed point means every discovered explanation has been checked at every
//! retained state and every current easiest move explored. It does not certify
//! that randomized MUS discovery found all explanations or the true minimum.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use rustsat::types::Lit;
use serde::{Deserialize, Serialize};

use super::{SolveTreeJson, SolveTreeJsonLink, SolveTreeJsonNode, SolveTreeJsonStats};
use crate::problem::json_file;
use crate::problem::musdict::MusDict;
use crate::problem::parse::PuzzleParse;
use crate::problem::serialize::SerializablePuzzleParse;
use crate::problem::solver::{MusConfig, PuzzleSolver, SolverConfig};

pub type NodeId = usize;
pub type ProofId = usize;
const FORMAT: &str = "demystify-solve-graph";
const VERSION: u32 = 1;

#[derive(Clone, Serialize, Deserialize)]
pub struct SearchSettings {
    pub mus: MusConfig,
    /// Recorded here; callers must set the process-wide SAT conflict limit.
    pub conflict_limit: i64,
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            mus: MusConfig::new_with_repeats(5),
            conflict_limit: 1000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Pending,
    Open,
    Solved,
    /// No further deduction exists, but unresolved candidates remain.
    Underdetermined,
    /// Deductions exist but budgeted discovery has not found an explanation.
    SearchIncomplete,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Sorted, deduplicated signed DIMACS literals, including structural closure.
    pub known: Vec<i32>,
    pub remaining: Vec<i32>,
    pub unresolved_candidates: usize,
    pub status: NodeStatus,
    /// Proof id -> deductions justified here. Includes non-minimum explanations
    /// so their availability is not lost when a smaller explanation is found.
    pub evidence: BTreeMap<ProofId, Vec<i32>>,
    pub edges: Vec<GraphEdge>,
    pub searched_round: usize,
    /// All proof ids below this cursor have been checked (including failures).
    pub checked_proofs: usize,
    pub needs_expansion: bool,
}

impl GraphNode {
    pub fn min_difficulty(&self, proofs: &[GraphProof]) -> Option<usize> {
        self.evidence
            .keys()
            .map(|&p| proofs[p].constraints.len())
            .min()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub target: NodeId,
    pub proof: ProofId,
    pub deduced: Vec<i32>,
    /// Automatic assignments, eliminations and reveals after the explicit move.
    pub implied: Vec<i32>,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphProof {
    pub constraints: Vec<i32>,
    pub discovered_at: NodeId,
    /// None for search discoveries; otherwise the proof reduced at this state.
    pub reduced_from: Option<ProofId>,
}

/// Self-contained, versioned archive. Node/proof ids are indices into the vectors.
/// The embedded parse preserves constraint identities, human-readable names,
/// puzzle rendering metadata, and CNF for later searches without Conjure.
#[derive(Clone, Serialize, Deserialize)]
pub struct SolveGraph {
    pub format: String,
    pub version: u32,
    pub producer_version: String,
    pub source: String,
    pub puzzle: SerializablePuzzleParse,
    pub solver_config: SolverConfig,
    pub searches: Vec<SearchSettings>,
    pub root: NodeId,
    pub nodes: Vec<GraphNode>,
    pub proofs: Vec<GraphProof>,
    pub fixed_point: bool,
    pub propagation_checks: usize,
}

fn encode(lits: impl IntoIterator<Item = Lit>) -> Vec<i32> {
    let mut values: Vec<_> = lits.into_iter().map(|l| l.to_ipasir()).collect();
    values.sort_unstable();
    values.dedup();
    values
}

fn decode(values: &[i32]) -> Result<Vec<Lit>> {
    values
        .iter()
        .map(|&v| Lit::from_ipasir(v).context("Invalid graph literal"))
        .collect()
}

/// Structural deductions cost zero. Include their closure in the state key,
/// including any additional reveals, so equivalent move orders share a node.
fn normalise(solver: &mut PuzzleSolver) {
    loop {
        let trivial: Vec<_> = solver
            .get_provable_varlits()
            .clone()
            .into_iter()
            .filter(|&lit| solver.explanation_proves(lit, &[]))
            .collect();
        if trivial.is_empty() {
            break;
        }
        for lit in trivial {
            solver.add_known_lit_unchecked(lit);
        }
    }
}

impl SolveGraph {
    /// The supplied solver may already contain pinned givens or a game state.
    pub fn new(
        mut solver: PuzzleSolver,
        source: String,
        mut settings: SearchSettings,
    ) -> Result<Self> {
        ensure!(settings.mus.repeats > 0, "MUS repeats must be positive");
        settings.mus.find_one = false;
        settings.mus.find_bigger = false;
        settings.mus.keep_all_muses = true;
        ensure!(
            solver.is_currently_solvable(),
            "Cannot explore an inconsistent puzzle"
        );
        normalise(&mut solver);
        let mut graph = Self {
            format: FORMAT.into(),
            version: VERSION,
            producer_version: env!("CARGO_PKG_VERSION").into(),
            source,
            puzzle: SerializablePuzzleParse::try_from(solver.puzzleparse())?,
            solver_config: solver.solver_config(),
            searches: vec![settings],
            root: 0,
            nodes: vec![],
            proofs: vec![],
            fixed_point: false,
            propagation_checks: 0,
        };
        graph.nodes.push(Self::node_from_solver(&mut solver));
        Ok(graph)
    }

    fn node_from_solver(solver: &mut PuzzleSolver) -> GraphNode {
        let known = encode(solver.get_known_lits().iter().copied());
        let remaining = encode(solver.get_provable_varlits().iter().copied());
        let unresolved_candidates = solver.get_literals_to_try_solving().len();
        GraphNode {
            known,
            remaining,
            unresolved_candidates,
            status: NodeStatus::Pending,
            evidence: BTreeMap::new(),
            edges: vec![],
            searched_round: 0,
            checked_proofs: 0,
            needs_expansion: true,
        }
    }

    /// Request another randomized discovery pass at each retained state. Existing
    /// evidence is never discarded. Resume without this call to finish a checkpoint.
    pub fn search_more(&mut self, mut settings: SearchSettings) -> Result<()> {
        ensure!(settings.mus.repeats > 0, "MUS repeats must be positive");
        settings.mus.find_one = false;
        settings.mus.find_bigger = false;
        settings.mus.keep_all_muses = true;
        self.searches.push(settings);
        self.fixed_point = false;
        Ok(())
    }

    /// Run exploration and upward/downward propagation until stable. The callback
    /// sees consistent checkpoints after each unit of work and can save periodically.
    /// Returning an error from it interrupts the run without losing completed work.
    pub fn run(&mut self, mut checkpoint: impl FnMut(&Self) -> Result<()>) -> Result<()> {
        self.validate()?;
        if self.fixed_point {
            return Ok(());
        }
        let puzzle = Arc::new(PuzzleParse::try_from(self.puzzle.clone())?);
        let mut engine = Engine {
            states: self
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.known.clone(), i))
                .collect(),
            proofs: self
                .proofs
                .iter()
                .enumerate()
                .map(|(i, p)| (p.constraints.clone(), i))
                .collect(),
            puzzle,
        };
        loop {
            // Explore all currently known easiest routes first. Appended nodes are
            // visited in this loop; each gets one bounded search per requested round.
            let mut id = 0;
            while id < self.nodes.len() {
                if self.nodes[id].searched_round < self.searches.len() {
                    engine.search(self, id)?;
                }
                if self.nodes[id].needs_expansion {
                    engine.expand(self, id)?;
                }
                checkpoint(self)?;
                id += 1;
            }

            // Up, then down. Checking all discovered proofs at each affected state
            // also transfers proofs between branches without requiring validity at
            // their common ancestor. Each (state, proof) pair is checked only once.
            let mut order: Vec<_> = (0..self.nodes.len()).collect();
            order.sort_by_key(|&i| (self.nodes[i].known.len(), i));
            for upward in [true, false] {
                if upward {
                    order.reverse();
                } else {
                    order.sort_by_key(|&i| (self.nodes[i].known.len(), i));
                }
                for &id in &order {
                    if self.nodes[id].checked_proofs < self.proofs.len() {
                        engine.propagate(self, id)?;
                        checkpoint(self)?;
                    }
                }
            }
            if self.nodes.iter().all(|n| {
                !n.needs_expansion
                    && n.searched_round == self.searches.len()
                    && n.checked_proofs == self.proofs.len()
            }) {
                self.fixed_point = true;
                checkpoint(self)?;
                return Ok(());
            }
        }
    }

    /// Structural validation on load; never trusts indices from a saved file.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.format == FORMAT && self.version == VERSION,
            "Unsupported solve graph format/version"
        );
        ensure!(self.root < self.nodes.len(), "Missing graph root");
        ensure!(!self.searches.is_empty(), "Missing search settings");
        ensure!(
            self.searches.iter().all(|s| s.mus.repeats > 0),
            "Invalid search repeat count"
        );
        let canonical = |v: &[i32]| -> bool {
            v.windows(2).all(|w| w[0] < w[1]) && v.iter().all(|&l| Lit::from_ipasir(l).is_ok())
        };
        let mut keys = BTreeSet::new();
        for proof in &self.proofs {
            ensure!(
                canonical(&proof.constraints) && !proof.constraints.is_empty(),
                "Invalid proof constraints"
            );
            ensure!(
                proof
                    .constraints
                    .iter()
                    .all(|l| self.puzzle.conset_lits.contains(l)),
                "Unknown proof constraint"
            );
            ensure!(
                proof.discovered_at < self.nodes.len(),
                "Invalid discovery state"
            );
            ensure!(
                proof.reduced_from.is_none_or(|p| p < self.proofs.len()),
                "Invalid reduction source"
            );
            ensure!(keys.insert(&proof.constraints), "Duplicate proof");
        }
        let mut states = BTreeSet::new();
        for (id, node) in self.nodes.iter().enumerate() {
            ensure!(
                canonical(&node.known) && canonical(&node.remaining),
                "Noncanonical state literals"
            );
            ensure!(states.insert(&node.known), "Duplicate graph state");
            ensure!(
                node.known.iter().all(|l| !node.known.contains(&-l)),
                "Contradictory state"
            );
            ensure!(
                node.remaining
                    .iter()
                    .all(|l| !node.known.contains(l) && !node.known.contains(&-l)),
                "Known deduction listed as remaining"
            );
            ensure!(
                node.checked_proofs <= self.proofs.len()
                    && node.searched_round <= self.searches.len(),
                "Invalid work cursor"
            );
            for (&p, lits) in &node.evidence {
                ensure!(
                    p < self.proofs.len() && canonical(lits) && !lits.is_empty(),
                    "Invalid node evidence"
                );
                ensure!(
                    lits.iter().all(|l| node.remaining.contains(l)),
                    "Evidence targets a nonremaining literal"
                );
            }
            for edge in &node.edges {
                ensure!(
                    edge.target < self.nodes.len() && edge.proof < self.proofs.len(),
                    "Invalid edge reference"
                );
                let target = &self.nodes[edge.target].known;
                ensure!(
                    target.len() > node.known.len()
                        && node.known.iter().all(|l| target.contains(l)),
                    "Edge does not strictly add knowledge at state {id}"
                );
                ensure!(
                    canonical(&edge.deduced)
                        && !edge.deduced.is_empty()
                        && canonical(&edge.implied),
                    "Invalid edge deductions"
                );
                ensure!(
                    edge.deduced.iter().all(|l| node
                        .evidence
                        .get(&edge.proof)
                        .is_some_and(|v| v.contains(l))),
                    "Edge lacks supporting evidence"
                );
                let mut expected = node.known.clone();
                expected.extend(&edge.deduced);
                expected.extend(&edge.implied);
                expected.sort_unstable();
                expected.dedup();
                ensure!(&expected == target, "Edge facts do not match its target");
            }
        }
        if self.fixed_point {
            ensure!(
                self.nodes.iter().all(|n| !n.needs_expansion
                    && n.searched_round == self.searches.len()
                    && n.checked_proofs == self.proofs.len()),
                "Fixed point has unfinished work"
            );
        }
        Ok(())
    }

    /// Atomic replacement: a failed/interrupted write leaves the previous checkpoint.
    pub fn save(&self, path: &Path) -> Result<()> {
        json_file::write_atomic(path, self, false).context("Replacing solve graph checkpoint")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let graph: Self = json_file::read(path)?;
        graph.validate()?;
        Ok(graph)
    }

    /// Shortest distance from the root using the current easiest moves. None
    /// denotes retained evidence outside the current active graph, not lost data.
    pub fn depths(&self) -> Vec<Option<usize>> {
        let mut depths = vec![None; self.nodes.len()];
        depths[self.root] = Some(0);
        let mut queue = VecDeque::from([self.root]);
        while let Some(id) = queue.pop_front() {
            for edge in self.nodes[id].edges.iter().filter(|e| e.active) {
                if depths[edge.target].is_none() {
                    depths[edge.target] = Some(depths[id].unwrap() + 1);
                    queue.push_back(edge.target);
                }
            }
        }
        depths
    }

    /// Branch-specific beginnings of a proof's availability for one deduction,
    /// within the active graph. No single depth can describe a shared-state DAG.
    pub fn availability_frontier(&self, proof: ProofId, deduction: i32) -> Vec<NodeId> {
        let depths = self.depths();
        let available: BTreeSet<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                (depths[i].is_some()
                    && n.evidence
                        .get(&proof)
                        .is_some_and(|ls| ls.contains(&deduction)))
                .then_some(i)
            })
            .collect();
        let mut starts = available.clone();
        for &id in &available {
            for edge in self.nodes[id].edges.iter().filter(|e| e.active) {
                starts.remove(&edge.target);
            }
        }
        starts.into_iter().collect()
    }

    pub fn literal_label(&self, lit: i32) -> String {
        self.puzzle
            .invlitmap
            .get(&lit.to_string())
            .map(|ls| {
                ls.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| lit.to_string())
    }

    pub fn proof_label(&self, proof: ProofId) -> String {
        self.proofs[proof]
            .constraints
            .iter()
            .map(|c| {
                self.puzzle
                    .conset
                    .get(&c.to_string())
                    .cloned()
                    .unwrap_or_else(|| c.to_string())
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Portable viewer with the archive embedded; no network or server required.
    pub fn to_html(&self) -> Result<String> {
        self.validate()?;
        // JSON is embedded in a script element. Escape '<' so puzzle descriptions
        // cannot terminate that element, even in an application/json script.
        let data = serde_json::to_string(self)?.replace('<', "\\u003c");
        Ok(include_str!("viewer.html").replace("@@GRAPH@@", &data))
    }

    /// Compatible with the existing D3 solve-tree explorer; export only active
    /// reachable routes. The archive separately retains all historical evidence.
    pub fn to_d3_json(&self) -> SolveTreeJson {
        let depths = self.depths();
        let mut nodes = vec![];
        let mut links = vec![];
        for (id, n) in self.nodes.iter().enumerate() {
            let Some(depth) = depths[id] else {
                continue;
            };
            let minimum = n.min_difficulty(&self.proofs);
            nodes.push(SolveTreeJsonNode {
                id: id.to_string(),
                depth,
                remaining: n.remaining.len(),
                min_mus_count: n
                    .evidence
                    .keys()
                    .filter(|&&p| Some(self.proofs[p].constraints.len()) == minimum)
                    .count(),
                min_mus_size: minimum.unwrap_or(0),
                is_terminal: n.edges.iter().all(|e| !e.active),
                known_lits_count: n.known.len(),
            });
            for e in n.edges.iter().filter(|e| e.active) {
                links.push(SolveTreeJsonLink {
                    source: id.to_string(),
                    target: e.target.to_string(),
                    mus_size: self.proofs[e.proof].constraints.len(),
                    deduced_count: e.deduced.len(),
                    description: format!(
                        "{} because {}",
                        e.deduced
                            .iter()
                            .map(|&l| self.literal_label(l))
                            .collect::<Vec<_>>()
                            .join(", "),
                        self.proof_label(e.proof)
                    ),
                    merged_count: None,
                    merged: None,
                });
            }
        }
        let stats = SolveTreeJsonStats {
            total_nodes: nodes.len(),
            total_edges: links.len(),
            max_depth: depths.iter().flatten().copied().max().unwrap_or(0),
            terminal_nodes: nodes.iter().filter(|n| n.is_terminal).count(),
            merged_edges: 0,
            lattice_hits: 0,
            root_id: self.root.to_string(),
        };
        SolveTreeJson {
            nodes,
            links,
            stats,
        }
    }
}

struct Engine {
    puzzle: Arc<PuzzleParse>,
    states: HashMap<Vec<i32>, NodeId>,
    proofs: HashMap<Vec<i32>, ProofId>,
}

impl Engine {
    fn solver(&self, graph: &SolveGraph, id: NodeId) -> Result<PuzzleSolver> {
        PuzzleSolver::fork_with_known_lits(
            self.puzzle.clone(),
            &decode(&graph.nodes[id].known)?,
            graph.solver_config,
        )
    }

    fn proof(
        &mut self,
        graph: &mut SolveGraph,
        constraints: Vec<i32>,
        id: NodeId,
        reduced_from: Option<ProofId>,
    ) -> ProofId {
        if let Some(&p) = self.proofs.get(&constraints) {
            return p;
        }
        let p = graph.proofs.len();
        self.proofs.insert(constraints.clone(), p);
        graph.proofs.push(GraphProof {
            constraints,
            discovered_at: id,
            reduced_from,
        });
        graph.fixed_point = false;
        p
    }

    fn record(graph: &mut SolveGraph, id: NodeId, proof: ProofId, deduced: Vec<i32>) {
        if deduced.is_empty() {
            return;
        }
        let entry = graph.nodes[id].evidence.entry(proof).or_default();
        let old_len = entry.len();
        entry.extend(deduced);
        entry.sort_unstable();
        entry.dedup();
        if entry.len() != old_len {
            graph.nodes[id].needs_expansion = true;
        }
    }

    fn search(&mut self, graph: &mut SolveGraph, id: NodeId) -> Result<()> {
        let solver = self.solver(graph, id)?;
        let remaining: BTreeSet<_> = decode(&graph.nodes[id].remaining)?.into_iter().collect();
        if !remaining.is_empty() {
            let mut config = graph.searches.last().unwrap().mus;
            config.find_one = false;
            config.find_bigger = false;
            config.keep_all_muses = true;
            let mut cache = MusDict::with_keep_all(true);
            for (&p, lits) in &graph.nodes[id].evidence {
                let cons: BTreeSet<_> = decode(&graph.proofs[p].constraints)?.into_iter().collect();
                for lit in decode(lits)? {
                    cache.add_mus(lit, cons.clone());
                }
            }
            let found = solver.get_many_vars_small_mus_quick(&remaining, &config, Some(cache));
            // Retain all alternative witnesses, not just the first per literal.
            for (&lit, muses) in found.muses() {
                for mus in muses {
                    let constraints: Vec<_> = mus.mus.iter().copied().collect();
                    if let Some(reduced) = solver.reduce_explanation(lit, &constraints) {
                        ensure!(
                            !reduced.is_empty(),
                            "Structural closure missed a trivial deduction"
                        );
                        let p = self.proof(graph, encode(reduced), id, None);
                        Self::record(graph, id, p, vec![lit.to_ipasir()]);
                    }
                }
            }
        }
        graph.nodes[id].searched_round = graph.searches.len();
        graph.nodes[id].needs_expansion = true;
        Ok(())
    }

    fn propagate(&mut self, graph: &mut SolveGraph, id: NodeId) -> Result<()> {
        let solver = self.solver(graph, id)?;
        let remaining = decode(&graph.nodes[id].remaining)?;
        while graph.nodes[id].checked_proofs < graph.proofs.len() {
            let p = graph.nodes[id].checked_proofs;
            let constraints = decode(&graph.proofs[p].constraints)?;
            let mut proven = vec![];
            for &lit in &remaining {
                graph.propagation_checks += 1;
                if let Some(reduced) = solver.reduce_explanation(lit, &constraints) {
                    proven.push(lit.to_ipasir());
                    ensure!(
                        !reduced.is_empty(),
                        "Structural closure missed a trivial deduction"
                    );
                    if reduced.len() < constraints.len() {
                        let smaller = self.proof(graph, encode(reduced), id, Some(p));
                        Self::record(graph, id, smaller, vec![lit.to_ipasir()]);
                    }
                }
            }
            Self::record(graph, id, p, proven);
            graph.nodes[id].checked_proofs += 1;
        }
        Ok(())
    }

    fn expand(&mut self, graph: &mut SolveGraph, id: NodeId) -> Result<()> {
        let minimum = graph.nodes[id].min_difficulty(&graph.proofs);
        let choices: Vec<_> = graph.nodes[id]
            .evidence
            .iter()
            .filter(|(p, _)| Some(graph.proofs[**p].constraints.len()) == minimum)
            .map(|(&p, ls)| (p, ls.clone()))
            .collect();
        for edge in &mut graph.nodes[id].edges {
            edge.active = false;
        }
        for (proof, deduced) in choices {
            if let Some(edge) = graph.nodes[id]
                .edges
                .iter_mut()
                .find(|e| e.proof == proof && e.deduced == deduced)
            {
                edge.active = true;
                continue;
            }
            let mut solver = self.solver(graph, id)?;
            for lit in decode(&deduced)? {
                solver.add_known_lit_unchecked(lit);
            }
            normalise(&mut solver);
            let known = encode(solver.get_known_lits().iter().copied());
            let implied = known
                .iter()
                .copied()
                .filter(|l| !graph.nodes[id].known.contains(l) && !deduced.contains(l))
                .collect();
            let target = if let Some(&target) = self.states.get(&known) {
                target
            } else {
                let target = graph.nodes.len();
                self.states.insert(known, target);
                graph.nodes.push(SolveGraph::node_from_solver(&mut solver));
                target
            };
            ensure!(
                graph.nodes[target].known.len() > graph.nodes[id].known.len(),
                "Move failed to add knowledge"
            );
            graph.nodes[id].edges.push(GraphEdge {
                target,
                proof,
                deduced,
                implied,
                active: true,
            });
        }
        let node = &mut graph.nodes[id];
        node.status = if minimum.is_some() {
            NodeStatus::Open
        } else if !node.remaining.is_empty() {
            NodeStatus::SearchIncomplete
        } else if node.unresolved_candidates == 0 {
            NodeStatus::Solved
        } else {
            NodeStatus::Underdetermined
        };
        node.needs_expansion = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problem::{PuzLit, PuzVar, VarValPair};
    use rustsat::{instances::Cnf, types::Clause};

    /// Tiny, explicit gated CNFs avoid external compilers and randomized test
    /// expectations. Puzzle variables are Boolean cells 1..=n; gates are >=10.
    fn fixture(n: i32, clauses: &[&[i32]]) -> Arc<PuzzleParse> {
        let gates: BTreeSet<_> = clauses
            .iter()
            .flat_map(|c| c.iter())
            .map(|l| l.abs())
            .filter(|&l| l >= 10)
            .collect();
        let mut puzzle = PuzzleParse::new_from_eprime(
            BTreeSet::from(["cell".into()]),
            BTreeSet::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            Some("Test".into()),
            vec![],
            BTreeMap::new(),
            vec![],
        );
        for i in 1..=n {
            let var = PuzVar::new("cell", vec![i64::from(i)]);
            puzzle
                .direct
                .domainmap
                .insert(var.clone(), BTreeSet::from([0, 1]));
            for value in [0, 1] {
                let signed = if value == 1 { i } else { -i };
                let lit = Lit::from_ipasir(signed).unwrap();
                let vv = VarValPair::new(&var, value);
                for (puzlit, l) in [
                    (PuzLit::new_eq(vv.clone()), lit),
                    (PuzLit::new_neq(vv), !lit),
                ] {
                    puzzle.direct.litmap.insert(puzlit.clone(), l);
                    puzzle.direct.invlitmap.entry(l).or_default().insert(puzlit);
                }
                puzzle.var_lits.insert_positive(lit);
                puzzle.var_lits.insert_negative(lit);
            }
        }
        for gate in gates {
            let variables = clauses
                .iter()
                .filter(|c| c.contains(&-gate))
                .flat_map(|c| c.iter())
                .map(|l| l.abs())
                .filter(|&l| l <= n)
                .map(|l| Lit::from_ipasir(l).unwrap())
                .collect();
            puzzle
                .constraints
                .insert(
                    Lit::from_ipasir(gate).unwrap(),
                    format!("rule{gate}"),
                    format!("Rule {gate}"),
                    variables,
                )
                .unwrap();
        }
        let mut cnf = Cnf::new();
        for c in clauses {
            cnf.add_clause(Clause::from_iter(decode(c).unwrap()));
        }
        puzzle.cnf = Some(Arc::new(cnf));
        // Rebuild the normal reverse constraint/variable index.
        Arc::new(
            PuzzleParse::try_from(SerializablePuzzleParse::try_from(&puzzle).unwrap()).unwrap(),
        )
    }

    fn graph(puzzle: Arc<PuzzleParse>) -> SolveGraph {
        SolveGraph::new(
            PuzzleSolver::new(puzzle).unwrap(),
            "fixture".into(),
            SearchSettings::default(),
        )
        .unwrap()
    }

    fn engine(g: &SolveGraph) -> Engine {
        Engine {
            puzzle: Arc::new(PuzzleParse::try_from(g.puzzle.clone()).unwrap()),
            states: g
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.known.clone(), i))
                .collect(),
            proofs: g
                .proofs
                .iter()
                .enumerate()
                .map(|(i, p)| (p.constraints.clone(), i))
                .collect(),
        }
    }

    fn seed(
        e: &mut Engine,
        g: &mut SolveGraph,
        state: usize,
        cons: &[i32],
        targets: &[i32],
    ) -> usize {
        let p = e.proof(g, cons.to_vec(), state, None);
        Engine::record(g, state, p, targets.to_vec());
        g.nodes[state].searched_round = g.searches.len();
        p
    }

    fn state(g: &SolveGraph, known: &[i32]) -> usize {
        g.nodes.iter().position(|n| n.known == known).unwrap()
    }

    fn missed_independent_move() -> SolveGraph {
        let mut g = graph(fixture(2, &[&[-10, 1], &[-11, 2]]));
        let mut e = engine(&g);
        seed(&mut e, &mut g, 0, &[10], &[1]);
        e.expand(&mut g, 0).unwrap();
        let child = state(&g, &[1]);
        seed(&mut e, &mut g, child, &[11], &[2]);
        g
    }

    #[test]
    fn late_proof_recovers_root_choice_and_other_route() {
        let mut g = missed_independent_move();
        g.run(|_| Ok(())).unwrap();
        g.validate().unwrap();
        let p = g.proofs.iter().position(|p| p.constraints == [11]).unwrap();
        assert_eq!(g.nodes[0].evidence[&p], [2]);
        assert_eq!(g.availability_frontier(p, 2), [0]);
        assert_eq!(g.nodes[0].edges.iter().filter(|e| e.active).count(), 2);
        let b = state(&g, &[2]);
        assert!(
            g.nodes[b]
                .edges
                .iter()
                .any(|e| e.active && g.nodes[e.target].known == [1, 2])
        );
        assert_eq!(g.depths().iter().flatten().count(), 4);
        assert!(g.fixed_point);
        let before = serde_json::to_value(&g).unwrap();
        g.run(|_| panic!("A fixed graph must not search again"))
            .unwrap();
        assert_eq!(before, serde_json::to_value(&g).unwrap());
    }

    #[test]
    fn parent_check_cannot_borrow_child_facts() {
        let mut g = graph(fixture(2, &[&[-10, 1], &[-11, -1, 2]]));
        let mut e = engine(&g);
        seed(&mut e, &mut g, 0, &[10], &[1]);
        e.expand(&mut g, 0).unwrap();
        let child = state(&g, &[1]);
        let p = seed(&mut e, &mut g, child, &[11], &[2]);
        g.run(|_| Ok(())).unwrap();
        assert!(!g.nodes[0].evidence.contains_key(&p));
        assert_eq!(g.availability_frontier(p, 2), [child]);
        assert_eq!(g.nodes[child].evidence[&p], [2]);
    }

    #[test]
    fn proof_crosses_branches_even_if_invalid_at_common_ancestor() {
        let mut g = graph(fixture(
            3,
            &[&[-10, 1], &[-11, -1, 2], &[-11, -3, 2], &[-12, 3]],
        ));
        let mut e = engine(&g);
        seed(&mut e, &mut g, 0, &[10], &[1]);
        seed(&mut e, &mut g, 0, &[12], &[3]);
        e.expand(&mut g, 0).unwrap();
        let a = state(&g, &[1]);
        let c = state(&g, &[3]);
        let p = seed(&mut e, &mut g, a, &[11], &[2]);
        // Deliberately simulate discovery missing every move in the other branch.
        g.nodes[c].searched_round = 1;
        g.run(|_| Ok(())).unwrap();
        assert!(!g.nodes[0].evidence.contains_key(&p));
        assert_eq!(g.nodes[c].evidence[&p], [2]);
        assert_eq!(g.availability_frontier(p, 2), [a, c]);
        assert!(g.nodes[c].edges.iter().any(|e| e.active && e.proof == p));
    }

    #[test]
    fn reductions_repair_edges_without_discarding_history() {
        let mut g = graph(fixture(2, &[&[-10, 1], &[-11, -1, 2]]));
        let mut e = engine(&g);
        let large = seed(&mut e, &mut g, 0, &[10, 11], &[2]);
        e.expand(&mut g, 0).unwrap();
        let old_target = state(&g, &[2]);
        g.nodes[old_target].searched_round = 1;
        g.run(|_| Ok(())).unwrap();
        g.validate().unwrap();
        assert_eq!(g.nodes[0].min_difficulty(&g.proofs), Some(1));
        assert!(
            g.nodes[0]
                .edges
                .iter()
                .any(|e| e.proof == large && e.target == old_target && !e.active)
        );
        assert!(g.depths()[old_target].is_none());
        assert!(g.nodes[0].evidence.contains_key(&large));
        let a = state(&g, &[1]);
        assert!(
            g.nodes[a]
                .edges
                .iter()
                .any(|e| e.active && g.proofs[e.proof].constraints == [11])
        );
        assert!(
            g.proofs
                .iter()
                .any(|p| p.constraints == [10] && p.reduced_from == Some(large))
        );
    }

    #[test]
    fn equal_sized_alternatives_are_retained_with_a_shared_target() {
        let mut g = graph(fixture(2, &[&[-10, 1], &[-11, 2], &[-12, 2]]));
        let mut e = engine(&g);
        seed(&mut e, &mut g, 0, &[10], &[1]);
        e.expand(&mut g, 0).unwrap();
        let a = state(&g, &[1]);
        let p = seed(&mut e, &mut g, a, &[11], &[2]);
        let q = seed(&mut e, &mut g, a, &[12], &[2]);
        g.run(|_| Ok(())).unwrap();
        assert_eq!(g.availability_frontier(p, 2), [0]);
        assert_eq!(g.availability_frontier(q, 2), [0]);
        let edges: Vec<_> = g.nodes[0]
            .edges
            .iter()
            .filter(|e| e.active && e.deduced == [2])
            .collect();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].target, edges[1].target);
    }

    #[test]
    fn checkpoint_roundtrip_resume_and_more_search() {
        let mut uninterrupted = missed_independent_move();
        let mut interrupted = uninterrupted.clone();
        uninterrupted.run(|_| Ok(())).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.json");
        let mut calls = 0;
        let result = interrupted.run(|g| {
            calls += 1;
            if calls == 2 {
                g.save(&path)?;
                anyhow::bail!("simulated interruption");
            }
            Ok(())
        });
        assert!(result.is_err());
        let mut loaded = SolveGraph::load(&path).unwrap();
        assert!(!loaded.fixed_point);
        loaded.run(|_| Ok(())).unwrap();
        assert_eq!(
            serde_json::to_value(&uninterrupted).unwrap(),
            serde_json::to_value(&loaded).unwrap()
        );
        let proofs = loaded.proofs.clone();
        loaded.search_more(SearchSettings::default()).unwrap();
        loaded.run(|_| Ok(())).unwrap();
        assert!(loaded.fixed_point);
        assert_eq!(loaded.searches.len(), 2);
        assert_eq!(&loaded.proofs[..proofs.len()], &proofs);
        loaded.save(&path).unwrap();
        assert!(SolveGraph::load(&path).unwrap().fixed_point);
    }

    #[test]
    fn compressed_checkpoint_roundtrip() {
        let graph = missed_independent_move();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("graph.json.zst");

        graph.save(&path).unwrap();
        let loaded = SolveGraph::load(&path).unwrap();

        assert_eq!(
            serde_json::to_value(graph).unwrap(),
            serde_json::to_value(loaded).unwrap()
        );
    }

    #[test]
    fn incomplete_search_is_not_a_solved_leaf_and_ambiguity_is_distinct() {
        let mut g = graph(fixture(1, &[&[-10, 1]]));
        // Simulate a budgeted search returning no explanation.
        g.nodes[0].searched_round = 1;
        g.run(|_| Ok(())).unwrap();
        assert!(g.fixed_point);
        assert_eq!(g.nodes[0].status, NodeStatus::SearchIncomplete);
        let mut ambiguous = graph(fixture(1, &[&[1, -1]]));
        ambiguous.run(|_| Ok(())).unwrap();
        assert_eq!(ambiguous.nodes[0].status, NodeStatus::Underdetermined);
        let mut solved = graph(fixture(1, &[&[1]]));
        solved.run(|_| Ok(())).unwrap();
        assert_eq!(solved.nodes[0].status, NodeStatus::Solved);
        assert_eq!(solved.nodes[0].known, [1]);
        assert!(
            SolveGraph::new(
                PuzzleSolver::new(fixture(1, &[&[1], &[-1]])).unwrap(),
                "bad".into(),
                SearchSettings::default()
            )
            .is_err()
        );
    }

    #[test]
    fn invalid_archives_are_rejected_and_html_cannot_inject_script() {
        let mut g = missed_independent_move();
        g.run(|_| Ok(())).unwrap();
        let mut bad = g.clone();
        bad.version = 99;
        assert!(bad.validate().is_err());
        let mut bad = g.clone();
        bad.nodes[0].edges[0].target = usize::MAX;
        assert!(bad.validate().is_err());
        let mut bad = g.clone();
        bad.nodes[0].known.push(0);
        assert!(bad.validate().is_err());
        g.source = "</script><script>alert('x')</script>".into();
        let html = g.to_html().unwrap();
        assert!(!html.contains(&g.source));
        assert!(html.contains("\\u003c/script>"));
    }
}
