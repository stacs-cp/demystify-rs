#![allow(clippy::needless_range_loop)]
/// This module contains the implementation of parsing and processing functions for problem files.
/// It includes functions for parsing DIMACS files, extracting annotations from Essence' files,
/// and creating data structures to represent the parsed information.
///
/// The main struct in this module is `PuzzleParse`, which represents the result of parsing a DIMACS file.
/// It contains various fields to store the parsed information, such as the annotations from the Essence' file,
/// the SAT instance parsed from the DIMACS file, mappings between literals and SAT integers, and more.
use anyhow::{Context, bail};
use itertools::Itertools;
use regex::Regex;
use rustsat::instances::{self, BasicVarManager, Cnf, SatInstance};
use rustsat::types::Lit;

use std::cmp::max;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use std::fs;
use std::io::BufReader;
use std::io::prelude::*;

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, info};

use std::fs::File;
use std::io;

use crate::problem::util::exec::ProgramRunner;
use crate::problem::util::parsing;
use crate::problem::{PuzLit, PuzVar};

use super::VarValPair;
use super::util::{FindVarConnections, safe_insert};

#[derive(Debug, Clone, PartialEq)]
pub struct EPrimeAnnotations {
    /// The set of variables in the Essence' file.
    pub vars: BTreeSet<String>,
    /// The set of auxiliary variables in the Essence'file.
    pub auxvars: BTreeSet<String>,
    /// The constraints in the Essence' file, represented as a mapping from constraint name to constraint expression.
    pub cons: BTreeMap<String, String>,
    /// 'reveal', which allow extra information to be added during solving.
    pub reveal: BTreeMap<String, String>,
    /// values from the 'reveal' map (for ease of searching)
    pub reveal_values: BTreeSet<String>,
    /// The parameters read from the param file
    pub(crate) params: BTreeMap<String, serde_json::value::Value>,
    /// The kind of puzzle
    pub kind: Option<String>,
    /// Informational strings collected from $#INFO directives
    pub info: Vec<String>,
    /// SVG decoration flags collected from $#DEC directives
    pub decs: Vec<String>,
    /// Constraint-family groups collected from $#FAMILY directives.
    /// Maps a group id to the `Family` describing its label and its members
    /// (each member is a constraint-family prefix — the part before `[` in a
    /// constraint name — together with a group-relative human label).
    /// A constraint family may belong to multiple groups.
    pub families: BTreeMap<String, Family>,
}

/// A family-group declared by a `$#FAMILY` directive. Carries human-readable
/// labels that are *group-relative*: the same member id (e.g. `row_alldiff`)
/// can have different labels in different groups depending on what the group
/// abstracts over.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Family {
    /// Group-level human label (defaults to the group id when not given).
    pub label: String,
    /// Member id → group-relative member label.  Member labels default to
    /// the member id when not given.
    pub members: BTreeMap<String, String>,
}

impl EPrimeAnnotations {
    /// Returns a reference to all parameters
    #[must_use]
    pub fn params(&self) -> &BTreeMap<String, serde_json::value::Value> {
        &self.params
    }

    #[must_use]
    pub fn has_param(&self, s: &str) -> bool {
        self.params.contains_key(s)
    }

    pub fn param_bool(&self, s: &str) -> anyhow::Result<bool> {
        serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {s}"))?
                .clone(),
        )
        .context(format!("Param {s} is not bool"))
    }

    pub fn param_i64(&self, s: &str) -> anyhow::Result<i64> {
        serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {s}"))?
                .clone(),
        )
        .context(format!("Param {s} is not int"))
    }

    pub fn param_vec_i64(&self, s: &str) -> anyhow::Result<Vec<i64>> {
        // Conjure produces arrays as maps, so we need to fix up
        let map: BTreeMap<i64, i64> = serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {s}"))?
                .clone(),
        )
        .context(format!("Param {s} is not an array of ints"))?;

        let mut ret: Vec<i64> = vec![0; map.len()];

        for i in 0..map.len() {
            ret[i] = *map
                .get(&((i + 1) as i64))
                .context(format!("Malformed param? {s}"))?;
        }

        Ok(ret)
    }

    pub fn param_vec_vec_i64(&self, s: &str) -> anyhow::Result<Vec<Vec<i64>>> {
        // Conjure produces arrays as maps, so we need to fix up
        let map: BTreeMap<i64, BTreeMap<i64, i64>> = serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {s}"))?
                .clone(),
        )
        .context(format!("Param {s} is not a 2d array of ints"))?;

        let mut ret: Vec<Vec<i64>> = vec![vec![]; map.len()];

        for i in 0..map.len() {
            let row = map
                .get(&((i + 1) as i64))
                .context(format!("Malformed param? {s}"))?;
            let mut rowvec: Vec<i64> = vec![0; row.len()];
            for j in 0..row.len() {
                rowvec[j] = *row
                    .get(&((j + 1) as i64))
                    .context(format!("Malformed param? {s}"))?;
            }

            ret[i] = rowvec;
        }

        Ok(ret)
    }

    pub fn param_vec_string(&self, s: &str) -> anyhow::Result<Vec<String>> {
        let map: BTreeMap<i64, serde_json::Value> = serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {s}"))?
                .clone(),
        )
        .context(format!("Param {s} is not an array of strings"))?;

        let mut ret: Vec<String> = vec![String::new(); map.len()];

        for i in 0..map.len() {
            ret[i] = map
                .get(&((i + 1) as i64))
                .context(format!("Malformed param? {s}"))?
                .to_string();
        }

        Ok(ret)
    }

    pub fn param_vec_vec_string(&self, s: &str) -> anyhow::Result<Vec<Vec<String>>> {
        // Conjure produces arrays as maps, so we need to fix up
        let map: BTreeMap<i64, BTreeMap<i64, serde_json::Value>> = serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {s}"))?
                .clone(),
        )
        .context(format!("Param {s} is not a 2d array of strings"))?;

        let mut ret: Vec<Vec<String>> = vec![vec![]; map.len()];

        for i in 0..map.len() {
            let row = map
                .get(&((i + 1) as i64))
                .context(format!("Malformed param? {s}"))?;
            let mut rowvec: Vec<String> = vec![String::new(); row.len()];
            for j in 0..row.len() {
                rowvec[j] = row
                    .get(&((j + 1) as i64))
                    .context(format!("Malformed param? {s}"))?
                    .to_string();
            }

            ret[i] = rowvec;
        }

        Ok(ret)
    }

    pub fn param_vec_vec_option_i64(&self, s: &str) -> anyhow::Result<Vec<Vec<Option<i64>>>> {
        // Conjure produces arrays as maps, so we need to fix up
        let map: BTreeMap<i64, BTreeMap<i64, Option<i64>>> = serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {s}"))?
                .clone(),
        )
        .context(format!("Param {s} is not a 2d array of ints and nulls"))?;

        let mut ret: Vec<Vec<Option<i64>>> = vec![vec![]; map.len()];

        for i in 0..map.len() {
            let row = map
                .get(&((i + 1) as i64))
                .context(format!("Malformed param? {s}"))?;
            let mut rowvec: Vec<Option<i64>> = vec![None; row.len()];
            for j in 0..row.len() {
                rowvec[j] = *row
                    .get(&((j + 1) as i64))
                    .context(format!("Malformed param? {s}"))?;
            }

            ret[i] = rowvec;
        }

        Ok(ret)
    }

    /// Parses the concatenated `info` strings as a JSON object
    pub fn parse_info_json<T: serde::de::DeserializeOwned>(&self) -> anyhow::Result<T> {
        let json_str = self.info.concat();
        if json_str.trim().is_empty() {
            serde_json::from_str("{}").context("Failed to parse empty info as JSON object")
        } else {
            serde_json::from_str(&json_str).context("Failed to parse info as JSON")
        }
    }
}

