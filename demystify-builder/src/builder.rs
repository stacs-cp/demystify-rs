//! `PuzzleBuilder` — declare boolean variables / constraint families, post
//! cardinality constraints, and finalise into a [`PuzzleParse`].

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;
use std::sync::Arc;

use demystify::problem::parse::{
    ConstraintStore, DirectEncoding, Family, OrderEncoding, PuzzleParse, ShowDirective, ShowRole,
    VarLitSets,
};
use demystify::problem::{PuzLit, PuzVar, VarValPair};
use rustsat::instances::SatInstance;
use rustsat::types::Lit;

use crate::cardinality::{encode_sum_eq, encode_sum_ge, encode_sum_le};
use crate::error::BuildError;

/// A single boolean atom in the puzzle.  Wraps a raw SAT literal; `+atom`
/// means the variable is true, `-atom` means it is false.
///
/// Atoms are `Copy` — pass them around freely.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Atom {
    lit: Lit,
}

impl Atom {
    /// The underlying SAT literal (positive = "atom is true").
    #[must_use]
    pub fn lit(self) -> Lit {
        self.lit
    }

    /// Signed wrapper: the atom must be true.
    #[must_use]
    pub fn pos(self) -> Signed {
        Signed {
            atom: self,
            pos: true,
        }
    }

    /// Signed wrapper: the atom must be false.
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn neg(self) -> Signed {
        Signed {
            atom: self,
            pos: false,
        }
    }
}

/// A literal in a cardinality constraint: either `+atom` (the atom must
/// be true to count) or `-atom` (the atom must be false to count).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Signed {
    pub atom: Atom,
    pub pos: bool,
}

impl Signed {
    fn as_lit(self) -> Lit {
        if self.pos {
            self.atom.lit
        } else {
            !self.atom.lit
        }
    }
}

/// Whether atoms in a declared matrix are user-deducible puzzle variables
/// (`$#VAR`), constraint-family activation lits (`$#CON`), or auxiliary
/// deducible atoms (`$#AUX`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum AtomRole {
    Var,
    Con,
    Aux,
}

/// A declared matrix of boolean atoms.  Stored row-major over the
/// (1-indexed inclusive) ranges given at declaration time.
#[derive(Clone, Debug)]
pub struct BoolMatrix {
    name: String,
    dims: Vec<RangeInclusive<i64>>,
    atoms: Vec<Atom>,
    #[allow(dead_code)]
    role: AtomRole,
}

impl BoolMatrix {
    /// Variable name as it appears in `PuzVar`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The declared index ranges (one per matrix dimension).
    #[must_use]
    pub fn dims(&self) -> &[RangeInclusive<i64>] {
        &self.dims
    }

    /// The atom at the given (1-indexed) tuple of indices.  Panics if the
    /// arity is wrong or any index is out of range.
    #[must_use]
    pub fn get(&self, indices: &[i64]) -> Atom {
        self.try_get(indices)
            .expect("BoolMatrix::get: bad indices — use try_get for non-panicking access")
    }

    /// Same as [`Self::get`] but returns an error rather than panicking.
    pub fn try_get(&self, indices: &[i64]) -> Result<Atom, BuildError> {
        if indices.len() != self.dims.len() {
            return Err(BuildError::IndexArityMismatch {
                name: self.name.clone(),
                expected: self.dims.len(),
                got: indices.len(),
            });
        }
        let mut flat = 0_usize;
        for (axis, (&idx, range)) in indices.iter().zip(self.dims.iter()).enumerate() {
            if !range.contains(&idx) {
                return Err(BuildError::IndexOutOfRange {
                    name: self.name.clone(),
                    axis,
                    got: idx,
                    range: format!("{}..={}", range.start(), range.end()),
                });
            }
            let span = (range.end() - range.start() + 1) as usize;
            let offset = (idx - range.start()) as usize;
            flat = flat * span + offset;
        }
        Ok(self.atoms[flat])
    }

