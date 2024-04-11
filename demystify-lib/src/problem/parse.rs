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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use std::fs;
use std::io::prelude::*;
use std::io::BufReader;

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tempfile::TempDir;
use tracing::{debug, info};

use std::fs::File;
use std::io;

use crate::problem::util::parse_constraint_name;
use crate::problem::{PuzLit, PuzVar};

use super::util::FindVarConnections;

#[derive(Debug)]
pub struct EPrimeAnnotations {
    /// The set of variables in the Essence' file.
    pub vars: BTreeSet<String>,
    /// The set of auxiliary variables in the Essence'file.
    pub auxvars: BTreeSet<String>,
    /// The constraints in the Essence' file, represented as a mapping from constraint name to constraint expression.
    pub cons: BTreeMap<String, String>,
    /// The parameters read from the param file
    params: BTreeMap<String, serde_json::value::Value>,
    /// The kind of puzzle
    pub kind: Option<String>,
}

impl EPrimeAnnotations {
    pub fn has_param(&self, s: &str) -> bool {
        self.params.contains_key(s)
    }

    pub fn param_bool(&self, s: &str) -> anyhow::Result<bool> {
        serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {}", s))?
                .clone(),
        )
        .context(format!("Param {} is not bool", s))
    }

    pub fn param_i64(&self, s: &str) -> anyhow::Result<i64> {
        serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {}", s))?
                .clone(),
        )
        .context(format!("Param {} is not int", s))
    }

    pub fn param_vec_i64(&self, s: &str) -> anyhow::Result<Vec<i64>> {
        // Conjure produces arrays as maps, so we need to fix up
        let map: HashMap<i64, i64> = serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {}", s))?
                .clone(),
        )
        .context(format!("Param {} is not an array of ints", s))?;

        let mut ret: Vec<i64> = vec![0; map.len()];

        for i in 0..map.len() {
            ret[i] = *map
                .get(&((i + 1) as i64))
                .context(format!("Malformed param? {}", s))?;
        }

        Ok(ret)
    }

    pub fn param_vec_vec_i64(&self, s: &str) -> anyhow::Result<Vec<Vec<i64>>> {
        // Conjure produces arrays as maps, so we need to fix up
        let map: HashMap<i64, HashMap<i64, i64>> = serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {}", s))?
                .clone(),
        )
        .context(format!("Param {} is not a 2d array of ints", s))?;

        let mut ret: Vec<Vec<i64>> = vec![vec![]; map.len()];

        for i in 0..map.len() {
            let row = map
                .get(&((i + 1) as i64))
                .context(format!("Malformed param? {}", s))?;
            let mut rowvec: Vec<i64> = vec![0; row.len()];
            for j in 0..row.len() {
                rowvec[j] = *row
                    .get(&((j + 1) as i64))
                    .context(format!("Malformed param? {}", s))?;
            }

            ret[i] = rowvec;
        }

        Ok(ret)
    }

    pub fn param_vec_vec_option_i64(&self, s: &str) -> anyhow::Result<Vec<Vec<Option<i64>>>> {
        // Conjure produces arrays as maps, so we need to fix up
        let map: HashMap<i64, HashMap<i64, Option<i64>>> = serde_json::from_value(
            self.params
                .get(s)
                .context(format!("Missing param: {}", s))?
                .clone(),
        )
        .context(format!("Param {} is not a 2d array of ints and nulls", s))?;

        let mut ret: Vec<Vec<Option<i64>>> = vec![vec![]; map.len()];

        for i in 0..map.len() {
            let row = map
                .get(&((i + 1) as i64))
                .context(format!("Malformed param? {}", s))?;
            let mut rowvec: Vec<Option<i64>> = vec![None; row.len()];
            for j in 0..row.len() {
                rowvec[j] = *row
                    .get(&((j + 1) as i64))
                    .context(format!("Malformed param? {}", s))?;
            }

            ret[i] = rowvec;
        }

        Ok(ret)
    }
}

/// Represents the result of parsing a DIMACS file.

#[derive(Debug)]
pub struct PuzzleParse {
    /// The annotations from the Essence' file
    pub eprime: EPrimeAnnotations,
    /// The SAT instance parsed from the DIMACS file.
    pub satinstance: SatInstance,
    // A Copy of the CNF of the SAT instance (as we frequently need this)
    pub cnf: Option<Arc<Cnf>>,
    /// A mapping from literals in the direct representation to their corresponding SAT integer.
    pub litmap: HashMap<PuzLit, Lit>,
    /// A mapping from SAT integers to the direct representation.
    pub invlitmap: HashMap<Lit, BTreeSet<PuzLit>>,
    /// A mapping from each variable to its domain
    pub domainmap: HashMap<PuzVar, BTreeSet<i64>>,
    /// List of all constraints in the problem, and their English-readable name
    pub conset: BTreeMap<Lit, String>,
    /// Lits of all literals in each constraint
    pub varlits_in_con: BTreeMap<Lit, Vec<Lit>>,
    /// List of all literals in a VAR
    pub varset_lits: BTreeSet<Lit>,
    /// List of all literals which turn on CON
    pub conset_lits: BTreeSet<Lit>,
    /// List of all literals in an AUX
    pub auxset_lits: BTreeSet<Lit>,