/// The fully-parsed puzzle, combining the Essence' annotations with all SAT encoding maps.
///
/// # Architecture overview
///
/// Conjure translates an `.eprime` model + `.param` file into a DIMACS CNF.  That CNF uses two
/// distinct encodings for the puzzle variables:
///
/// * **Direct encoding** — each assignment `var[i,j] = v` becomes one SAT literal.
///   `litmap` / `invlitmap` translate between these puzzle-level assignments and raw SAT literals.
///
/// * **Order encoding** — each threshold `var[i,j] >= v` becomes one SAT literal.
///   `order_encoding_map` / `inv_order_encoding_map` handle this encoding.
///   Most code only needs the direct encoding; the order maps are used when scanning the full SAT
///   instance (e.g. `all_var_related_lits`).
///
/// ## Key maps and their relationships
///
/// ```text
/// PuzLit (var[i,j]=v, eq/neq)  ←→  Lit (raw SAT integer)
///     direct.litmap:     PuzLit → Lit
///     direct.invlitmap:  Lit → BTreeSet<PuzLit>
///
/// PuzVar (var[i,j])  →  domain values
///     direct.domainmap:  PuzVar → BTreeSet<i64>
///
/// Constraint instances ($#CON class[i,j,...] = true):
///     constraints  (ConstraintStore — four maps kept in sync atomically)
///
/// Convenience sets:
///     var_lits  (VarLitSets — positive, negative, and special lit sets)
///
/// Order encoding:
///     order  (OrderEncoding — forward/inverse/all-lits maps)
/// ```
///
/// ## Reveal map
///
/// `$#AUX` variables expose "extra" assignments when their trigger literal is proved.
/// `reveal_map[x] = y` means: when `x` is deduced true, also treat `y` as known.

// ── ConstraintStore ──────────────────────────────────────────────────────────
// Four maps that must always agree on which constraint lits exist.

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConstraintStore {
    conset: BTreeMap<Lit, String>,
    invconset: BTreeMap<String, Lit>,
    varlits_in_con: BTreeMap<Lit, Vec<Lit>>,
    /// Constraint-family root name (e.g. `row_alldiff`) for each constraint
    /// lit.  This is the `$#CON` declaration name without index substitution
    /// — needed by the named-strategy fingerprinter to identify families,
    /// since the rendered `description` is template-substituted free text.
    family_of: BTreeMap<Lit, String>,
    lits: BTreeSet<Lit>,
}

impl ConstraintStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            conset: BTreeMap::new(),
            invconset: BTreeMap::new(),
            varlits_in_con: BTreeMap::new(),
            family_of: BTreeMap::new(),
            lits: BTreeSet::new(),
        }
    }

    pub fn insert(
        &mut self,
        lit: Lit,
        family: String,
        description: String,
        var_lits: Vec<Lit>,
    ) -> anyhow::Result<()> {
        safe_insert(&mut self.conset, lit, description.clone())?;
        safe_insert(&mut self.invconset, description, lit)?;
        safe_insert(&mut self.family_of, lit, family)?;
        self.lits.insert(lit);
        safe_insert(&mut self.varlits_in_con, lit, var_lits)?;
        Ok(())
    }

    pub fn remove(&mut self, lit: &Lit) {
        self.lits.remove(lit);
        if let Some(name) = self.conset.remove(lit) {
            self.invconset.remove(&name);
        }
        self.varlits_in_con.remove(lit);
        self.family_of.remove(lit);
    }

    pub fn retain(&mut self, mut f: impl FnMut(&Lit) -> bool) {
        let to_remove: Vec<Lit> = self.lits.iter().filter(|l| !f(l)).copied().collect();
        for lit in &to_remove {
            self.remove(lit);
        }
    }

    #[must_use]
    pub fn contains(&self, lit: &Lit) -> bool {
        self.lits.contains(lit)
    }

    #[must_use]
    pub fn description(&self, lit: &Lit) -> &String {
        assert!(self.contains(lit));
        self.conset.get(lit).unwrap()
    }

    #[must_use]
    pub fn try_description(&self, lit: &Lit) -> Option<&String> {
        self.conset.get(lit)
    }

    /// Constraint-family root name for a lit (e.g. `"row_alldiff"`), or
    /// `None` if the lit is not in the store.
    #[must_use]
    pub fn family_of(&self, lit: &Lit) -> Option<&String> {
        self.family_of.get(lit)
    }

    /// Iterate over `(lit, family-root-name)` pairs.
    pub fn families(&self) -> impl Iterator<Item = (&Lit, &String)> {
        self.family_of.iter()
    }

    #[must_use]
    pub fn lit_for(&self, description: &String) -> &Lit {
        self.invconset
            .get(description)
            .expect("IE: Bad constraint name")
    }

    #[must_use]
    pub fn var_lits(&self, lit: &Lit) -> &Vec<Lit> {
        self.varlits_in_con.get(lit).expect("IE: Bad constraint")
    }

    #[must_use]
    pub fn lits(&self) -> &BTreeSet<Lit> {
        &self.lits
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Lit, &String)> {
        self.conset.iter()
    }

    pub fn descriptions(&self) -> impl Iterator<Item = &String> {
        self.invconset.keys()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lits.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lits.is_empty()
    }

    #[must_use]
    pub fn from_raw(
        conset: BTreeMap<Lit, String>,
        invconset: BTreeMap<String, Lit>,
        varlits_in_con: BTreeMap<Lit, Vec<Lit>>,
        family_of: BTreeMap<Lit, String>,
        lits: BTreeSet<Lit>,
    ) -> Self {
        Self {
            conset,
            invconset,
            varlits_in_con,
            family_of,
            lits,
        }
    }
}

// ── DirectEncoding ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DirectEncoding {
    pub litmap: BTreeMap<PuzLit, Lit>,
    pub invlitmap: BTreeMap<Lit, BTreeSet<PuzLit>>,
    pub domainmap: BTreeMap<PuzVar, BTreeSet<i64>>,
}

impl DirectEncoding {
    #[must_use]
    pub fn new() -> Self {
        Self {
            litmap: BTreeMap::new(),
            invlitmap: BTreeMap::new(),
            domainmap: BTreeMap::new(),
        }
    }
}

// ── OrderEncoding ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrderEncoding {
    pub map: BTreeMap<PuzVar, HashSet<Lit>>,
    pub inv_map: BTreeMap<Lit, PuzVar>,
    pub all_lits: BTreeSet<Lit>,
}

impl OrderEncoding {
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
            inv_map: BTreeMap::new(),
            all_lits: BTreeSet::new(),
        }
    }

    pub fn rebuild_all_lits(&mut self) {
        self.all_lits = self.map.values().flatten().copied().collect();
    }
}

// ── VarLitSets ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VarLitSets {
    positive: BTreeSet<Lit>,
    negative: BTreeSet<Lit>,
    special: BTreeSet<Lit>,
}

impl VarLitSets {
    #[must_use]
    pub fn new() -> Self {
        Self {
            positive: BTreeSet::new(),
            negative: BTreeSet::new(),
            special: BTreeSet::new(),
        }
    }

    pub fn insert_positive(&mut self, lit: Lit) {
        self.positive.insert(lit);
    }

    pub fn insert_negative(&mut self, lit: Lit) {
        self.negative.insert(lit);
    }

    pub fn insert_special(&mut self, lit: Lit) {
        self.special.insert(lit);
    }

    #[must_use]
    pub fn positive(&self) -> &BTreeSet<Lit> {
        &self.positive
    }

    #[must_use]
    pub fn negative(&self) -> &BTreeSet<Lit> {
        &self.negative
    }

    #[must_use]
    pub fn special(&self) -> &BTreeSet<Lit> {
        &self.special
    }

