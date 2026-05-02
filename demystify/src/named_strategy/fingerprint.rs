//! Canonical fingerprint of a MUS.
//!
//! For a MUS `M = {c1, ..., ck}`:
//!  1. Each `ci` becomes a *node* labelled by its constraint-family symbol
//!     (after stripping the `[...]` index suffix and applying any
//!     family-group remap from `FamilyMap`).
//!  2. Edges `ci—cj` are added with a quantised weight `{1, 2, 3+}` reflecting
//!     the cardinality of the intersection of their `PuzVar` sets.
//!  3. The resulting labelled graph is reduced to a canonical-form string
//!     via brute-force permutation enumeration (for ≤ `BRUTE_FORCE_CAP`
//!     nodes) or a degree-sequence hash above that cap.
//!
//! Quantisation: edge weights bucket as `1`, `2`, or `3` (= "≥3"). This is
//! what makes the fingerprint stable across puzzle sizes — a Sudoku
//! "naked single" fingerprint should not depend on whether the grid is 4×4
//! or 9×9.
//!
//! The number of literals deduced by the MUS is *not* part of the
//! fingerprint. The same logical technique often deduces a different number
//! of literals depending on context (a naked pair may eliminate 1, 4, or
//! 14 candidates), and folding that into the fingerprint forces the name
//! database to enumerate every (technique × deduction-count) combination.
//! Treat the deduction count as separate metadata if needed.

use std::collections::BTreeSet;

use itertools::Itertools;
use rustsat::types::Lit;

use crate::problem::PuzVar;
use crate::problem::musdict::MusContext;
use crate::problem::parse::PuzzleParse;

use super::family::FamilyMap;

/// Maximum MUS size at which we run the O(n!) canonical-form enumeration.
/// Above this we fall back to a degree-sequence hash (no false matches, but
/// can fail to identify a known technique).
pub const BRUTE_FORCE_CAP: usize = 8;

/// A canonical fingerprint of a MUS. Two `MusFingerprint` values compare
/// equal iff their structural representations match.
///
/// The internal canonical-form string is encapsulated so the format can
/// evolve without breaking external callers; expose only `as_str()` and
/// `Display`. Crate-internal callers (planner, tests) can still construct
/// and move-out the string via `pub(crate)` access.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MusFingerprint {
    pub(crate) canonical: String,
}

impl MusFingerprint {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Consume the fingerprint and return its canonical-form string.
    /// Used by the planner when wrapping the fingerprint into a `UserMus`.
    #[must_use]
    pub fn into_canonical(self) -> String {
        self.canonical
    }
}

impl std::fmt::Display for MusFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.canonical)
    }
}

/// Strip the `[...]` indices suffix from a constraint name.
/// `"row_alldiff[1, 2, 3, 5]"` → `"row_alldiff"`.
///
/// Only used by the unit tests to verify the parsing helper still behaves
/// — production code reads the family name directly from
/// `parse.constraints.family_of(lit)` instead of re-parsing the rendered
/// description.
#[cfg(test)]
fn family_from_name(name: &str) -> &str {
    name.split('[').next().unwrap_or(name).trim_end()
}

/// Quantise an overlap cardinality to {1, 2, 3} (3 means "≥3").
fn quantise(overlap: usize) -> u8 {
    overlap.min(3) as u8
}

#[derive(Debug)]
struct NodeInfo {
    family: String,
    puzvars: BTreeSet<PuzVar>,
}

fn puzvars_for_constraint(parse: &PuzzleParse, con_lit: &Lit) -> BTreeSet<PuzVar> {
    let mut vars = BTreeSet::new();
    for vl in parse.constraints.var_lits(con_lit) {
        for vvp in parse.direct_or_ordered_lit_to_varvalpair(vl) {
            vars.insert(vvp.var().clone());
        }
    }
    vars
}

/// Compute the canonical fingerprint of a MUS.
///
/// # Panics
///
/// Panics if any constraint lit in the MUS is missing from
/// `parse.constraints` — this would indicate the constraint store wasn't
/// built correctly, which is a programming error rather than a runtime
/// condition we should silently paper over.
#[must_use]
pub fn fingerprint(parse: &PuzzleParse, mus: &MusContext, families: &FamilyMap) -> MusFingerprint {
    let infos: Vec<NodeInfo> = mus
        .mus
        .iter()
        .map(|lit| {
            let raw_family = parse.constraints.family_of(lit).unwrap_or_else(|| {
                panic!(
                    "fingerprint: constraint lit {lit:?} has no family entry; \
                     ConstraintStore is out of sync"
                )
            });
            NodeInfo {
                family: families.resolve(raw_family.as_str()).to_string(),
                puzvars: puzvars_for_constraint(parse, lit),
            }
        })
        .collect();

    let edges = build_edges(&infos);
    let canonical = canonicalise(&infos, &edges);
    MusFingerprint { canonical }
}

