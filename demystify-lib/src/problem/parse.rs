#![allow(clippy::needless_range_loop)]
/// This module contains the implementation of parsing and processing functions for problem files.
/// It includes functions for parsing DIMACS files, extracting annotations from Essence' files,
/// and creating data structures to represent the parsed information.
///
/// The main struct in this module is `PuzzleParse`, which represents the result of parsing a DIMACS file.
/// It contains various fields to store the parsed information, such as the annotations from the Essence' file,
/// the SAT instance parsed from the DIMACS file, mappings between literals and SAT integers, and more.
use anyhow::{bail, Context};
use itertools::Itertools;
use regex::Regex;
use rustsat::instances::{self, BasicVarManager, Cnf, SatInstance};
use rustsat::types::Lit;

use std::collections::{BTreeMap, BTreeSet, HashSet};

use std::fs;
use std::io::prelude::*;
use std::io::BufReader;

use std::mem::forget;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tempfile::TempDir;
use tracing::{debug, info};

use std::fs::File;
use std::io;

use crate::problem::util::parsing;
use crate::problem::{PuzLit, PuzVar};

use super::util::FindVarConnections;
use super::VarValPair;

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
    params: BTreeMap<String, serde_json::value::Value>,
    /// The kind of puzzle
    pub kind: Option<String>,
}

impl EPrimeAnnotations {
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
}

/// Represents the result of parsing a DIMACS file.

#[derive(Debug, Clone, PartialEq)]

pub struct PuzzleParse {
    /// The annotations from the Essence' file
    pub eprime: EPrimeAnnotations,
    /// The SAT instance parsed from the DIMACS file.
    pub satinstance: SatInstance,
    // A Copy of the CNF of the SAT instance (as we frequently need this)
    pub cnf: Option<Arc<Cnf>>,
    /// A mapping from literals in the direct representation to their corresponding SAT integer.
    pub litmap: BTreeMap<PuzLit, Lit>,
    /// A mapping from SAT integers to the direct representation.
    pub invlitmap: BTreeMap<Lit, BTreeSet<PuzLit>>,
    /// A mapping from each variable to its domain
    pub domainmap: BTreeMap<PuzVar, BTreeSet<i64>>,
    /// List of all literals representing constraints in the problem, and their English-readable name
    pub conset: BTreeMap<Lit, String>,
    /// Inverse of conset
    pub invconset: BTreeMap<String, Lit>,
    /// Lits of all literals in each constraint
    pub varlits_in_con: BTreeMap<Lit, Vec<Lit>>,
    /// List of all literals in a VAR in the direct encoding
    pub varset_lits: BTreeSet<Lit>,
    /// Lits of all literals in a VAR in the direct encoding, representing a variable becoming unassigned
    pub varset_lits_neg: BTreeSet<Lit>,
    /// List of all literals which turn on CON
    pub conset_lits: BTreeSet<Lit>,
    /// List of all literals in an AUX
    pub auxset_lits: BTreeSet<Lit>,

    /// A mapping from variables in the order representation to their corresponding SAT integers.
    /// These are generally not useful, but are sometimes used when scanning
    /// the entire problem
    pub order_encoding_map: BTreeMap<PuzVar, HashSet<Lit>>,
    /// A mapping from lits to the order representation they represent.
    /// These are generally not useful, but are sometimes used when scanning
    /// the entire problem
    pub inv_order_encoding_map: BTreeMap<Lit, PuzVar>,
    /// List of all literals in tbe order encoding of a variable
    /// These are generally not useful, but are sometimes used when scanning
    /// the SAT instance
    pub order_encoding_all_lits: BTreeSet<Lit>,

    /// Whenever a lit 'x' is proved, then `reveal_map`(x) should also be
    /// added to the known lits.
    pub reveal_map: BTreeMap<Lit, Lit>,
}

fn safe_insert<K: Ord, V>(dict: &mut BTreeMap<K, V>, key: K, value: V) -> anyhow::Result<()> {
    if dict.insert(key, value).is_some() {
        bail!("Internal Error: Repeated Key")
    }
    Ok(())
}