    /// Iterate every (1-indexed indices, atom) entry in row-major order.
    pub fn iter(&self) -> BoolMatrixIter<'_> {
        BoolMatrixIter {
            matrix: self,
            next: 0,
        }
    }

    /// Flat slice of every atom in the matrix, row-major.  Convenient for
    /// `sum_*` over the whole matrix.
    #[must_use]
    pub fn atoms(&self) -> &[Atom] {
        &self.atoms
    }

    fn indices_of(&self, flat: usize) -> Vec<i64> {
        let mut rem = flat;
        let mut out = vec![0_i64; self.dims.len()];
        for (axis, range) in self.dims.iter().enumerate().rev() {
            let span = (range.end() - range.start() + 1) as usize;
            let component = rem % span;
            rem /= span;
            out[axis] = range.start() + component as i64;
        }
        out
    }
}

pub struct BoolMatrixIter<'a> {
    matrix: &'a BoolMatrix,
    next: usize,
}

impl<'a> Iterator for BoolMatrixIter<'a> {
    type Item = (Vec<i64>, Atom);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.matrix.atoms.len() {
            return None;
        }
        let indices = self.matrix.indices_of(self.next);
        let atom = self.matrix.atoms[self.next];
        self.next += 1;
        Some((indices, atom))
    }
}

/// Handle returned by `sum_ge` / `sum_le`.  Exposes the activation literal
/// so callers can correlate the constraint with downstream solver output.
#[derive(Copy, Clone, Debug)]
pub struct ConstraintHandle {
    pub lit: Lit,
}

/// Mutable handle for adding members to an in-progress `$#FAMILY` group.
pub struct FamilyMut<'a> {
    group_id: String,
    families: &'a mut BTreeMap<String, Family>,
}

impl<'a> FamilyMut<'a> {
    /// Add a member to this family group.  `member_id` must name a
    /// previously-declared `$#CON` family (typically the `name` argument
    /// of a `con_bool_matrix` call); `label` is the group-relative label
    /// shown by the renderer.
    pub fn member(self, member_id: &str, label: &str) -> Self {
        let family = self
            .families
            .get_mut(&self.group_id)
            .expect("FamilyMut: group disappeared mid-build");
        family
            .members
            .insert(member_id.to_string(), label.to_string());
        self
    }
}

/// Builder for constructing a [`PuzzleParse`] entirely in Rust.
///
/// Typical lifecycle:
///
/// 1. Create with [`PuzzleBuilder::new`].
/// 2. Declare boolean variables (`var_bool_matrix`, `con_bool_matrix`,
///    `aux_bool_matrix`, `aux_bool`).
/// 3. Add show / kind / family / info metadata.
/// 4. Post constraints (`sum_ge`, `sum_le`) and any raw setup clauses
///    (`add_clause`).
/// 5. Call [`PuzzleBuilder::build`] to finalise into a `PuzzleParse`.
pub struct PuzzleBuilder {
    kind: Option<String>,
    info: Vec<String>,
    sat: SatInstance,

    var_names: BTreeSet<String>,
    aux_names: BTreeSet<String>,
    con_families: BTreeMap<String, String>,
    /// `lit → family name` for every `$#CON` activation atom.  Both the
    /// positive and negative orientation of each lit map to the same
    /// family (the activation sense is "lit positive").  Used to reject
    /// `sum_*` calls whose guard atom didn't come from the named family.
    con_atom_family: BTreeMap<Lit, String>,
    used_con_families: BTreeSet<String>,
    referenced_vars: BTreeSet<String>,

    litmap: BTreeMap<PuzLit, Lit>,
    domainmap: BTreeMap<PuzVar, BTreeSet<i64>>,

    var_lits_pos: BTreeSet<Lit>,
    var_lits_neg: BTreeSet<Lit>,
    var_lits_special: BTreeSet<Lit>,

    constraints: ConstraintStore,
    used_descriptions: BTreeSet<String>,

    show: Vec<ShowDirective>,
    families: BTreeMap<String, Family>,
}