    #[must_use]
    pub fn contains(&self, lit: &Lit) -> bool {
        self.positive.contains(lit)
    }

    #[must_use]
    pub fn from_raw(
        positive: BTreeSet<Lit>,
        negative: BTreeSet<Lit>,
        special: BTreeSet<Lit>,
    ) -> Self {
        Self {
            positive,
            negative,
            special,
        }
    }
}

// ── PuzzleParse ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct PuzzleParse {
    pub eprime: EPrimeAnnotations,
    pub satinstance: SatInstance,
    pub cnf: Option<Arc<Cnf>>,
    pub direct: DirectEncoding,
    pub constraints: ConstraintStore,
    pub var_lits: VarLitSets,
    pub order: OrderEncoding,
    pub reveal_map: BTreeMap<Lit, Lit>,
    pub(crate) var_to_cons: BTreeMap<PuzVar, BTreeSet<Lit>>,
}

impl PuzzleParse {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new_from_eprime(
        vars: BTreeSet<String>,
        auxvars: BTreeSet<String>,
        cons: BTreeMap<String, String>,
        reveal: BTreeMap<String, String>,
        params: BTreeMap<String, serde_json::value::Value>,
        kind: Option<String>,
        info: Vec<String>,
        families: BTreeMap<String, Family>,
    ) -> PuzzleParse {
        PuzzleParse {
            eprime: EPrimeAnnotations {
                vars,
                auxvars,
                cons,
                reveal: reveal.clone(),
                reveal_values: reveal.values().cloned().collect(),
                params,
                kind,
                info,
                decs: vec![],
                families,
            },
            satinstance: SatInstance::new(),
            cnf: None,
            direct: DirectEncoding::new(),
            constraints: ConstraintStore::new(),
            var_lits: VarLitSets::new(),
            order: OrderEncoding::new(),
            reveal_map: BTreeMap::new(),
            var_to_cons: BTreeMap::new(),
        }
    }

    fn finalise(&mut self) -> anyhow::Result<()> {
        {
            let mut newlitmap = BTreeMap::new();
            for (key, &value) in &self.direct.litmap {
                if let Some(&val) = self.direct.litmap.get(&key.neg()) {
                    if val != -value {
                        bail!(
                            "Malformed Savilerow DIMACS output: Issue with {:?}",
                            (key, value)
                        );
                    }
                } else {
                    safe_insert(&mut newlitmap, key.neg(), -value)?;
                }
            }
            self.direct.litmap.extend(newlitmap);
        }

        for (key, value) in &self.direct.litmap {
            self.direct
                .invlitmap
                .entry(*value)
                .or_default()
                .insert(key.clone());
        }

        for lit in self.direct.litmap.keys() {
            let var_id = lit.var();
            if lit.sign() {
                self.direct
                    .domainmap
                    .entry(var_id)
                    .or_default()
                    .insert(lit.val());
            }
        }

        for (puzlit, &lit) in &self.direct.litmap {
            let var = puzlit.var();

            let name = var.name();
            if self.eprime.vars.contains(name) {
                self.var_lits.insert_positive(lit);
                if !puzlit.sign() {
                    self.var_lits.insert_negative(lit);
                }
            } else if self.eprime.auxvars.contains(name) || name.starts_with("conjure_aux") {
                if name.starts_with("demystify_") && puzlit.sign() {
                    self.var_lits.insert_special(lit);
                }
            } else if self.eprime.cons.contains_key(name)
                || self.eprime.reveal_values.contains(name)
            {
            } else {
                bail!("Cannot identify {:?}, should it be marked as AUX?", puzlit);
            }
        }

        for (puzlit, &lit) in &self.direct.litmap {
            let var = puzlit.var();
            let name = var.name();
            if self.eprime.reveal.contains_key(name) && puzlit.sign() {
                let mut index = puzlit.varval().var().indices().clone();
                index.push(puzlit.varval().val);

                let target_name = self.eprime.reveal.get(name).unwrap();

                let target_puzvar = PuzVar::new(target_name, index);
                let target_varvalpair = VarValPair::new(&target_puzvar, 1);
                let target_puzlit = PuzLit::new_eq(target_varvalpair);

                if let Some(&target_lit) = self.direct.litmap.get(&target_puzlit) {
                    safe_insert(&mut self.reveal_map, lit, target_lit)
                        .context("Some variable used in two 'REVEAL'")?;
                } else {
                    info!("Can't find {target_puzlit} from {puzlit}");
                }
            }
        }

        let mut usedconstraintnames: HashSet<String> = HashSet::new();

        let fvc = FindVarConnections::new(&self.satinstance, &self.all_var_related_lits());

        for (varid, vals) in &self.direct.domainmap {
            if let Some(template_string) = self.eprime.cons.get(varid.name()) {
                debug!(target: "parser", "Found {:?} in constraint {:?}", varid, varid.name());

                if !vals.contains(&0) {
                    bail!(format!("CON {:?} cannot be made false", varid));
                }

                if !vals.contains(&1) {
                    bail!(format!("CON {:?} cannot be made true", varid));
                }

                if vals.len() != 2 {
                    bail!(format!(
                        "CON {:?} domain is {:?}, should be (0,1)",
                        varid, vals
                    ));
                }

                let constraintname = parsing::parse_constraint_name(
                    template_string,
                    &self.eprime.params,
                    &varid.indices,
                )?;

                if usedconstraintnames.contains(&constraintname) {
                    bail!(format!(
                        "CON name {:?} used twice. This should already have substitutions done, so if you see '{{' or '}}' check your formatting string.",
                        constraintname
                    ))
                }
                usedconstraintnames.insert(constraintname.clone());

                let puzlit = PuzLit::new_eq(VarValPair::new(varid, 1));
                let lit = *self.direct.litmap.get(&puzlit).unwrap();
                let connections = fvc.get_connections(lit);
                if connections.is_empty() {
                    debug!(target: "parser", "Skipping trivial constraint (no variable connections): {}", constraintname);
                    continue;
                }
                info!("MAP {} {:?}", &constraintname, &connections);
                self.constraints
                    .insert(lit, varid.name().clone(), constraintname, connections)?;
            }
        }

        // Remove constraints whose clause sets are identical to an earlier constraint.
        {
            let (cnf, _) = self.satinstance.clone().into_cnf();
            let con_neg_lits: HashSet<Lit> = self.constraints.lits().iter().map(|l| -*l).collect();

            let mut con_to_clauses: BTreeMap<Lit, BTreeSet<Vec<Lit>>> = BTreeMap::new();
            for clause in &cnf {
                let clause_lits: Vec<Lit> = clause.iter().copied().collect();
                for &lit in &clause_lits {
                    if con_neg_lits.contains(&lit) {
                        let con_lit = -lit;
                        let mut filtered: Vec<Lit> =
                            clause_lits.iter().filter(|&&l| l != lit).copied().collect();
                        filtered.sort();
                        con_to_clauses.entry(con_lit).or_default().insert(filtered);
                    }
                }
            }

            let mut seen_clause_sets: HashSet<BTreeSet<Vec<Lit>>> = HashSet::new();
            let mut to_remove: Vec<Lit> = Vec::new();
            for con_lit in self.constraints.lits() {
                if let Some(clause_set) = con_to_clauses.get(con_lit)
                    && !seen_clause_sets.insert(clause_set.clone())
                {
                    to_remove.push(*con_lit);
                }
            }

            if !to_remove.is_empty() {
                eprintln!(
                    "Removing {} duplicate constraints (of {} total)",
                    to_remove.len(),
                    self.constraints.len()
                );
                for lit in &to_remove {
                    self.constraints.remove(lit);
                }
            }
        }

        self.var_to_cons = Self::build_var_to_cons(&self.constraints, &self.direct, &self.order);

        Ok(())
    }

