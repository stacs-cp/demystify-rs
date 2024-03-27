use anyhow::bail;
use regex::Regex;
use rustsat::instances::{self, BasicVarManager, SatInstance};
use rustsat::types::Lit;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use std::fs;
use std::io::prelude::*;
use std::io::BufReader;

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;
use tracing::{debug, info};

use std::fs::File;
use std::io;

use crate::problem::{PuzLit, PuzVar};

#[derive(Debug)]
pub struct EPrimeAnnotations {
    /// The set of variables in the Essence' file.
    pub vars: BTreeSet<String>,
    /// The set of auxiliary variables in the Essence'file.
    pub auxvars: BTreeSet<String>,
    /// The constraints in the Essence' file, represented as a mapping from constraint name to constraint expression.
    pub cons: BTreeMap<String, String>,
}
/// Represents the result of parsing a DIMACS file.

#[derive(Debug)]
pub struct PuzzleParse {
    /// The annotations from the Essence' file
    pub eprime: EPrimeAnnotations,
    /// The SAT instance parsed from the DIMACS file.
    pub satinstance: SatInstance,
    /// A mapping from literals in the direct representation to their corresponding SAT integer.
    pub litmap: HashMap<PuzLit, Lit>,
    /// A mapping from SAT integers to the direct representation.
    pub invlitmap: HashMap<Lit, BTreeSet<PuzLit>>,
    /// A mapping from each variable to its domain
    pub domainmap: HashMap<PuzVar, BTreeSet<i64>>,
    /// A mapping from literals in the order representation to their corresponding SAT integer.
    pub ordervarmap: HashMap<PuzLit, Lit>,
    /// List of all constraints in the problem, and their English-readable name
    pub conset: BTreeMap<Lit, String>,
    /// List of all literals in a VAR
    pub varset_lits: BTreeSet<Lit>,
    /// List of all literals which turn on CON
    pub conset_lits: BTreeSet<Lit>,
    /// List of all literals in an AUX
    pub auxset_lits: BTreeSet<Lit>,
}

impl PuzzleParse {
    pub fn new_from_eprime(
        vars: BTreeSet<String>,
        auxvars: BTreeSet<String>,
        cons: BTreeMap<String, String>,
    ) -> PuzzleParse {
        PuzzleParse {
            eprime: EPrimeAnnotations {
                vars,
                auxvars,
                cons,
            },
            satinstance: SatInstance::new(),
            litmap: HashMap::new(),
            invlitmap: HashMap::new(),
            domainmap: HashMap::new(),
            ordervarmap: HashMap::new(),
            conset: BTreeMap::new(),
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

                // TODO: Complete template
                let constraintname = format!("{:?} in {:?}", template_string, varid);

                // Check is we have used this name before
                if usedconstraintnames.contains(&constraintname) {
                    bail!(format!("CON name {:?} used twice", constraintname))
                }
                usedconstraintnames.insert(constraintname.clone());

                // TODO: Skip constraints which are already parsed,
                // or trivial (parse.py 270 -- 291)

                let puzlit = PuzLit::new_eq_val(varid, 1);
                let lit = *self.litmap.get(&puzlit).unwrap();
                self.conset.insert(lit.clone(), constraintname);
                self.conset_lits.insert(lit);

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

    pub fn lit_is_con(&self, lit: &Lit) -> bool {
        self.conset_lits.contains(lit)
    }

    pub fn lit_to_con(&self, lit: &Lit) -> &String {
        assert!(self.lit_is_con(lit));
        self.conset.get(lit).unwrap()
    }

    pub fn lit_is_var(&self, lit: &Lit) -> bool {
        self.varset_lits.contains(lit)
    }

    pub fn lit_to_vars(&self, lit: &Lit) -> &BTreeSet<PuzLit> {
        self.invlitmap.get(lit).unwrap()
    }
}

fn parse_eprime(in_path: &PathBuf) -> anyhow::Result<PuzzleParse> {
    info!(target: "parser", "reading DIMACS {:?}", in_path);

    let mut vars: BTreeSet<String> = BTreeSet::new();
    let mut auxvars: BTreeSet<String> = BTreeSet::new();

    let mut cons: BTreeMap<String, String> = BTreeMap::new();

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
                println!("Found Aux VAR: '{}'", v);

                if vars.contains(&v) || auxvars.contains(&v) {
                    bail!(format!("{} defined twice", v));
                }
                auxvars.insert(v);
            }
        }
    }

    info!(target: "parser", "Names parsed from ESSENCE': vars: {:?} auxvars: {:?} cons {:?}", vars, auxvars, cons);
    Ok(PuzzleParse::new_from_eprime(vars, auxvars, cons))
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

                if !match_[1].starts_with("aux") && litval != 9223372036854775807 {
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
                if !match_[1].starts_with("aux") && litval != 9223372036854775807 {
                    let satlit = Lit::from_ipasir(i32::try_from(litval)?)?;
                    let varid = crate::problem::util::parse_savile_row_name(dimacs, &match_[1])?;

                    if let Some(varid) = varid {
                        let puzlit = PuzLit::new_eq_val(&varid, match_[2].parse::<i64>().unwrap());
                        dimacs.ordervarmap.insert(puzlit, satlit);
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

    // Read parameters in as a JSON object
    let params: serde_json::Value = if eprimeparam.ends_with(".json") {
        info!(target: "parser", "Reading params {:?} as json", eprimeparam);
        let file = fs::File::open(eprimeparam).unwrap();
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).unwrap()
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

        serde_json::from_slice(&output.stdout).unwrap()
    };

    info!(target: "parser", "Read params {:?}", params);

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

    let mut eprimeparse = parse_eprime(&in_eprime_path)?;

    eprimeparse.satinstance =
        instances::SatInstance::<BasicVarManager>::from_dimacs_path(&in_dimacs_path)?;

    read_dimacs(&in_dimacs_path, &mut eprimeparse)?;

    eprimeparse.finalise()?;
    Ok(eprimeparse)
}

#[cfg(test)]
mod tests {
    use test_log::test;

    #[test]
    fn test_parse_essence_binairo() {
        let eprime_path = "./tst/binairo.eprime";
        let eprimeparam_path = "./tst/binairo-1.param";

        crate::problem::util::test_utils::build_puzzleparse(eprime_path, eprimeparam_path);
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
        // These next two may become '3' at some point, when we do better
        // at rejecting useless constraints
        assert_eq!(puz.conset.len(), 4);
        assert_eq!(puz.conset_lits.len(), 4);
        assert_eq!(puz.varset_lits.len(), 4 * 4 * 2); // 4 variables, 4 domain values, 2 pos+neg lits
        assert_eq!(puz.auxset_lits.len(), 0);
    }
}