impl PuzzleParse {
    #[must_use]
    pub fn new_from_eprime(
        vars: BTreeSet<String>,
        auxvars: BTreeSet<String>,
        cons: BTreeMap<String, String>,
        reveal: BTreeMap<String, String>,
        params: BTreeMap<String, serde_json::value::Value>,
        kind: Option<String>,
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
            },
            satinstance: SatInstance::new(),
            cnf: None,
            litmap: BTreeMap::new(),
            invlitmap: BTreeMap::new(),
            domainmap: BTreeMap::new(),
            order_encoding_map: BTreeMap::new(),
            inv_order_encoding_map: BTreeMap::new(),
            order_encoding_all_lits: BTreeSet::new(),
            conset: BTreeMap::new(),
            invconset: BTreeMap::new(),
            varlits_in_con: BTreeMap::new(),
            varset_lits: BTreeSet::new(),
            varset_lits_neg: BTreeSet::new(),
            conset_lits: BTreeSet::new(),
            auxset_lits: BTreeSet::new(),
            reveal_map: BTreeMap::new(),
        }
    }

    fn finalise(&mut self) -> anyhow::Result<()> {
        {
            let mut newlitmap = BTreeMap::new();
            // Make sure 'litmap' contains both positive and negative version of every problem literal
            for (key, &value) in &self.litmap {
                if let Some(&val) = self.litmap.get(&key.neg()) {
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
            self.litmap.extend(newlitmap);
        }

        // Set up inverse of 'litmap', mapping from integers to PuzLit objects
        for (key, value) in &self.litmap {
            self.invlitmap
                .entry(*value)
                .or_default()
                .insert(key.clone());
        }

        // Get the domain of each variable quickly
        for lit in self.litmap.keys() {
            let var_id = lit.var();
            if lit.sign() {
                self.domainmap.entry(var_id).or_default().insert(lit.val());
            }
        }

        for (puzlit, &lit) in &self.litmap {
            let var = puzlit.var();

            let name = var.name();
            if self.eprime.vars.contains(name) {
                self.varset_lits.insert(lit);
                if !puzlit.sign() {
                    self.varset_lits_neg.insert(lit);
                }
            } else if self.eprime.auxvars.contains(name) {
                self.auxset_lits.insert(lit);
            } else if self.eprime.cons.contains_key(name) {
                // constraints are specially dealt with above
            } else if self.eprime.reveal_values.contains(name) {
                // reveal_values are dealt with below,
                // as we need all of 'varset' to be complete first
                // So here we just allow the name
            } else {
                bail!("Cannot identify {:?}", puzlit);
            }
        }

        for (puzlit, &lit) in &self.litmap {
            let var = puzlit.var();
            let name = var.name();
            if self.eprime.reveal.contains_key(name) && puzlit.sign() {
                let mut index = puzlit.varval().var().indices().clone();
                index.push(puzlit.varval().val);

                let target_name = self.eprime.reveal.get(name).unwrap();

                let target_puzvar = PuzVar::new(target_name, index);
                let target_varvalpair = VarValPair::new(&target_puzvar, 1);
                let target_puzlit = PuzLit::new_eq(target_varvalpair);

                if let Some(&target_lit) = self.litmap.get(&target_puzlit) {
                    safe_insert(&mut self.reveal_map, lit, target_lit)
                        .context("Some variable used in two 'REVEAL'")?;
                } else {
                    info!("Can't find {target_puzlit} from {puzlit}");
                }
            }
        }

        let mut usedconstraintnames: HashSet<String> = HashSet::new();

        let fvc = FindVarConnections::new(&self.satinstance, &self.all_var_related_lits());

        // Tidy up and check constraints
        for (varid, vals) in &self.domainmap {
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

                // Check is we have used this name before
                if usedconstraintnames.contains(&constraintname) {
                    bail!(format!("CON name {:?} used twice", constraintname))
                }
                usedconstraintnames.insert(constraintname.clone());

                // TODO: Skip constraints which are already parsed,
                // or trivial (parse.py 270 -- 291)

                let puzlit = PuzLit::new_eq(VarValPair::new(varid, 1));
                let lit = *self.litmap.get(&puzlit).unwrap();
                safe_insert(&mut self.conset, lit, constraintname.clone())?;
                safe_insert(&mut self.invconset, constraintname.clone(), lit)?;
                self.conset_lits.insert(lit);
                safe_insert(&mut self.varlits_in_con, lit, fvc.get_connections(lit))?;
                info!(
                    "MAP {} {:?}",
                    &constraintname,
                    self.varlits_in_con.get(&lit).unwrap()
                );
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn lit_is_con(&self, lit: &Lit) -> bool {
        self.conset_lits.contains(lit)
    }

    #[must_use]
    pub fn lit_to_con(&self, lit: &Lit) -> &String {
        assert!(self.lit_is_con(lit));
        self.conset.get(lit).unwrap()
    }

    #[must_use]
    pub fn lit_is_var(&self, lit: &Lit) -> bool {
        self.varset_lits.contains(lit)
    }

    #[must_use]
    pub fn lit_to_vars(&self, lit: &Lit) -> &BTreeSet<PuzLit> {
        self.invlitmap.get(lit).expect("IE: Bad lit")
    }

    // All lits included in both the direct and ordered encoding
    // of VARs
    #[must_use]
    pub fn all_var_related_lits(&self) -> HashSet<Lit> {
        let ordered_var: BTreeSet<Lit> = self
            .order_encoding_map
            .iter()
            .filter(|(k, _)| self.eprime.vars.contains(k.name()))
            .flat_map(|(_, v)| v)
            .copied()
            .collect();

        self.varset_lits.union(&ordered_var).copied().collect()
    }

    // All VarValPairs included in VARs
    #[must_use]
    pub fn all_var_varvals(&self) -> BTreeSet<VarValPair> {
        self.varset_lits
            .iter()
            .flat_map(|x| self.lit_to_vars(x))
            .map(super::PuzLit::varval)
            .collect()
    }

    /// Given a collection of Lits representing both direct and ordered
    /// representations, collect them into a collection of `VarValPair`s
    #[must_use]
    pub fn direct_or_ordered_lit_to_varvalpair(&self, lit: &Lit) -> BTreeSet<VarValPair> {
        let direct_lits = self.invlitmap.get(lit).cloned().unwrap_or_default();

        let order_lits = if let Some(var) = self.inv_order_encoding_map.get(lit) {
            self.domainmap
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
    pub fn constraints(&self) -> BTreeSet<String> {
        self.invconset.keys().cloned().collect()
    }

    #[must_use]
    pub fn constraint_scope(&self, con: &String) -> BTreeSet<VarValPair> {
        let lit = self.invconset.get(con).expect("IE: Bad constraint name");

        let lits = self.varlits_in_con.get(lit).expect("IE: Bad constraint");
        let puzlits = lits
            .iter()
            .flat_map(|l| self.direct_or_ordered_lit_to_varvalpair(l))
            .collect_vec();

        BTreeSet::from_iter(puzlits)
    }

    pub fn filter_out_constraint(&mut self, con: &str) {
        assert!(self.eprime.cons.contains_key(con));
        let mut new_conset_lits = BTreeSet::new();
        for l in self.conset_lits.iter() {
            let puzvars = self.invlitmap.get(l).unwrap();
            if !puzvars.iter().all(|p| p.var().name() == con) {
                new_conset_lits.insert(*l);
            }
        }
        println!(
            "Removing {}: {} -> {}",
            con,
            self.conset_lits.len(),
            new_conset_lits.len()
        );
        self.conset_lits = new_conset_lits;
    }
}

fn parse_eprime(in_path: &PathBuf, eprimeparam: &PathBuf) -> anyhow::Result<PuzzleParse> {
    info!(target: "parser", "reading DIMACS {:?}", in_path);

    let mut vars: BTreeSet<String> = BTreeSet::new();
    let mut puzzle: BTreeSet<String> = BTreeSet::new();

    let mut auxvars: BTreeSet<String> = BTreeSet::new();

    let mut cons: BTreeMap<String, String> = BTreeMap::new();

    let mut factvars: BTreeMap<String, String> = BTreeMap::new();

    let mut kind: Option<String> = None;

    let conmatch = Regex::new(r#"\$#CON (.*) "(.*)""#).unwrap();

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
                    .unwrap_or_else(|| panic!("Broken line: {line}"));

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

    info!(target: "parser", "Names parsed from ESSENCE': vars: {:?} auxvars: {:?} cons {:?}", vars, auxvars, cons);

    // Read parameters in as a JSON object
    let params = read_essence_param(eprimeparam)?;

    Ok(PuzzleParse::new_from_eprime(
        vars, auxvars, cons, factvars, params, kind,
    ))
}

fn read_dimacs(in_path: &PathBuf, dimacs: &mut PuzzleParse) -> anyhow::Result<()> {
    let dvarmatch = Regex::new(r"c Var '(.*)' direct represents '(.*)' with '(.*)'").unwrap();
    let ovarmatch = Regex::new(r"c Var '(.*)' order represents '(.*)' with '(.*)'").unwrap();

    let file = File::open(in_path)?;
    let reader = io::BufReader::new(file);

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
                    let satlit = Lit::from_ipasir(i32::try_from(litval)?)?;
                    let varid =
                        crate::problem::util::parsing::parse_savile_row_name(dimacs, &match_[1])?;

                    if let Some(varid) = varid {
                        let puzlit = PuzLit::new_eq(VarValPair::new(
                            &varid,
                            match_[2].parse::<i64>().unwrap(),
                        ));
                        safe_insert(&mut dimacs.litmap, puzlit, satlit)?;
                    }
                }
            } else {
                let match_ = omatch.unwrap();
                let litval = match_[3].parse::<i64>().unwrap();
                info!(target: "parser", "matches: {:?}", match_);
                if !match_[1].starts_with("aux") && litval != 9_223_372_036_854_775_807 {
                    let satlit = Lit::from_ipasir(i32::try_from(litval)?)?;
                    let varid =
                        crate::problem::util::parsing::parse_savile_row_name(dimacs, &match_[1])?;

                    if let Some(varid) = varid {
                        // Not currently using exact literal
                        // let puzlit = PuzLit::new_eq_val(&varid, match_[2].parse::<i64>().unwrap());
                        dimacs
                            .order_encoding_map
                            .entry(varid.clone())
                            .or_default()
                            .insert(satlit);
                        dimacs
                            .order_encoding_map
                            .entry(varid.clone())
                            .or_default()
                            .insert(-satlit);
                        dimacs.order_encoding_all_lits.insert(satlit);
                        dimacs.order_encoding_all_lits.insert(-satlit);
                        if let Some(val) = dimacs.inv_order_encoding_map.get(&satlit) {
                            if *val != varid {
                                bail!("{} used for two variables: {} {}", satlit, val, varid);
                            }
                        }
                        safe_insert(&mut dimacs.inv_order_encoding_map, satlit, varid.clone())?;
                        safe_insert(&mut dimacs.inv_order_encoding_map, -satlit, varid.clone())?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn parse_essence(eprimein: &PathBuf, eprimeparamin: &PathBuf) -> anyhow::Result<PuzzleParse> {
    //let mut litmap = BTreeMap::new();
    //let mut varlist = Vec::new();

    let tdir = TempDir::new().unwrap();

    let eprime = tdir.path().join(eprimein.file_name().unwrap());
    let eprimeparam = tdir.path().join(eprimeparamin.file_name().unwrap());

    fs::copy(eprimein, &eprime)?;
    fs::copy(eprimeparamin, &eprimeparam)?;

    info!("Parsing Essence in TempDir: {tdir:?}");

    let finaleprime: PathBuf;
    let finaleprimeparam: PathBuf;

    // If input is essence, translate to essence' for savilerow
    if eprime.ends_with(".essence") {
        info!(target: "parser", "Running {:?} {:?} through conjure", eprime, eprimeparam);
        let output = Command::new("conjure")
            .arg("solve")
            .arg("-o")
            .arg(tdir.path().to_str().unwrap())
            .arg(eprime)
            .arg(eprimeparam)
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

    let makedimacs = Command::new("savilerow")
        .arg("-in-eprime")
        .arg(&finaleprime)
        .arg("-in-param")
        .arg(&finaleprimeparam)
        .arg("-sat-output-mapping")
        .arg("-sat")
        .arg("-sat-family")
        .arg("lingeling")
        .arg("-S0")
        .arg("-O0")
        .arg("-reduce-domains")
        .arg("-aggregate")
        .output()
        .expect("Failed to execute command");

    if !makedimacs.status.success() {
        bail!(
            "savilerow failed\n{}\n{}",
            String::from_utf8_lossy(&makedimacs.stdout),
            String::from_utf8_lossy(&makedimacs.stderr)
        );
    }

    let in_eprime_path = PathBuf::from(&finaleprime);

    // Need to put '.dimacs' on the end in this slightly horrible way.
    let in_dimacs_path = PathBuf::from(finaleprimeparam.to_str().unwrap().to_owned() + ".dimacs");

    let mut eprimeparse = parse_eprime(&in_eprime_path, &finaleprimeparam)?;

    eprimeparse.satinstance =
        instances::SatInstance::<BasicVarManager>::from_dimacs_path(&in_dimacs_path)
            .context("reading dimacs")?;

    eprimeparse.cnf = Some(Arc::new(eprimeparse.satinstance.clone().into_cnf().0));

    read_dimacs(&in_dimacs_path, &mut eprimeparse).context("reading variable info from dimacs")?;

    eprimeparse.finalise().context("finalisation of parsing")?;

    forget(tdir);

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
    info!(target: "parser", "Pretty printing {:?} as {}", file, format);
    let output = Command::new("conjure")
        .arg("pretty")
        .arg("--output-format")
        .arg(format)
        .arg(file)
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

        assert_eq!(puz.conset_lits.len() - filter1_puz.conset_lits.len(), 6);
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

        // These next two may become '3' at some point, when we do better
        // at rejecting useless constraints
        assert_eq!(puz.conset.len(), 4);
        assert_eq!(puz.conset_lits.len(), 4);
        assert_eq!(puz.varset_lits.len(), 4 * 4 * 2); // 4 variables, 4 domain values, 2 pos+neg lits
        assert_eq!(puz.auxset_lits.len(), 0);
        let cons = puz.constraints();

        assert!(puz.conset_lits.iter().all(|l| puz.lit_is_con(l)));
        assert!(puz.varset_lits.iter().all(|l| !puz.lit_is_con(l)));
        assert!(puz.conset_lits.iter().all(|l| !puz.lit_is_var(l)));
        assert!(puz.varset_lits.iter().all(|l| puz.lit_is_var(l)));

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

        let cons = puz.constraints();

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

        let cons = puz.constraints();

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
}