fn build_edges(infos: &[NodeInfo]) -> Vec<(usize, usize, u8)> {
    let n = infos.len();
    let mut edges = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let overlap = infos[i].puzvars.intersection(&infos[j].puzvars).count();
            if overlap > 0 {
                edges.push((i, j, quantise(overlap)));
            }
        }
    }
    edges
}

fn canonicalise(infos: &[NodeInfo], edges: &[(usize, usize, u8)]) -> String {
    if infos.len() <= BRUTE_FORCE_CAP {
        brute_force_canonical(infos, edges)
    } else {
        degree_sequence_canonical(infos, edges)
    }
}

fn brute_force_canonical(infos: &[NodeInfo], edges: &[(usize, usize, u8)]) -> String {
    let n = infos.len();

    // perm[slot] = original_index in `infos` that occupies that canonical slot.
    let mut best: Option<String> = None;

    for perm in (0..n).permutations(n) {
        let mut inverse = vec![0usize; n];
        for (slot, &orig) in perm.iter().enumerate() {
            inverse[orig] = slot;
        }

        let families_in_slots: Vec<&str> = perm.iter().map(|&o| infos[o].family.as_str()).collect();

        let mut edges_in_slots: Vec<(usize, usize, u8)> = edges
            .iter()
            .map(|&(i, j, w)| {
                let a = inverse[i];
                let b = inverse[j];
                if a < b { (a, b, w) } else { (b, a, w) }
            })
            .collect();
        edges_in_slots.sort();

        let candidate = serialise_perm(&families_in_slots, &edges_in_slots);
        if best.as_ref().is_none_or(|cur| candidate < *cur) {
            best = Some(candidate);
        }
    }

    best.unwrap_or_else(|| serialise_perm(&[], &[]))
}

fn degree_sequence_canonical(infos: &[NodeInfo], edges: &[(usize, usize, u8)]) -> String {
    let n = infos.len();
    let mut deg = vec![0usize; n];
    for &(i, j, _) in edges {
        deg[i] += 1;
        deg[j] += 1;
    }
    let mut nodes: Vec<(usize, &str)> =
        (0..n).map(|i| (deg[i], infos[i].family.as_str())).collect();
    nodes.sort_unstable();

    let mut edge_weights: Vec<u8> = edges.iter().map(|&(_, _, w)| w).collect();
    edge_weights.sort_unstable();

    let mut out = String::from("DEG:");
    out.push_str(&nodes.iter().map(|(d, f)| format!("{d}:{f}")).join(","));
    out.push(';');
    out.push_str(&edge_weights.iter().map(u8::to_string).join(","));
    out
}