    /// A mapping from variables in the order representation to their corresponding SAT integers.
    /// These are generally not useful, but are sometimes used when scanning
    /// the entire problem
    pub ordervarmap: HashMap<PuzVar, HashSet<Lit>>,
    /// A mapping from lits to the order representation they represent.
    /// These are generally not useful, but are sometimes used when scanning
    /// the entire problem
    pub invordervarmap: HashMap<Lit, PuzVar>,
    /// List of all literals in tbe order encoding of a VAR
    /// These are generally not useful, but are sometimes used when scanning
    /// the entire problem
    pub varset_order_lits: BTreeSet<Lit>,
}

impl PuzzleParse {
    #[must_use]
    pub fn new_from_eprime(
        vars: BTreeSet<String>,
        auxvars: BTreeSet<String>,
        cons: BTreeMap<String, String>,
        params: BTreeMap<String, serde_json::value::Value>,
        kind: Option<String>,
    ) -> PuzzleParse {
        PuzzleParse {
            eprime: EPrimeAnnotations {
                vars,
                auxvars,
                cons,
                params,
                kind,
            },
            satinstance: SatInstance::new(),
            cnf: None,
            litmap: HashMap::new(),
            invlitmap: HashMap::new(),
            domainmap: HashMap::new(),
            ordervarmap: HashMap::new(),
            invordervarmap: HashMap::new(),
            varset_order_lits: BTreeSet::new(),
            conset: BTreeMap::new(),
            varlits_in_con: BTreeMap::new(),
            varset_lits: BTreeSet::new(),
            conset_lits: BTreeSet::new(),
            auxset_lits: BTreeSet::new(),
        }
    }

    fn finalise(&mut self) -> anyhow::Result<()> {
        {
            let mut newlitmap = HashMap::new();
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
                    newlitmap.insert(key.neg(), -value);
                }
            }
            self.litmap.extend(newlitmap.drain());
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

        let mut usedconstraintnames: HashSet<String> = HashSet::new();

        // Gather all lits, for use gathering connections between constraints and variables
        let all_lits = self
            .varset_lits
            .union(&self.varset_order_lits)
            .copied()
            .collect();

        let fvc = FindVarConnections::new(&self.satinstance, &all_lits);

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

                let constraintname =
                    parse_constraint_name(template_string, &self.eprime.params, &varid.indices)?;

                // Check is we have used this name before
                if usedconstraintnames.contains(&constraintname) {
                    bail!(format!("CON name {:?} used twice", constraintname))
                }
                usedconstraintnames.insert(constraintname.clone());

                // TODO: Skip constraints which are already parsed,
                // or trivial (parse.py 270 -- 291)

                let puzlit = PuzLit::new_eq_val(varid, 1);
                let lit = *self.litmap.get(&puzlit).unwrap();
                self.conset.insert(lit, constraintname);
                self.conset_lits.insert(lit);
                self.varlits_in_con.insert(lit, fvc.get_connections(lit));

                // TODO: Find the literals in every constraint
            }
        }

        for (puzlit, &lit) in &self.litmap {
            let var = puzlit.var();
            let name = var.name();
            if self.eprime.vars.contains(name) {
                self.varset_lits.insert(lit);
            } else if self.eprime.auxvars.contains(name) {
                self.auxset_lits.insert(lit);
            } else if self.eprime.cons.contains_key(name) {
                // constraints are specially dealt with above
            } else {
                bail!("Cannot indentify {:?}", puzlit);
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
        self.invlitmap.get(lit).unwrap()
    }

    /// Given a collection of Lits representing both direct and ordered
    /// representations, collect them into a collection of `PuzLits`
    #[must_use]
    pub fn collect_puzlits_both_direct_and_ordered(&self, lits: Vec<Lit>) -> Vec<PuzLit> {
        let mut collected: BTreeSet<PuzLit> = BTreeSet::new();

        for l in lits {
            if let Some(found_lits) = self.invlitmap.get(&l) {
                for f in found_lits {
                    if f.sign() {
                        collected.insert(f.clone());
                    } else {
                        collected.insert(f.neg());
                    }
                }
            }
            if let Some(found_var) = self.invordervarmap.get(&l) {
                for &val in self.domainmap.get(found_var).unwrap() {
                    collected.insert(PuzLit::new_eq_val(found_var, val));
                }
            }
        }

        collected.into_iter().collect_vec()
    }
}