impl Default for PuzzleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PuzzleBuilder {
    /// Construct a fresh, empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            kind: None,
            info: Vec::new(),
            sat: SatInstance::new(),
            var_names: BTreeSet::new(),
            aux_names: BTreeSet::new(),
            con_families: BTreeMap::new(),
            con_atom_family: BTreeMap::new(),
            used_con_families: BTreeSet::new(),
            referenced_vars: BTreeSet::new(),
            litmap: BTreeMap::new(),
            domainmap: BTreeMap::new(),
            var_lits_pos: BTreeSet::new(),
            var_lits_neg: BTreeSet::new(),
            var_lits_special: BTreeSet::new(),
            constraints: ConstraintStore::new(),
            used_descriptions: BTreeSet::new(),
            show: Vec::new(),
            families: BTreeMap::new(),
        }
    }

    /// Set the `$#KIND` tag.  Optional, but renderers may use it.
    pub fn kind(&mut self, kind: impl Into<String>) -> &mut Self {
        self.kind = Some(kind.into());
        self
    }

    /// Push a `$#INFO` string (concatenated by the renderer).
    pub fn info(&mut self, info: impl Into<String>) -> &mut Self {
        self.info.push(info.into());
        self
    }

    /// Declare a `$#VAR` boolean matrix — the user-facing deducible
    /// puzzle variables.  `dims` is a slice of 1-based inclusive ranges;
    /// pass an empty slice for a 0-d (scalar) variable.
    pub fn var_bool_matrix(&mut self, name: &str, dims: &[RangeInclusive<i64>]) -> BoolMatrix {
        self.declare_matrix(name, dims, AtomRole::Var)
    }

    /// Declare a `$#CON` constraint family.  Each atom in the returned
    /// matrix acts as an activation literal for one constraint instance —
    /// pass it as the `guard` argument to `sum_ge` / `sum_le`.
    pub fn con_bool_matrix(&mut self, name: &str, dims: &[RangeInclusive<i64>]) -> BoolMatrix {
        self.declare_matrix(name, dims, AtomRole::Con)
    }

    /// Declare a `$#CON` 0-dimensional constraint atom (scalar).
    pub fn con_bool(&mut self, name: &str) -> Atom {
        let m = self.declare_matrix(name, &[], AtomRole::Con);
        m.atoms[0]
    }

    /// Declare a `$#AUX` boolean matrix — auxiliary deducible atoms that
    /// don't directly correspond to the main puzzle output.  Rarely
    /// needed for star battle.
    pub fn aux_bool_matrix(&mut self, name: &str, dims: &[RangeInclusive<i64>]) -> BoolMatrix {
        self.declare_matrix(name, dims, AtomRole::Aux)
    }

    /// Shortcut for a 0-dimensional `$#AUX` boolean.
    pub fn aux_bool(&mut self, name: &str) -> Atom {
        let m = self.declare_matrix(name, &[], AtomRole::Aux);
        m.atoms[0]
    }

    fn declare_matrix(
        &mut self,
        name: &str,
        dims: &[RangeInclusive<i64>],
        role: AtomRole,
    ) -> BoolMatrix {
        if self.var_names.contains(name)
            || self.aux_names.contains(name)
            || self.con_families.contains_key(name)
        {
            panic!("variable name '{name}' is already declared");
        }
        for r in dims {
            assert!(
                r.start() <= r.end(),
                "BoolMatrix dim ranges must be non-empty: got {}..={}",
                r.start(),
                r.end()
            );
        }
        let total = dims
            .iter()
            .map(|r| (r.end() - r.start() + 1) as usize)
            .product::<usize>()
            .max(1);

        let mut atoms = Vec::with_capacity(total);
        // We iterate the cartesian product over `dims` in row-major order
        // (last index varies fastest).  For 0-d, produce a single entry
        // with an empty indices vec.
        let mut iter = CartesianProductIter::new(dims);
        while let Some(indices) = iter.next() {
            let lit = self.sat.new_lit();
            let atom = Atom { lit };
            atoms.push(atom);

            let puzvar = PuzVar::new(name, indices.clone());

            // Register all four PuzLit ↔ Lit entries.
            let vv0 = VarValPair::new(&puzvar, 0);
            let vv1 = VarValPair::new(&puzvar, 1);
            self.litmap.insert(PuzLit::new_eq(vv0.clone()), !lit);
            self.litmap.insert(PuzLit::new_neq(vv0), lit);
            self.litmap.insert(PuzLit::new_eq(vv1.clone()), lit);
            self.litmap.insert(PuzLit::new_neq(vv1), !lit);

            self.domainmap
                .entry(puzvar.clone())
                .or_default()
                .extend([0_i64, 1_i64]);

            // var_lits classification — mirrors `finalise()` in
            // demystify/src/problem/parse.rs.
            match role {
                AtomRole::Var => {
                    // For each of the four puzlits we'd insert lit into
                    // positive; for the two with `equal=false` we'd also
                    // insert it into negative.  Net effect: both signs
                    // of L are in `positive`, and both signs of L are
                    // in `negative`.
                    self.var_lits_pos.insert(lit);
                    self.var_lits_pos.insert(!lit);
                    self.var_lits_neg.insert(lit);
                    self.var_lits_neg.insert(!lit);
                }
                AtomRole::Aux => {
                    if name.starts_with("demystify_") {
                        // Per finalise(): demystify_* AUX atoms whose sign
                        // is true (the `=v` puzlits) go in `special`.
                        self.var_lits_special.insert(lit);
                        self.var_lits_special.insert(!lit);
                    }
                }
                AtomRole::Con => {
                    // Track lit → family so register_con can verify that
                    // a sum_* caller passes a guard from the right family.
                    self.con_atom_family.insert(lit, name.to_string());
                    self.con_atom_family.insert(!lit, name.to_string());
                }
            }
        }

        // Register the variable name in the appropriate annotation set.
        match role {
            AtomRole::Var => {
                self.var_names.insert(name.to_string());
            }
            AtomRole::Aux => {
                self.aux_names.insert(name.to_string());
            }
            AtomRole::Con => {
                // The template-string contents don't matter — we substitute
                // descriptions ourselves at constraint-post time — but the
                // key must be present and the value non-empty.
                self.con_families
                    .insert(name.to_string(), format!("{{builder:{name}}}"));
            }
        }

        BoolMatrix {
            name: name.to_string(),
            dims: dims.to_vec(),
            atoms,
            role,
        }
    }

    /// Register a `$#SHOW <var> <role>` directive.
    pub fn show(&mut self, var: &str, role: ShowRole) -> &mut Self {
        self.show.push(ShowDirective {
            var: var.to_string(),
            role,
        });
        self
    }

    /// Begin a `$#FAMILY` group declaration.  Members are added by
    /// chaining `.member(member_id, label)` calls.
    pub fn family(&mut self, group_id: &str, label: &str) -> FamilyMut<'_> {
        self.families.insert(
            group_id.to_string(),
            Family {
                label: label.to_string(),
                members: BTreeMap::new(),
            },
        );
        FamilyMut {
            group_id: group_id.to_string(),
            families: &mut self.families,
        }
    }

    /// Add a raw clause (not associated with any `$#CON` family).  Use
    /// for unconditional setup clauses.
    pub fn add_clause(&mut self, lits: impl IntoIterator<Item = Lit>) {
        let clause: Vec<Lit> = lits.into_iter().collect();
        self.sat.add_nary(&clause);
    }

    /// Post `guard -> (sum(signed) >= k)` and register a `$#CON` entry.
    ///
    /// `family` must be the name of a previously-declared
    /// `con_bool_matrix` (or `con_bool`); the activation lit `guard`
    /// must be one of that family's atoms (or any [`Atom`] whose
    /// `=true` lit you want to act as the guard).  `description` is
    /// the human-readable label demystify shows in explanations and
    /// must be unique across all constraints in the puzzle.
    pub fn sum_ge(
        &mut self,
        guard: Atom,
        signed: &[Signed],
        k: i64,
        family: &str,
        description: impl Into<String>,
    ) -> Result<ConstraintHandle, BuildError> {
        let description = description.into();
        self.register_con(family, &description, guard, signed)?;
        let raw_lits: Vec<Lit> = signed.iter().map(|s| s.as_lit()).collect();
        encode_sum_ge(&mut self.sat, &raw_lits, k, Some(guard.lit))?;
        Ok(ConstraintHandle { lit: guard.lit })
    }

    /// Post `guard -> (sum(signed) <= k)` and register a `$#CON` entry.
    /// See [`Self::sum_ge`] for the full contract.
    pub fn sum_le(
        &mut self,
        guard: Atom,
        signed: &[Signed],
        k: i64,
        family: &str,
        description: impl Into<String>,
    ) -> Result<ConstraintHandle, BuildError> {
        let description = description.into();
        self.register_con(family, &description, guard, signed)?;
        let raw_lits: Vec<Lit> = signed.iter().map(|s| s.as_lit()).collect();
        encode_sum_le(&mut self.sat, &raw_lits, k, Some(guard.lit))?;
        Ok(ConstraintHandle { lit: guard.lit })
    }

    /// Post `guard -> (sum(signed) == k)` and register a `$#CON` entry.
    /// Same contract as [`Self::sum_ge`].
    pub fn sum_eq(
        &mut self,
        guard: Atom,
        signed: &[Signed],
        k: i64,
        family: &str,
        description: impl Into<String>,
    ) -> Result<ConstraintHandle, BuildError> {
        let description = description.into();
        self.register_con(family, &description, guard, signed)?;
        let raw_lits: Vec<Lit> = signed.iter().map(|s| s.as_lit()).collect();
        encode_sum_eq(&mut self.sat, &raw_lits, k, Some(guard.lit))?;
        Ok(ConstraintHandle { lit: guard.lit })
    }

    /// Post `sum(signed) == k` unconditionally as plain CNF, with no
    /// `$#CON` entry.  Use for structural/setup constraints (one-hot
    /// encodings, fixed givens) that demystify shouldn't surface as
    /// individual deduction steps.
    pub fn sum_eq_unguarded(&mut self, signed: &[Signed], k: i64) -> Result<(), BuildError> {
        let raw_lits: Vec<Lit> = signed.iter().map(|s| s.as_lit()).collect();
        encode_sum_eq(&mut self.sat, &raw_lits, k, None)?;
        Ok(())
    }

    fn register_con(
        &mut self,
        family: &str,
        description: &str,
        guard: Atom,
        signed: &[Signed],
    ) -> Result<(), BuildError> {
        if !self.con_families.contains_key(family) {
            return Err(BuildError::UnknownFamily(family.to_string()));
        }
        // Validate that the guard atom was declared as part of the named
        // family.  A guard from a different family (or no family at all)
        // would silently corrupt the constraint store's family tracking.
        match self.con_atom_family.get(&guard.lit) {
            Some(actual) if actual == family => {}
            other => {
                return Err(BuildError::GuardFromWrongFamily {
                    expected: family.to_string(),
                    actual: other.cloned(),
                });
            }
        }
        if !self.used_descriptions.insert(description.to_string()) {
            return Err(BuildError::DuplicateConstraintDescription(
                description.to_string(),
            ));
        }
        // Collect both orientations of each user-facing atom in the sum,
        // mirroring how Conjure-built models populate `varlits_in_con`.
        let mut var_lits: Vec<Lit> = Vec::with_capacity(signed.len() * 2);
        for s in signed {
            var_lits.push(s.atom.lit);
            var_lits.push(!s.atom.lit);
            // For renderer / var_to_cons lookups, track which $#VAR atoms
            // this constraint mentions.
            if let Some(puzlits) = lookup_invlitmap(&self.litmap, s.atom.lit) {
                for pl in puzlits {
                    self.referenced_vars.insert(pl.var().name().clone());
                }
            }
        }
        // Sort & dedupe so the stored varlits list is canonical.
        var_lits.sort_by_key(|l| l.to_ipasir());
        var_lits.dedup();

        self.used_con_families.insert(family.to_string());
        self.constraints
            .insert(
                guard.lit,
                family.to_string(),
                description.to_string(),
                var_lits,
            )
            .map_err(|e| BuildError::Other(e.context("ConstraintStore::insert failed")))?;
        Ok(())
    }

    /// Finalise the builder into a fully-formed `PuzzleParse`.
    pub fn build(self) -> Result<PuzzleParse, BuildError> {
        // Loud-failure validations: every $#VAR must be mentioned in at
        // least one constraint; every $#CON family must be used.
        for v in &self.var_names {
            if !self.referenced_vars.contains(v) {
                return Err(BuildError::UnusedVar(v.clone()));
            }
        }
        for fam in self.con_families.keys() {
            if !self.used_con_families.contains(fam) {
                return Err(BuildError::UnusedFamily(fam.clone()));
            }
        }

        let direct = {
            let mut invlitmap: BTreeMap<Lit, BTreeSet<PuzLit>> = BTreeMap::new();
            for (puzlit, &lit) in &self.litmap {
                invlitmap.entry(lit).or_default().insert(puzlit.clone());
            }
            DirectEncoding {
                litmap: self.litmap,
                invlitmap,
                domainmap: self.domainmap,
            }
        };
        let order = OrderEncoding::new();
        let var_lits =
            VarLitSets::from_raw(self.var_lits_pos, self.var_lits_neg, self.var_lits_special);

        let var_to_cons_owned = PuzzleParse::build_var_to_cons(&self.constraints, &direct, &order);

        let cnf = self.sat.cnf().clone();
        let satinstance = self.sat;

        // Construct PuzzleParse using the public `new_from_eprime` builder
        // for the annotations + a couple of post-fixes.  All fields of
        // PuzzleParse are public except `var_to_cons`, which we set via
        // the public `set_var_to_cons` (see the patch in `demystify`).
        let mut parse = PuzzleParse::new_from_eprime(
            self.var_names,
            self.aux_names,
            self.con_families,
            BTreeMap::new(), // reveal
            BTreeMap::new(), // params
            self.kind,
            self.info,
            self.families,
            self.show,
        );
        parse.satinstance = satinstance;
        parse.cnf = Some(Arc::new(cnf));
        parse.direct = direct;
        parse.constraints = self.constraints;
        parse.var_lits = var_lits;
        parse.order = order;
        parse.set_var_to_cons(var_to_cons_owned);

        Ok(parse)
    }
}