    pub fn build_var_to_cons(
        constraints: &ConstraintStore,
        direct: &DirectEncoding,
        order: &OrderEncoding,
    ) -> BTreeMap<PuzVar, BTreeSet<Lit>> {
        let mut var_to_cons: BTreeMap<PuzVar, BTreeSet<Lit>> = BTreeMap::new();
        for &con_lit in constraints.lits() {
            for vl in constraints.var_lits(&con_lit) {
                if let Some(puzlits) = direct.invlitmap.get(vl) {
                    for pl in puzlits {
                        var_to_cons.entry(pl.var()).or_default().insert(con_lit);
                    }
                }
                if let Some(var) = order.inv_map.get(vl) {
                    var_to_cons.entry(var.clone()).or_default().insert(con_lit);
                }
            }
        }
        var_to_cons
    }

    #[must_use]
    pub fn lit_is_con(&self, lit: &Lit) -> bool {
        self.constraints.contains(lit)
    }

    #[must_use]
    pub fn lit_to_con(&self, lit: &Lit) -> &String {
        self.constraints.description(lit)
    }

    #[must_use]
    pub fn lit_is_var(&self, lit: &Lit) -> bool {
        self.var_lits.contains(lit)
    }

    #[must_use]
    pub fn lit_to_vars(&self, lit: &Lit) -> &BTreeSet<PuzLit> {
        self.direct.invlitmap.get(lit).expect("IE: Bad lit")
    }

    #[must_use]
    pub fn all_var_related_lits(&self) -> HashSet<Lit> {
        let ordered_var: BTreeSet<Lit> = self
            .order
            .map
            .iter()
            .filter(|(k, _)| self.eprime.vars.contains(k.name()))
            .flat_map(|(_, v)| v)
            .copied()
            .collect();

        self.var_lits
            .positive()
            .union(&ordered_var)
            .copied()
            .collect()
    }

    #[must_use]
    pub fn all_var_varvals(&self) -> BTreeSet<VarValPair> {
        self.var_lits
            .positive()
            .iter()
            .flat_map(|x| self.lit_to_vars(x))
            .map(super::PuzLit::varval)
            .collect()
    }

    #[must_use]
    pub fn direct_or_ordered_lit_to_varvalpair(&self, lit: &Lit) -> BTreeSet<VarValPair> {
        let direct_lits = self.direct.invlitmap.get(lit).cloned().unwrap_or_default();

        let order_lits = if let Some(var) = self.order.inv_map.get(lit) {
            self.direct
                .domainmap
                .get(var)
                .unwrap()
                .iter()
                .map(|&d| VarValPair::new(var, d))
                .collect_vec()
        } else {
            vec![]
        };

        direct_lits
            .into_iter()
            .map(|x| x.varval())
            .chain(order_lits)
            .collect()
    }

    #[must_use]
    pub fn has_facts(&self) -> bool {
        !self.eprime.reveal.is_empty()
    }

    #[must_use]
    pub fn constraint_names(&self) -> BTreeSet<String> {
        self.constraints.descriptions().cloned().collect()
    }

    pub fn constraint_roots(&self) -> BTreeSet<String> {
        self.eprime.cons.keys().cloned().collect()
    }

    #[must_use]
    pub fn constraint_scope(&self, con: &String) -> BTreeSet<VarValPair> {
        let lit = self.constraints.lit_for(con);
        let lits = self.constraints.var_lits(lit);
        let puzlits = lits
            .iter()
            .flat_map(|l| self.direct_or_ordered_lit_to_varvalpair(l))
            .collect_vec();

        BTreeSet::from_iter(puzlits)
    }

    /// Returns the constraint Lits whose scope mentions the variable of the given literal.
    /// Falls back to all constraints if the variable isn't found.
    #[must_use]
    pub fn cons_for_var_lit(&self, lit: &Lit) -> BTreeSet<Lit> {
        if let Some(puzlits) = self.direct.invlitmap.get(lit) {
            let mut cons = BTreeSet::new();
            for pl in puzlits {
                if let Some(con_lits) = self.var_to_cons.get(&pl.var()) {
                    cons.extend(con_lits);
                }
            }
            if !cons.is_empty() {
                return cons;
            }
        }
        self.constraints.lits().clone()
    }

    pub fn filter_out_constraint(&mut self, con: &str) {
        assert!(
            self.eprime.cons.contains_key(con),
            "Filtered constraint is not present: {con}"
        );
        let invlitmap = &self.direct.invlitmap;
        let old_len = self.constraints.len();
        self.constraints.retain(|l| {
            let puzvars = invlitmap.get(l).unwrap();
            !puzvars.iter().all(|p| p.var().name() == con)
        });
        eprintln!(
            "Removing {}: {} -> {}",
            con,
            old_len,
            self.constraints.len()
        );
    }

    #[must_use]
    pub fn get_matrix_indices(&self, var: &str) -> Option<Vec<i64>> {
        let mut domain: Option<Vec<i64>> = None;
        for key in self.direct.domainmap.keys() {
            if key.name() == var {
                let indices = key.indices();
                if let Some(mut current_domain) = domain {
                    for i in 0..current_domain.len() {
                        current_domain[i] = max(current_domain[i], indices[i]);
                    }
                    domain = Some(current_domain);
                } else {
                    domain = Some(indices.clone());
                }
            }
        }
        domain
    }
}

#[derive(Debug)]
struct ParsedEprimeData {
    vars: BTreeSet<String>,
    auxvars: BTreeSet<String>,
    cons: BTreeMap<String, String>,
    factvars: BTreeMap<String, String>,
    kind: Option<String>,
    info: Vec<String>,
    decs: Vec<String>,
    families: BTreeMap<String, Family>,
}

#[derive(Debug, PartialEq)]
enum FamilyToken {
    Bare(String),
    Quoted(String),
}

/// Tokenise a `$#FAMILY` body, distinguishing bare identifiers from
/// double-quoted labels.  No escape sequences are supported inside quotes
/// (an embedded `"` ends the string).  Whitespace separates tokens.
fn tokenize_family_body(s: &str) -> anyhow::Result<Vec<FamilyToken>> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        match chars.peek() {
            None => break,
            Some(&'"') => {
                chars.next();
                let mut buf = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '"' {
                        closed = true;
                        break;
                    }
                    buf.push(c);
                }
                if !closed {
                    bail!("unterminated quoted string in $#FAMILY directive");
                }
                out.push(FamilyToken::Quoted(buf));
            }
            Some(_) => {
                let mut buf = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '"' {
                        break;
                    }
                    buf.push(c);
                    chars.next();
                }
                out.push(FamilyToken::Bare(buf));
            }
        }
    }
    Ok(out)
}

/// Pop the next token from `iter` if it is a quoted label, returning the
/// label string. Returns `None` when the next token is bare or absent —
/// the caller should fall back to a default label in that case.
fn next_label(iter: &mut std::iter::Peekable<std::vec::IntoIter<FamilyToken>>) -> Option<String> {
    match iter.next_if(|t| matches!(t, FamilyToken::Quoted(_)))? {
        FamilyToken::Quoted(s) => Some(s),
        FamilyToken::Bare(_) => unreachable!("filtered by next_if above"),
    }
}