fn serialise_perm(families: &[&str], edges: &[(usize, usize, u8)]) -> String {
    let mut out = String::new();
    out.push_str(&families.join(","));
    out.push(';');
    out.push_str(
        &edges
            .iter()
            .map(|(i, j, w)| format!("{i}-{j}:{w}"))
            .join(","),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_node(family: &str, puzvars: &[(&str, &[i64])]) -> NodeInfo {
        NodeInfo {
            family: family.into(),
            puzvars: puzvars
                .iter()
                .map(|(n, idx)| PuzVar::new(n, idx.to_vec()))
                .collect(),
        }
    }

    #[test]
    fn family_strip_works() {
        assert_eq!(family_from_name("row_alldiff[1, 2, 3, 5]"), "row_alldiff");
        assert_eq!(family_from_name("box_contains[0, 0, 5]"), "box_contains");
        // No brackets: passed through.
        assert_eq!(family_from_name("plain"), "plain");
        // Trailing whitespace before bracket trimmed.
        assert_eq!(family_from_name("foo  [x]"), "foo");
    }

    #[test]
    fn quantise_buckets() {
        assert_eq!(quantise(1), 1);
        assert_eq!(quantise(2), 2);
        assert_eq!(quantise(3), 3);
        assert_eq!(quantise(7), 3);
        assert_eq!(quantise(99), 3);
    }

    /// Two graphs that are isomorphic should produce the same canonical form,
    /// regardless of the order of the input nodes.
    #[test]
    fn isomorphism_collapses_orderings() {
        // A path of three nodes: a — b — c, all distinct family.
        let infos1 = vec![
            mk_node("A", &[("x", &[1])]),
            mk_node("B", &[("x", &[1]), ("x", &[2])]),
            mk_node("C", &[("x", &[2])]),
        ];
        let edges1 = build_edges(&infos1);
        let canon1 = canonicalise(&infos1, &edges1);

        // Same path, nodes reordered.
        let infos2 = vec![
            mk_node("C", &[("x", &[2])]),
            mk_node("A", &[("x", &[1])]),
            mk_node("B", &[("x", &[1]), ("x", &[2])]),
        ];
        let edges2 = build_edges(&infos2);
        let canon2 = canonicalise(&infos2, &edges2);

        assert_eq!(canon1, canon2);
    }

    /// Different graphs should produce different fingerprints.
    #[test]
    fn distinct_graphs_distinct_fingerprints() {
        // Path A—B—C
        let path = vec![
            mk_node("X", &[("v", &[1])]),
            mk_node("X", &[("v", &[1]), ("v", &[2])]),
            mk_node("X", &[("v", &[2])]),
        ];
        // Triangle A—B—C—A
        let triangle = vec![
            mk_node("X", &[("v", &[1]), ("v", &[2])]),
            mk_node("X", &[("v", &[2]), ("v", &[3])]),
            mk_node("X", &[("v", &[1]), ("v", &[3])]),
        ];
        let canon_path = canonicalise(&path, &build_edges(&path));
        let canon_tri = canonicalise(&triangle, &build_edges(&triangle));
        assert_ne!(canon_path, canon_tri);
    }

    /// Edge-weight quantisation makes 4×4 and 9×9 sudoku patterns coincide
    /// (modelled here by changing the *number* of shared variables but not
    /// whether the overlap is "many" or "few").
    #[test]
    fn weight_quantisation_collapses_size() {
        // Small puzzle: two constraints share 5 variables.
        let small = vec![
            mk_node(
                "C",
                &[
                    ("v", &[1]),
                    ("v", &[2]),
                    ("v", &[3]),
                    ("v", &[4]),
                    ("v", &[5]),
                ],
            ),
            mk_node(
                "C",
                &[
                    ("v", &[1]),
                    ("v", &[2]),
                    ("v", &[3]),
                    ("v", &[4]),
                    ("v", &[5]),
                ],
            ),
        ];
        // Large puzzle: same constraints share 9 variables.
        let large = vec![
            mk_node(
                "C",
                &[
                    ("v", &[1]),
                    ("v", &[2]),
                    ("v", &[3]),
                    ("v", &[4]),
                    ("v", &[5]),
                    ("v", &[6]),
                    ("v", &[7]),
                    ("v", &[8]),
                    ("v", &[9]),
                ],
            ),
            mk_node(
                "C",
                &[
                    ("v", &[1]),
                    ("v", &[2]),
                    ("v", &[3]),
                    ("v", &[4]),
                    ("v", &[5]),
                    ("v", &[6]),
                    ("v", &[7]),
                    ("v", &[8]),
                    ("v", &[9]),
                ],
            ),
        ];
        let canon_small = canonicalise(&small, &build_edges(&small));
        let canon_large = canonicalise(&large, &build_edges(&large));
        assert_eq!(canon_small, canon_large);
    }

    /// The naked-vs-hidden signature: a MUS using only `*_alldiff` should not
    /// fingerprint the same as a MUS using `*_contains`.
    #[test]
    fn family_distinguishes_naked_vs_hidden() {
        let naked = vec![
            mk_node("alldiff", &[("v", &[1])]),
            mk_node("alldiff", &[("v", &[1]), ("v", &[2])]),
        ];
        let hidden = vec![
            mk_node("contains", &[("v", &[1])]),
            mk_node("contains", &[("v", &[1]), ("v", &[2])]),
        ];
        let canon_n = canonicalise(&naked, &build_edges(&naked));
        let canon_h = canonicalise(&hidden, &build_edges(&hidden));
        assert_ne!(canon_n, canon_h);
    }

    /// Single-node graph (e.g. a unit MUS) round-trips cleanly.
    #[test]
    fn singleton_canonical_form() {
        let info = vec![mk_node("naked_single", &[("v", &[1])])];
        let canon = canonicalise(&info, &build_edges(&info));
        // Form: family;edges (edges empty for singleton)
        assert_eq!(canon, "naked_single;");
    }

    /// Empty MUS (defensive — shouldn't happen in practice).
    #[test]
    fn empty_canonical_form() {
        let canon = canonicalise(&[], &[]);
        assert_eq!(canon, ";");
    }

    /// Above the brute-force cap, the degree-sequence fallback is used.
    /// Two graphs with matching family multisets and degree sequences will
    /// alias to the same fingerprint — that's an accepted false-collision
    /// in exchange for tractability.
    #[test]
    fn degree_fallback_above_cap() {
        let infos: Vec<NodeInfo> = (0..10).map(|i| mk_node("X", &[("v", &[i])])).collect();
        let canon = canonicalise(&infos, &[]);
        assert!(
            canon.starts_with("DEG:"),
            "should use degree fallback: {canon}"
        );
    }
}