fn parse_eprime(in_path: &PathBuf, eprimeparam: &PathBuf) -> anyhow::Result<PuzzleParse> {
    info!(target: "parser", "reading DIMACS {:?}", in_path);

    let mut vars: BTreeSet<String> = BTreeSet::new();
    let mut auxvars: BTreeSet<String> = BTreeSet::new();

    let mut cons: BTreeMap<String, String> = BTreeMap::new();

    let mut kind: Option<String> = None;

    let conmatch = Regex::new(r#"\$#CON (.*) \"(.*)\""#).unwrap();

    let file = File::open(in_path)?;
    let reader = io::BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        if line.contains("$#") {
            debug!(target: "parser", "line {:?}", line);
            let parts: Vec<&str> = line.split_whitespace().collect();

            if line.starts_with("$#VAR") {
                let v = parts[1].to_string();
                info!(target: "parser", "Found VAR: '{}'", v);

                if vars.contains(&v) || auxvars.contains(&v) {
                    bail!(format!("variable {} defined twice", v));
                }
                vars.insert(v);
            } else if line.starts_with("$#CON") {
                info!(target: "parser", "{}", line);
                let captures = conmatch.captures(&line).unwrap();

                let con_name = captures.get(1).unwrap().as_str().to_string();
                let con_value = captures.get(2).unwrap().as_str().to_string();

                info!(target: "parser", "Found CON: '{}' '{}'", con_name, con_value);

                if cons.contains_key(&con_name) {
                    bail!(format!("{} defined twice", con_name));
                }
                cons.insert(con_name, con_value);
            } else if line.starts_with("$#AUX") {
                let v = parts[1].to_string();
                info!(target: "parser", "Found Aux VAR: '{}'", v);

                if vars.contains(&v) || auxvars.contains(&v) {
                    bail!(format!("{} defined twice", v));
                }
                auxvars.insert(v);
            } else if line.starts_with("$#KIND") {
                let v = parts[1].to_string();
                if kind.is_some() {
                    bail!("Cannot have two 'KIND' statements");
                }
                kind = Some(v)
            }
        }
    }

    info!(target: "parser", "Names parsed from ESSENCE': vars: {:?} auxvars: {:?} cons {:?}", vars, auxvars, cons);

    // Read parameters in as a JSON object
    let params = read_essence_param(eprimeparam)?;

    Ok(PuzzleParse::new_from_eprime(
        vars, auxvars, cons, params, kind,
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
                    let varid = crate::problem::util::parse_savile_row_name(dimacs, &match_[1])?;

                    if let Some(varid) = varid {
                        let puzlit = PuzLit::new_eq_val(&varid, match_[2].parse::<i64>().unwrap());
                        dimacs.litmap.insert(puzlit, satlit);
                    }
                }
            } else {
                let match_ = omatch.unwrap();
                let litval = match_[3].parse::<i64>().unwrap();
                info!(target: "parser", "matches: {:?}", match_);
                if !match_[1].starts_with("aux") && litval != 9_223_372_036_854_775_807 {
                    let satlit = Lit::from_ipasir(i32::try_from(litval)?)?;
                    let varid = crate::problem::util::parse_savile_row_name(dimacs, &match_[1])?;

                    if let Some(varid) = varid {
                        // Not currently using exact literal
                        // let puzlit = PuzLit::new_eq_val(&varid, match_[2].parse::<i64>().unwrap());
                        dimacs
                            .ordervarmap
                            .entry(varid.clone())
                            .or_default()
                            .insert(satlit);
                        dimacs
                            .ordervarmap
                            .entry(varid.clone())
                            .or_default()
                            .insert(-satlit);
                        dimacs.varset_order_lits.insert(satlit);
                        dimacs.varset_order_lits.insert(-satlit);
                        if let Some(val) = dimacs.invordervarmap.get(&satlit) {
                            if *val != varid {
                                bail!("{} used for two variables: {} {}", satlit, val, varid);
                            }
                        }
                        dimacs.invordervarmap.insert(satlit, varid.clone());
                        dimacs.invordervarmap.insert(-satlit, varid.clone());
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn parse_essence(eprime: &PathBuf, eprimeparam: &PathBuf) -> anyhow::Result<PuzzleParse> {
    //let mut litmap = HashMap::new();
    //let mut varlist = Vec::new();

    let tdir = TempDir::new().unwrap();

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
        instances::SatInstance::<BasicVarManager>::from_dimacs_path(&in_dimacs_path)?;

    eprimeparse.cnf = Some(Arc::new(eprimeparse.satinstance.clone().as_cnf().0));

    read_dimacs(&in_dimacs_path, &mut eprimeparse)?;

    eprimeparse.finalise()?;
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
        info!(target: "parser", "Reading params {:?} as conjure param", eprimeparam);
        let output = Command::new("conjure")
            .arg("pretty")
            .arg("--output-format")
            .arg("json")
            .arg(eprimeparam)
            .output()
            .expect("Failed to execute command");

        if !output.status.success() {
            bail!(format!(
                "Conjure pretty-printing of params failed\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        serde_json::from_slice(&output.stdout).context("Failed to parse JSON produced by conjure")
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;

    #[test]
    fn test_parse_essence_binairo() {
        let eprime_path = "./tst/binairo.eprime";
        let eprimeparam_path = "./tst/binairo-1.param";

        let puz =
            crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);

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
    }
}