/// Parse the body of a `$#FAMILY` directive (everything after the directive
/// name) into a `(group_id, Family)` pair.
///
/// Format: `<group> ["group label"] <member1> ["label1"] <member2> ["label2"] ...`
/// Quoted strings are labels (optional); bare tokens are identifiers.
fn parse_family_line(rest: &str) -> anyhow::Result<(String, Family)> {
    let tokens = tokenize_family_body(rest)?;
    let mut iter = tokens.into_iter().peekable();

    let group_id = match iter.next() {
        Some(FamilyToken::Bare(s)) => s,
        Some(FamilyToken::Quoted(_)) => {
            bail!("$#FAMILY: group id must be a bare identifier, not a quoted label")
        }
        None => bail!("$#FAMILY: empty directive"),
    };

    let group_label = next_label(&mut iter).unwrap_or_else(|| group_id.clone());

    let mut members: BTreeMap<String, String> = BTreeMap::new();
    while let Some(tok) = iter.next() {
        let member_id = match tok {
            FamilyToken::Bare(s) => s,
            FamilyToken::Quoted(_) => {
                bail!("$#FAMILY: unexpected quoted label where a member id was expected")
            }
        };
        let member_label = next_label(&mut iter).unwrap_or_else(|| member_id.clone());
        if members.contains_key(&member_id) {
            bail!("$#FAMILY '{group_id}': member '{member_id}' declared twice");
        }
        members.insert(member_id, member_label);
    }

    if members.is_empty() {
        bail!("$#FAMILY '{group_id}': no members specified");
    }

    Ok((
        group_id,
        Family {
            label: group_label,
            members,
        },
    ))
}