fn lookup_invlitmap(litmap: &BTreeMap<PuzLit, Lit>, lit: Lit) -> Option<Vec<&PuzLit>> {
    let hits: Vec<&PuzLit> = litmap
        .iter()
        .filter_map(|(pl, &l)| if l == lit { Some(pl) } else { None })
        .collect();
    if hits.is_empty() { None } else { Some(hits) }
}

/// Generates 1-indexed inclusive cartesian products of the given ranges
/// in row-major order (last axis varies fastest).  An empty `dims`
/// produces a single empty tuple.
struct CartesianProductIter<'a> {
    dims: &'a [RangeInclusive<i64>],
    counters: Vec<i64>,
    done: bool,
}

impl<'a> CartesianProductIter<'a> {
    fn new(dims: &'a [RangeInclusive<i64>]) -> Self {
        let counters = dims.iter().map(|r| *r.start()).collect::<Vec<_>>();
        Self {
            dims,
            counters,
            done: false,
        }
    }

    fn next(&mut self) -> Option<Vec<i64>> {
        if self.done {
            return None;
        }
        let out = self.counters.clone();
        // Advance counters last-axis-fastest.
        if self.dims.is_empty() {
            self.done = true;
            return Some(out);
        }
        let mut axis = self.dims.len();
        loop {
            if axis == 0 {
                self.done = true;
                break;
            }
            axis -= 1;
            if self.counters[axis] < *self.dims[axis].end() {
                self.counters[axis] += 1;
                // Reset all later axes.
                for later in axis + 1..self.dims.len() {
                    self.counters[later] = *self.dims[later].start();
                }
                break;
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cartesian_product_2x3() {
        let dims = vec![1..=2, 1..=3];
        let mut it = CartesianProductIter::new(&dims);
        let mut got = Vec::new();
        while let Some(v) = it.next() {
            got.push(v);
        }
        assert_eq!(
            got,
            vec![
                vec![1, 1],
                vec![1, 2],
                vec![1, 3],
                vec![2, 1],
                vec![2, 2],
                vec![2, 3],
            ]
        );
    }

    #[test]
    fn cartesian_product_empty_is_unit() {
        let dims: Vec<RangeInclusive<i64>> = vec![];
        let mut it = CartesianProductIter::new(&dims);
        assert_eq!(it.next(), Some(vec![]));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn var_bool_matrix_registers_four_puzlits_per_cell() {
        let mut b = PuzzleBuilder::new();
        let m = b.var_bool_matrix("g", &[1..=1, 1..=2]);
        assert_eq!(m.atoms.len(), 2);
        // 4 puzlits per cell × 2 cells = 8 litmap entries.
        assert_eq!(b.litmap.len(), 8);
    }

    #[test]
    fn matrix_iter_yields_row_major() {
        let mut b = PuzzleBuilder::new();
        let m = b.var_bool_matrix("g", &[1..=2, 1..=2]);
        let collected: Vec<Vec<i64>> = m.iter().map(|(i, _)| i).collect();
        assert_eq!(
            collected,
            vec![vec![1, 1], vec![1, 2], vec![2, 1], vec![2, 2]]
        );
    }

    #[test]
    fn matrix_get_index_out_of_range_errors() {
        let mut b = PuzzleBuilder::new();
        let m = b.var_bool_matrix("g", &[1..=2]);
        assert!(matches!(
            m.try_get(&[3]),
            Err(BuildError::IndexOutOfRange { .. })
        ));
    }

    #[test]
    fn matrix_get_arity_mismatch_errors() {
        let mut b = PuzzleBuilder::new();
        let m = b.var_bool_matrix("g", &[1..=2]);
        assert!(matches!(
            m.try_get(&[1, 1]),
            Err(BuildError::IndexArityMismatch { .. })
        ));
    }

    #[test]
    fn duplicate_description_rejected() {
        let mut b = PuzzleBuilder::new();
        let v = b.var_bool_matrix("g", &[1..=2]);
        let act = b.con_bool_matrix("rule", &[1..=2]);
        b.sum_ge(act.get(&[1]), &[v.get(&[1]).pos()], 1, "rule", "desc")
            .unwrap();
        let err = b
            .sum_ge(act.get(&[2]), &[v.get(&[2]).pos()], 1, "rule", "desc")
            .unwrap_err();
        assert!(matches!(err, BuildError::DuplicateConstraintDescription(_)));
    }

    #[test]
    fn unused_var_rejected() {
        let mut b = PuzzleBuilder::new();
        let _g = b.var_bool_matrix("g", &[1..=1]);
        // No constraints reference g.
        let err = b.build().unwrap_err();
        assert!(matches!(err, BuildError::UnusedVar(name) if name == "g"));
    }

    #[test]
    fn guard_from_other_family_rejected() {
        let mut b = PuzzleBuilder::new();
        let v = b.var_bool_matrix("g", &[1..=2]);
        let foo = b.con_bool_matrix("foo", &[1..=2]);
        let _bar = b.con_bool_matrix("bar", &[1..=2]);
        // foo[1] does not belong to family "bar".
        let err = b
            .sum_ge(foo.get(&[1]), &[v.get(&[1]).pos()], 1, "bar", "desc")
            .unwrap_err();
        assert!(matches!(
            err,
            BuildError::GuardFromWrongFamily { expected, actual: Some(a) }
                if expected == "bar" && a == "foo"
        ));
    }

    #[test]
    fn guard_from_var_atom_rejected() {
        let mut b = PuzzleBuilder::new();
        let v = b.var_bool_matrix("g", &[1..=2]);
        let _rule = b.con_bool_matrix("rule", &[1..=2]);
        // v[1] is a $#VAR atom, not a constraint activation lit at all.
        let err = b
            .sum_ge(v.get(&[1]), &[v.get(&[2]).pos()], 1, "rule", "desc")
            .unwrap_err();
        assert!(matches!(
            err,
            BuildError::GuardFromWrongFamily { actual: None, .. }
        ));
    }

    /// Build a tiny puzzle that uses sum_ge with one positive and one
    /// negated literal — equivalent to `a ∨ ¬b ≥ 1`, i.e. `b → a` — and
    /// verify via the SAT solver that the resulting CNF behaves like
    /// that implication.
    #[test]
    fn sum_ge_with_negated_literal_encodes_implication() {
        use rustsat::solvers::{Solve, SolveIncremental, SolverResult};
        let mut b = PuzzleBuilder::new();
        let v = b.var_bool_matrix("g", &[1..=2]);
        let rule = b.con_bool("rule");
        b.sum_ge(
            rule,
            &[v.get(&[1]).pos(), v.get(&[2]).neg()],
            1,
            "rule",
            "g[1] or not g[2]",
        )
        .unwrap();
        let puzzle = b.build().unwrap();
        let cnf = puzzle.cnf.as_ref().unwrap().as_ref().clone();
        let g1_lit = puzzle
            .direct
            .litmap
            .get(&PuzLit::new_eq(VarValPair::new(
                &PuzVar::new("g", vec![1]),
                1,
            )))
            .copied()
            .unwrap();
        let g2_lit = puzzle
            .direct
            .litmap
            .get(&PuzLit::new_eq(VarValPair::new(
                &PuzVar::new("g", vec![2]),
                1,
            )))
            .copied()
            .unwrap();
        let rule_lit = rule.lit();

        // For each of the 4 (g1, g2) assignments under "rule = true",
        // confirm the encoding is satisfiable iff g1 ∨ ¬g2 holds.
        for g1_val in [true, false] {
            for g2_val in [true, false] {
                let mut solver = rustsat_batsat::BasicSolver::default();
                for cl in cnf.iter() {
                    solver.add_clause(cl.clone()).unwrap();
                }
                let assumps = vec![
                    rule_lit,
                    if g1_val { g1_lit } else { !g1_lit },
                    if g2_val { g2_lit } else { !g2_lit },
                ];
                let res = solver.solve_assumps(&assumps).unwrap();
                let expected_sat = g1_val || !g2_val;
                assert_eq!(
                    res == SolverResult::Sat,
                    expected_sat,
                    "rule=true, g1={g1_val}, g2={g2_val}: expected sat={expected_sat}"
                );
            }
        }
    }

    /// `add_clause` should pin a literal: building a puzzle whose only
    /// constraint is "guard → sum(stars) ≥ 1" together with a unit
    /// `add_clause([stars[1].neg().as_lit_for_test()])` should be UNSAT
    /// once we assume `guard` true.
    #[test]
    fn add_clause_pins_literal() {
        use rustsat::solvers::{Solve, SolveIncremental, SolverResult};
        let mut b = PuzzleBuilder::new();
        let stars = b.var_bool_matrix("stars", &[1..=1]);
        let must = b.con_bool("must");
        b.sum_ge(
            must,
            &[stars.get(&[1]).pos()],
            1,
            "must",
            "at least one star",
        )
        .unwrap();
        // Pin stars[1] = false via a raw unit clause.
        b.add_clause([!stars.get(&[1]).lit()]);
        let puzzle = b.build().unwrap();
        let cnf = puzzle.cnf.as_ref().unwrap().as_ref().clone();
        let mut solver = rustsat_batsat::BasicSolver::default();
        for cl in cnf.iter() {
            solver.add_clause(cl.clone()).unwrap();
        }
        // With must=true, the puzzle should be UNSAT (we need a star
        // but the unit clause forbids the only one).
        let res = solver.solve_assumps(&[must.lit()]).unwrap();
        assert_eq!(res, SolverResult::Unsat);
        // With must=false, satisfiable.
        let res2 = solver.solve_assumps(&[!must.lit()]).unwrap();
        assert_eq!(res2, SolverResult::Sat);
    }

    #[test]
    fn aux_bool_builds_and_solves() {
        // An $#AUX atom should be encodable, end up in the right
        // annotation set, and let demystify build/solve through.
        let mut b = PuzzleBuilder::new();
        let v = b.var_bool_matrix("g", &[1..=2]);
        let helper = b.aux_bool_matrix("helper", &[1..=2]);
        let rule = b.con_bool_matrix("rule", &[1..=2]);
        // Two constraints, each referencing the aux atom on the same side
        // as a star: rule[i] → (helper[i] ∨ g[i]) ≥ 1.
        for i in 1..=2 {
            b.sum_ge(
                rule.get(&[i]),
                &[helper.get(&[i]).pos(), v.get(&[i]).pos()],
                1,
                "rule",
                format!("helper[{i}] or g[{i}]"),
            )
            .unwrap();
        }
        let puzzle = b.build().unwrap();
        assert!(puzzle.eprime.auxvars.contains("helper"));
        assert!(puzzle.eprime.vars.contains("g"));
        assert!(puzzle.eprime.cons.contains_key("rule"));
    }

    #[test]
    fn sum_eq_unguarded_emits_no_constraint_entry() {
        let mut b = PuzzleBuilder::new();
        let v = b.var_bool_matrix("g", &[1..=2]);
        let force = b.con_bool("force");
        // One real $#CON so the builder still has something registered.
        b.sum_ge(
            force,
            &[v.get(&[1]).pos(), v.get(&[2]).pos()],
            1,
            "force",
            "at least one g is true",
        )
        .unwrap();
        // Unguarded sum_eq for setup: exactly one of g[1], g[2] is true.
        b.sum_eq_unguarded(&[v.get(&[1]).pos(), v.get(&[2]).pos()], 1)
            .unwrap();
        let puzzle = b.build().unwrap();
        // Only one $#CON entry — the unguarded constraint does not appear.
        assert_eq!(puzzle.constraints.len(), 1);
    }
}