fn parse_eprime_file(in_path: &PathBuf) -> anyhow::Result<ParsedEprimeData> {
    info!(target: "parser", "reading DIMACS {:?}", in_path);

    let mut vars: BTreeSet<String> = BTreeSet::new();
    let mut puzzle: BTreeSet<String> = BTreeSet::new();

    let mut auxvars: BTreeSet<String> = BTreeSet::new();

    let mut cons: BTreeMap<String, String> = BTreeMap::new();

    let mut factvars: BTreeMap<String, String> = BTreeMap::new();

    let mut info: Vec<String> = Vec::new();
    let mut decs: Vec<String> = Vec::new();
    let mut families: BTreeMap<String, Family> = BTreeMap::new();

    let mut kind: Option<String> = None;

    let conmatch = Regex::new(r#"\$#CON (.*) "(.*)" *$"#).unwrap();

    let file = File::open(in_path)?;
    let reader = io::BufReader::new(file);

    let mut all_names = HashSet::new();

    for line in reader.lines() {
        let line = line?;

        if line.contains("$#") {
            debug!(target: "parser", "line {:?}", line);
            let parts: Vec<&str> = line.split_whitespace().collect();

            if line.starts_with("$#VAR") {
                let v = parts[1].to_string();
                info!(target: "parser", "Found VAR: '{}'", v);

                if all_names.contains(&v) {
                    bail!(format!("{v} defined twice"));
                }
                all_names.insert(v.clone());

                vars.insert(v);
            } else if line.starts_with("$#PUZZLE") {
                let v = parts[1].to_string();
                info!(target: "parser", "Found PUZZLE: '{}'", v);

                if all_names.contains(&v) {
                    bail!(format!("{v} defined twice"));
                }
                all_names.insert(v.clone());

                puzzle.insert(v);
            } else if line.starts_with("$#CON") {
                info!(target: "parser", "{}", line);
                let captures = conmatch
                    .captures(&line)
                    .context(format!("Malformed $#CON line: {line}"))?;

                let con_name = captures.get(1).unwrap().as_str().to_string();
                let con_value = captures.get(2).unwrap().as_str().to_string();

                info!(target: "parser", "Found CON: '{}' '{}'", con_name, con_value);

                if all_names.contains(&con_name) {
                    bail!(format!("{con_name} defined twice"));
                }
                all_names.insert(con_name.clone());

                if cons.contains_key(&con_name) {
                    bail!(format!("{} defined twice", con_name));
                }
                safe_insert(&mut cons, con_name, con_value)?;
            } else if line.starts_with("$#AUX") {
                let v = parts[1].to_string();
                info!(target: "parser", "Found Aux VAR: '{}'", v);

                if all_names.contains(&v) {
                    bail!(format!("{v} defined twice"));
                }
                all_names.insert(v.clone());

                auxvars.insert(v);
            } else if line.starts_with("$#KIND") {
                let v = parts[1].to_string();
                if kind.is_some() {
                    bail!("Cannot have two 'KIND' statements");
                }
                kind = Some(v);
            } else if line.starts_with("$#INFO") {
                // Collect the rest of the line after $#INFO as an info string
                let info_string = line.strip_prefix("$#INFO").unwrap_or("").trim().to_string();
                info!(target: "parser", "Found INFO: '{}'", info_string);
                info.push(info_string);
            } else if line.starts_with("$#DEC ") {
                let dec_string = line.strip_prefix("$#DEC ").unwrap_or("").trim().to_string();
                info!(target: "parser", "Found DEC: '{}'", dec_string);
                decs.push(dec_string);
            } else if line.starts_with("$#FAMILY ") {
                let rest = line.strip_prefix("$#FAMILY ").unwrap_or("");
                let (group_id, family) = parse_family_line(rest)
                    .with_context(|| format!("Failed to parse $#FAMILY line: {line}"))?;
                info!(target: "parser",
                    "Found FAMILY: group '{}' label '{}' members {:?}",
                    group_id, family.label, family.members);
                if families.contains_key(&group_id) {
                    bail!(format!("FAMILY group '{group_id}' defined twice"));
                }
                families.insert(group_id, family);
            } else if line.starts_with("$#REVEAL ") {
                if parts.len() != 3 {
                    bail!(format!(
                        "Invalid format, should be $#REVEAL <orig> <reveal> : {line} > {parts:?}"
                    ));
                }

                let key = parts[1].to_owned();
                let value = parts[2].to_owned();

                if !vars.contains(&key) {
                    bail!(format!(
                        "{key} from a REVEAL must be first be defined as a VAR"
                    ));
                }

                if all_names.contains(&value) {
                    bail!(format!("{value} defined twice"));
                }
                all_names.insert(value.clone());

                safe_insert(&mut factvars, key, value)?;
            } else {
                bail!(format!("Do not understand line '{line}'"));
            }
        }

        for name in &all_names {
            for other in &all_names {
                if name != other && (name.starts_with(other) || other.starts_with(name)) {
                    bail!(format!(
                        "Cannot have one name be a prefix of another: {name} and {other}"
                    ));
                }
            }
        }
    }

    // Validate that every $#FAMILY member id refers to a declared $#CON
    // constraint name.  Catches typos like `$#FAMILY g row_alldif` (missing
    // an 'f') that would otherwise silently never match anything.
    for (group_id, family) in &families {
        for member_id in family.members.keys() {
            if !cons.contains_key(member_id) {
                bail!(
                    "$#FAMILY '{group_id}': member '{member_id}' is not a declared \
                     $#CON constraint (declared: {:?})",
                    cons.keys().collect::<Vec<_>>()
                );
            }
        }
    }

    info!(target: "parser", "Names parsed from ESSENCE': vars: {:?} auxvars: {:?} cons {:?}", vars, auxvars, cons);

    Ok(ParsedEprimeData {
        vars,
        auxvars,
        cons,
        factvars,
        kind,
        info,
        decs,
        families,
    })
}

fn parse_eprime(in_path: &PathBuf, eprimeparam: &PathBuf) -> anyhow::Result<PuzzleParse> {
    let parsed_eprime = parse_eprime_file(in_path)?;
    let params = read_essence_param(eprimeparam)?;

    let mut puzzleparse = PuzzleParse::new_from_eprime(
        parsed_eprime.vars,
        parsed_eprime.auxvars,
        parsed_eprime.cons,
        parsed_eprime.factvars,
        params,
        parsed_eprime.kind,
        parsed_eprime.info,
        parsed_eprime.families,
    );
    puzzleparse.eprime.decs = parsed_eprime.decs;
    Ok(puzzleparse)
}

type DimacsMaps = (
    BTreeMap<PuzLit, Lit>,
    BTreeMap<PuzVar, HashSet<Lit>>,
    BTreeMap<Lit, PuzVar>,
);

fn read_dimacs_to_maps(in_path: &PathBuf) -> anyhow::Result<DimacsMaps> {
    let dvarmatch = Regex::new(r"c Var '(.*)' direct represents '(.*)' with '(.*)'").unwrap();
    let ovarmatch = Regex::new(r"c Var '(.*)' order represents '(.*)' with '(.*)'").unwrap();

    let file = File::open(in_path)?;
    let reader = io::BufReader::new(file);

    let mut litmap = BTreeMap::new();
    let mut order_encoding_map: BTreeMap<PuzVar, HashSet<Lit>> = BTreeMap::new();
    let mut inv_order_encoding_map = BTreeMap::new();

    for line in reader.lines() {
        let line = line?;
        if line.starts_with("c Var") {
            let dmatch = dvarmatch.captures(&line);
            let omatch = ovarmatch.captures(&line);
            if !(dmatch.is_some() || omatch.is_some()) {
                bail!("Failed to parse '{:?}'", line);
            }

            if let Some(match_) = dmatch {
                let litval = match_[3].parse::<i64>().unwrap();

                if !match_[1].starts_with("aux") && litval != 9_223_372_036_854_775_807 {
                    let satlit = Lit::from_ipasir(
                        i32::try_from(litval)
                            .with_context(|| format!("Number too large: {litval}"))?,
                    )?;
                    let varid = crate::problem::util::parsing::parse_savile_row_name(&match_[1])
                        .with_context(|| {
                            format!("Failed parsing savile row name {}", &match_[1])
                        })?;
                    if let Some(varid) = varid {
                        let puzlit = PuzLit::new_eq(VarValPair::new(
                            &varid,
                            match_[2].parse::<i64>().unwrap(),
                        ));
                        safe_insert(&mut litmap, puzlit, satlit)?;
                    }
                }
            } else {
                let match_ = omatch.unwrap();
                let litval = match_[3].parse::<i64>().unwrap();
                info!(target: "parser", "matches: {:?}", match_);
                if !match_[1].starts_with("aux") && litval != 9_223_372_036_854_775_807 {
                    let satlit = Lit::from_ipasir(i32::try_from(litval)?)?;
                    let varid = crate::problem::util::parsing::parse_savile_row_name(&match_[1])
                        .with_context(|| {
                            format!("Failed parsing savile row name {}", &match_[1])
                        })?;

                    if let Some(varid) = varid {
                        order_encoding_map
                            .entry(varid.clone())
                            .or_default()
                            .insert(satlit);
                        order_encoding_map
                            .entry(varid.clone())
                            .or_default()
                            .insert(-satlit);
                        if let Some(val) = inv_order_encoding_map.get(&satlit)
                            && *val != varid
                        {
                            bail!("{} used for two variables: {} {}", satlit, val, varid);
                        }
                        safe_insert(&mut inv_order_encoding_map, satlit, varid.clone())?;
                        safe_insert(&mut inv_order_encoding_map, -satlit, varid.clone())?;
                    }
                }
            }
        }
    }

    Ok((litmap, order_encoding_map, inv_order_encoding_map))
}

fn update_puzzle_parse_with_maps(
    dimacs: &mut PuzzleParse,
    litmap: BTreeMap<PuzLit, Lit>,
    order_encoding_map: BTreeMap<PuzVar, HashSet<Lit>>,
    inv_order_encoding_map: BTreeMap<Lit, PuzVar>,
) -> anyhow::Result<()> {
    dimacs.direct.litmap = litmap;
    dimacs.order.map = order_encoding_map;
    dimacs.order.inv_map = inv_order_encoding_map;
    dimacs.order.rebuild_all_lits();
    Ok(())
}

fn read_dimacs(in_path: &PathBuf, dimacs: &mut PuzzleParse) -> anyhow::Result<()> {
    let (litmap, order_encoding_map, inv_order_encoding_map) = read_dimacs_to_maps(in_path)?;
    update_puzzle_parse_with_maps(dimacs, litmap, order_encoding_map, inv_order_encoding_map)
}

pub fn parse_essence(eprimein: &PathBuf, eprimeparamin: &PathBuf) -> anyhow::Result<PuzzleParse> {
    let t_total = std::time::Instant::now();

    let tdir = tempfile::Builder::new()
        .prefix(".demystify-")
        .tempdir_in(".")
        .unwrap();

    let eprime = tdir.path().join(eprimein.file_name().unwrap());
    let eprimeparam = tdir.path().join(eprimeparamin.file_name().unwrap());

    fs::copy(eprimein, &eprime)?;
    fs::copy(eprimeparamin, &eprimeparam)?;

    info!("Parsing Essence in TempDir: {tdir:?}");

    let finaleprime: PathBuf;
    let finaleprimeparam: PathBuf;

    info!(target: "parser", "Handling {:?}", eprime);

    // Check if file extension is .essence
    let is_essence = eprime.extension().is_some_and(|ext| ext == "essence");

    // If input is essence, translate to essence' for savilerow
    if is_essence {
        info!(target: "parser", "Running {:?} {:?} through conjure", eprime, eprimeparam);
        let output = ProgramRunner::prepare("conjure", tdir.path())
            .arg("solve")
            .arg("-o")
            .arg(".")
            .arg(eprime.file_name().unwrap())
            .arg(eprimeparam.file_name().unwrap())
            .output()
            .expect("Failed to execute command");

        if !output.status.success() {
            bail!(format!(
                "conjure failed\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        finaleprime = tdir.path().join("model000001.eprime");

        let option_param = fs::read_dir(tdir.path())
            .unwrap()
            .filter_map(Result::ok)
            .find(|d| d.path().extension().and_then(|s| s.to_str()) == Some("param"))
            .map(|d| d.path());

        if let Some(fname) = option_param {
            finaleprimeparam = fname;
        } else {
            bail!("Could not find 'param' file generated by SavileRow");
        }
    } else {
        finaleprime = eprime.clone();
        finaleprimeparam = eprimeparam.clone();
    }

    info!(target: "parser", "Running savilerow on {:?} {:?}", finaleprime, finaleprimeparam);

    let makedimacs = ProgramRunner::prepare("savilerow", tdir.path())
        .arg("-in-eprime")
        .arg(finaleprime.file_name().unwrap())
        .arg("-in-param")
        .arg(finaleprimeparam.file_name().unwrap())
        .arg("-sat-output-mapping")
        .arg("-sat")
        .arg("-sat-family")
        .arg("lingeling")
        .arg("-S0")
        .arg("-O0")
        .arg("-reduce-domains")
        .arg("-aggregate")
        .output()
        .expect("Failed to find 'savilerow' -- have you installed savilerow and conjure?");

    if !makedimacs.status.success() {
        bail!(
            "savilerow failed. The most likely reason for this is your file is malformed.\n{}\n{}",
            String::from_utf8_lossy(&makedimacs.stdout),
            String::from_utf8_lossy(&makedimacs.stderr)
        );
    }

    let conjure_secs = t_total.elapsed().as_secs_f64();
    info!(target: "progress", "conjure/savilerow completed in {conjure_secs:.2}s");
    let t_setup = std::time::Instant::now();

    let original_input_path = PathBuf::from(&eprime);

    // Need to put '.dimacs' on the end in this slightly horrible way.
    let in_dimacs_path = PathBuf::from(finaleprimeparam.to_str().unwrap().to_owned() + ".dimacs");

    let mut eprimeparse = parse_eprime(&original_input_path, &finaleprimeparam)?;

    eprimeparse.satinstance =
        instances::SatInstance::<BasicVarManager>::from_dimacs_path(&in_dimacs_path)
            .context("reading dimacs")?;

    eprimeparse.cnf = Some(Arc::new(eprimeparse.satinstance.clone().into_cnf().0));

    read_dimacs(&in_dimacs_path, &mut eprimeparse).context("reading variable info from dimacs")?;

    eprimeparse.finalise().context("finalisation of parsing failed. The most likely reason for this is you gave a puzzle which has no solutions!")?;

    let setup_secs = t_setup.elapsed().as_secs_f64();
    let total_secs = t_total.elapsed().as_secs_f64();
    info!(target: "progress", "parse setup completed in {setup_secs:.2}s (total parse: {total_secs:.2}s)");

    Ok(eprimeparse)
}

fn read_essence_param(
    eprimeparam: &PathBuf,
) -> anyhow::Result<BTreeMap<String, serde_json::value::Value>> {
    if eprimeparam.ends_with(".json") {
        info!(target: "parser", "Reading params {:?} as json", eprimeparam);
        let file = fs::File::open(eprimeparam).unwrap();
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).context("Failed reading json param file")
    } else {
        pretty_print_essence(eprimeparam, "json")
    }
}

fn pretty_print_essence(
    file: &PathBuf,
    format: &str,
) -> anyhow::Result<BTreeMap<String, serde_json::value::Value>> {
    let tdir = tempfile::Builder::new()
        .prefix(".demystify-")
        .tempdir_in(".")
        .unwrap();
    let temp_file = tdir.path().join(file.file_name().unwrap());
    fs::copy(file, &temp_file)?;

    info!(target: "parser", "Pretty printing {:?} as {}", temp_file, format);
    let output = ProgramRunner::prepare("conjure", tdir.path())
        .arg("pretty")
        .arg("--output-format")
        .arg(format)
        .arg(temp_file.file_name().unwrap())
        .output()
        .expect("Failed to execute command");

    if !output.status.success() {
        bail!(format!(
            "Conjure pretty-printing failed\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    serde_json::from_slice(&output.stdout).context("Failed to parse JSON produced by conjure")
}

#[cfg(test)]
mod tests {

    use test_log::test;

    use super::pretty_print_essence;

    use std::{collections::BTreeSet, path::PathBuf};

    #[test]
    fn test_parse_essence_binairo() {
        let eprime_path = "./tst/binairo.eprime";
        let eprimeparam_path = "./tst/binairo-1.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

        assert!(!puz.has_facts());

        assert!(!puz.eprime.has_param("q"));

        assert!(puz.eprime.has_param("n"));

        assert_eq!(puz.eprime.param_i64("n").unwrap(), 6);

        assert!(puz.eprime.has_param("start_grid"));

        assert!(puz.eprime.param_vec_i64("start_grid").is_err());

        let initial: Vec<Vec<i64>> = puz.eprime.param_vec_vec_i64("start_grid").unwrap();

        assert_eq!(initial[0], vec![2, 2, 2, 0, 0, 2]);
        assert_eq!(initial[5], vec![2, 0, 2, 2, 1, 1]);
    }

    #[test]
    fn test_parse_essence_minesweeper() {
        let eprime_path = "./tst/minesweeper.eprime";
        let eprimeparam_path = "./tst/minesweeperPrinted.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

        assert!(puz.has_facts());

        assert!(!puz.eprime.has_param("q"));

        assert!(puz.eprime.has_param("width"));

        assert_eq!(puz.eprime.param_i64("width").unwrap(), 5);
    }

    #[test]
    fn test_filter_constraint_binairo() {
        let eprime_path = "./tst/binairo.eprime";
        let eprimeparam_path = "./tst/binairo-1.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

        let mut filter1_puz = puz.clone();

        filter1_puz.filter_out_constraint("rowwhite");

        assert_eq!(puz.constraints.len() - filter1_puz.constraints.len(), 5);
    }

    #[test]
    #[should_panic]
    fn test_filter_constraint_fail_binairo() {
        let eprime_path = "./tst/binairo.eprime";
        let eprimeparam_path = "./tst/binairo-1.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

        let mut filter_fail_puz = puz.clone();

        filter_fail_puz.filter_out_constraint("row");
    }

    #[test]
    fn test_parse_essence_little() {
        let eprime_path = "./tst/little1.eprime";
        let eprimeparam_path = "./tst/little1.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

        assert_eq!(puz.eprime.vars.len(), 1);
        assert_eq!(puz.eprime.cons.len(), 1);
        assert_eq!(puz.eprime.auxvars.len(), 0);
        assert_eq!(puz.eprime.kind, Some("Tiny".to_string()));

        assert!(puz.eprime.has_param("n"));

        assert_eq!(puz.eprime.param_i64("n").unwrap(), 4);

        assert!(puz.eprime.param_bool("n").is_err());

        assert!(!puz.eprime.param_bool("b1").unwrap());
        assert!(puz.eprime.param_bool("b2").unwrap());

        assert_eq!(puz.eprime.param_vec_i64("l").unwrap(), vec![2, 4, 6, 8]);

        assert_eq!(
            puz.eprime.param_vec_string("l").unwrap(),
            vec!["2", "4", "6", "8"]
        );

        assert_eq!(
            puz.eprime.param_vec_vec_string("l2").unwrap(),
            vec![vec!["1", "2"], vec!["3", "4"]]
        );

        assert_eq!(
            puz.eprime.param_vec_string("lb").unwrap(),
            vec!["false", "false", "true", "false"]
        );

        assert_eq!(
            puz.eprime.param_vec_vec_string("lb2").unwrap(),
            vec![vec!["false", "true"], vec!["true", "false"]]
        );

        assert_eq!(puz.constraints.len(), 3);
        assert_eq!(puz.var_lits.positive().len(), 4 * 4 * 2); // 4 variables, 4 domain values, 2 pos+neg lits
        let cons = puz.constraint_names();

        assert!(puz.constraints.lits().iter().all(|l| puz.lit_is_con(l)));
        assert!(puz.var_lits.positive().iter().all(|l| !puz.lit_is_con(l)));
        assert!(puz.constraints.lits().iter().all(|l| !puz.lit_is_var(l)));
        assert!(puz.var_lits.positive().iter().all(|l| puz.lit_is_var(l)));

        let scopes: Vec<_> = cons.iter().map(|c| (c, puz.constraint_scope(c))).collect();

        insta::assert_debug_snapshot!(scopes);
    }

    #[test]
    fn test_parse_sudoku_little() {
        let eprime_path = "./tst/little-sudoku.eprime";
        let eprimeparam_path = "./tst/little-sudoku.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

        assert_eq!(puz.eprime.vars.len(), 1);
        assert_eq!(puz.eprime.cons.len(), 1);
        assert_eq!(puz.eprime.auxvars.len(), 0);
        assert_eq!(puz.eprime.kind, Some("Sudoku".to_string()));

        assert!(puz.eprime.has_param("n"));

        assert_eq!(puz.eprime.param_i64("n").unwrap(), 3);

        let cons = puz.constraint_names();

        let scopes: Vec<_> = cons.iter().map(|c| (c, puz.constraint_scope(c))).collect();

        insta::assert_debug_snapshot!(scopes);
    }

    #[test]
    fn test_parse_sudoku_little_2() {
        let eprime_path = "./tst/little-sudoku-2.eprime";
        let eprimeparam_path = "./tst/little-sudoku-2.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

        assert_eq!(puz.eprime.vars.len(), 1);
        assert_eq!(puz.eprime.cons.len(), 1);
        assert_eq!(puz.eprime.auxvars.len(), 0);
        assert_eq!(puz.eprime.kind, Some("Sudoku".to_string()));

        assert!(puz.eprime.has_param("n"));

        assert_eq!(puz.eprime.param_i64("n").unwrap(), 3);

        let cons = puz.constraint_names();

        let scopes: Vec<_> = cons.iter().map(|c| (c, puz.constraint_scope(c))).collect();

        insta::assert_debug_snapshot!(scopes);
    }

    #[test]
    fn pretty_print() {
        let eprime_path = "./tst/binairo.eprime";
        let parse = pretty_print_essence(&PathBuf::from(eprime_path), "astjson");
        // Do not want to output the whole tree
        let k: BTreeSet<_> = parse.unwrap().keys().cloned().collect();
        insta::assert_debug_snapshot!(k);
    }

    use std::io::Write;

    #[test]
    fn test_parse_info_directives() {
        // Create a temporary eprime file with INFO directives
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#VAR y").unwrap();
        writeln!(temp_file, "$#CON con1 \"x != y\"").unwrap();
        writeln!(temp_file, "$#KIND sudoku").unwrap();
        writeln!(temp_file, "$#INFO This is a test info message").unwrap();
        writeln!(temp_file, "$#INFO Another info message with spaces").unwrap();
        writeln!(temp_file, "$#INFO Info with special chars: !@#$%^&*()").unwrap();

        let path = temp_file.path().to_path_buf();

        // Parse the file
        let result = super::parse_eprime_file(&path);

        assert!(result.is_ok(), "Parsing should succeed");
        let parsed = result.unwrap();

        // Verify the info field contains our messages
        assert_eq!(parsed.info.len(), 3, "Should have 3 info messages");
        assert_eq!(parsed.info[0], "This is a test info message");
        assert_eq!(parsed.info[1], "Another info message with spaces");
        assert_eq!(parsed.info[2], "Info with special chars: !@#$%^&*()");

        // Verify other fields are still parsed correctly
        assert_eq!(parsed.vars.len(), 2);
        assert!(parsed.vars.contains("x"));
        assert!(parsed.vars.contains("y"));
        assert_eq!(parsed.cons.len(), 1);
        assert!(parsed.cons.contains_key("con1"));
        assert_eq!(parsed.kind, Some("sudoku".to_string()));
    }

    #[test]
    fn test_parse_empty_info() {
        // Create a temporary eprime file without INFO directives
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#KIND puzzle").unwrap();

        let path = temp_file.path().to_path_buf();

        // Parse the file
        let result = super::parse_eprime_file(&path);

        assert!(result.is_ok(), "Parsing should succeed");
        let parsed = result.unwrap();

        // Verify the info field is empty
        assert_eq!(parsed.info.len(), 0, "Should have no info messages");
    }

    #[test]
    fn test_parse_family_directives_with_labels() {
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#CON row_alldiff \"row\"").unwrap();
        writeln!(temp_file, "$#CON con_alldiff \"col\"").unwrap();
        writeln!(temp_file, "$#CON box_alldiff \"box\"").unwrap();
        writeln!(
            temp_file,
            "$#FAMILY unit_alldiff \"Unit all-different\" row_alldiff \"Row\" con_alldiff \"Column\" box_alldiff \"Box\""
        )
        .unwrap();

        let path = temp_file.path().to_path_buf();
        let parsed = super::parse_eprime_file(&path).expect("parsing should succeed");

        let f = parsed.families.get("unit_alldiff").expect("group present");
        assert_eq!(f.label, "Unit all-different");
        assert_eq!(f.members.len(), 3);
        assert_eq!(f.members.get("row_alldiff"), Some(&"Row".to_string()));
        assert_eq!(f.members.get("con_alldiff"), Some(&"Column".to_string()));
        assert_eq!(f.members.get("box_alldiff"), Some(&"Box".to_string()));
    }

    #[test]
    fn test_parse_family_no_labels_defaults_to_ids() {
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#CON a \"a\"").unwrap();
        writeln!(temp_file, "$#CON b \"b\"").unwrap();
        writeln!(temp_file, "$#CON c \"c\"").unwrap();
        writeln!(temp_file, "$#FAMILY g a b c").unwrap();

        let path = temp_file.path().to_path_buf();
        let parsed = super::parse_eprime_file(&path).expect("parsing should succeed");

        let f = parsed.families.get("g").expect("group present");
        assert_eq!(f.label, "g", "group label defaults to group id");
        assert_eq!(f.members.get("a"), Some(&"a".to_string()));
        assert_eq!(f.members.get("b"), Some(&"b".to_string()));
        assert_eq!(f.members.get("c"), Some(&"c".to_string()));
    }

    #[test]
    fn test_parse_family_mixed_labels() {
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#CON a \"a\"").unwrap();
        writeln!(temp_file, "$#CON b \"b\"").unwrap();
        writeln!(temp_file, "$#CON c \"c\"").unwrap();
        // Group label given, member b labelled, members a and c not labelled.
        writeln!(temp_file, "$#FAMILY g \"Group G\" a b \"Bee\" c").unwrap();

        let path = temp_file.path().to_path_buf();
        let parsed = super::parse_eprime_file(&path).expect("parsing should succeed");

        let f = parsed.families.get("g").expect("group present");
        assert_eq!(f.label, "Group G");
        assert_eq!(f.members.get("a"), Some(&"a".to_string()));
        assert_eq!(f.members.get("b"), Some(&"Bee".to_string()));
        assert_eq!(f.members.get("c"), Some(&"c".to_string()));
    }

    #[test]
    fn test_parse_family_label_with_spaces() {
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#CON a \"a\"").unwrap();
        writeln!(
            temp_file,
            "$#FAMILY g \"This label has spaces\" a \"Another spaced label\""
        )
        .unwrap();

        let path = temp_file.path().to_path_buf();
        let parsed = super::parse_eprime_file(&path).expect("parsing should succeed");

        let f = parsed.families.get("g").expect("group present");
        assert_eq!(f.label, "This label has spaces");
        assert_eq!(
            f.members.get("a"),
            Some(&"Another spaced label".to_string())
        );
    }

    #[test]
    fn test_parse_family_unterminated_quote_fails() {
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#FAMILY g \"unterminated").unwrap();

        let path = temp_file.path().to_path_buf();
        let result = super::parse_eprime_file(&path);
        assert!(result.is_err(), "Unterminated quote should fail");
    }

    #[test]
    fn test_parse_family_duplicate_group_fails() {
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#FAMILY g a b").unwrap();
        writeln!(temp_file, "$#FAMILY g c d").unwrap();

        let path = temp_file.path().to_path_buf();
        let result = super::parse_eprime_file(&path);
        assert!(result.is_err(), "Duplicate family group should fail");
    }

    #[test]
    fn test_parse_family_no_members_fails() {
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#FAMILY only_group").unwrap();

        let path = temp_file.path().to_path_buf();
        let result = super::parse_eprime_file(&path);
        assert!(result.is_err(), "FAMILY with no members should fail");
    }

    #[test]
    fn test_parse_family_duplicate_member_fails() {
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#CON a \"a\"").unwrap();
        writeln!(temp_file, "$#FAMILY g a a").unwrap();

        let path = temp_file.path().to_path_buf();
        let result = super::parse_eprime_file(&path);
        assert!(result.is_err(), "Duplicate member should fail");
    }

    #[test]
    fn test_parse_family_unknown_member_fails() {
        // $#FAMILY referencing a member that wasn't declared as $#CON should
        // be caught at parse time, not silently accepted.
        let mut temp_file = tempfile::Builder::new()
            .prefix(".demystify-")
            .tempfile_in(".")
            .unwrap();
        writeln!(temp_file, "$#VAR x").unwrap();
        writeln!(temp_file, "$#CON real_con \"real\"").unwrap();
        writeln!(temp_file, "$#FAMILY g real_con typo_con").unwrap();

        let path = temp_file.path().to_path_buf();
        let result = super::parse_eprime_file(&path);
        assert!(
            result.is_err(),
            "FAMILY referencing undeclared $#CON should fail"
        );
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("typo_con"),
            "error message should name the missing constraint, got: {err}"
        );
    }
}
