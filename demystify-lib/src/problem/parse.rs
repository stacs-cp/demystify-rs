use anyhow::bail;
use regex::Regex;
use rustsat::instances::{self, BasicVarManager, SatInstance};

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

use super::ConID;

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
    pub litmap: HashMap<PuzLit, i64>,
    /// A mapping from each variable to its domain
    pub domainmap: HashMap<PuzVar, BTreeSet<i64>>,
    /// A mapping from literals in the order representation to their corresponding SAT integer.
    pub ordervarmap: HashMap<PuzLit, i64>,
    /// A mapping from SAT integers to the direct representation.
    pub invlitmap: HashMap<i64, PuzLit>,
    /// List of all constraints in the problem
    pub conset: BTreeSet<ConID>,
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
            domainmap: HashMap::new(),
            ordervarmap: HashMap::new(),
            invlitmap: HashMap::new(),
            conset: BTreeSet::new(),
        }
    }

    fn finalise(&mut self) -> anyhow::Result<()> {
        // Set up inverse of 'litmap', mapping from integers to PuzLit objects
        for (key, value) in &self.litmap {
            assert!(!self.invlitmap.contains_key(value));
            self.invlitmap.insert(*value, key.clone());
        }

        // Get the domain of each variable quickly
        for lit in self.litmap.keys() {
            let var_id = lit.var();
            assert!(lit.sign());
            self.domainmap.entry(var_id).or_default().insert(lit.val());
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

                self.conset
                    .insert(ConID::new(PuzLit::new_eq_val(varid, 1), constraintname));

                // TODO: Find the literals in every constraint
            }
        }

        Ok(())
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
                if !match_[1].starts_with("aux") {
                    let varid = crate::problem::util::parse_savile_row_name(dimacs, &match_[1])?;

                    if let Some(varid) = varid {
                        let lit = PuzLit::new_eq_val(&varid, match_[2].parse::<i64>().unwrap());
                        dimacs.litmap.insert(lit, match_[3].parse::<i64>().unwrap());
                    }
                }
            } else {
                let match_ = omatch.unwrap();
                info!(target: "parser", "matches: {:?}", match_);
                if !match_[1].starts_with("aux") {
                    let varid = crate::problem::util::parse_savile_row_name(dimacs, &match_[1])?;

                    if let Some(varid) = varid {
                        let lit = PuzLit::new_eq_val(&varid, match_[2].parse::<i64>().unwrap());
                        dimacs
                            .ordervarmap
                            .insert(lit, match_[3].parse::<i64>().unwrap());
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
    use super::*;
    #[test]
    fn test_parse_essence() {
        let eprime_path = "./tst/binairo.eprime";
        let eprimeparam_path = "./tst/binairo-1.param";

        // Create temporary directory for test files
        let temp_dir = tempfile::tempdir().expect("Failed to create temporary directory");

        // Copy eprime file to temporary directory
        let temp_eprime_path = temp_dir.path().join("binairo.eprime");
        fs::copy(eprime_path, &temp_eprime_path).expect("Failed to copy eprime file");

        // Copy eprimeparam file to temporary directory
        let temp_eprimeparam_path = temp_dir.path().join("binairo-1.param");
        fs::copy(eprimeparam_path, &temp_eprimeparam_path)
            .expect("Failed to copy eprimeparam file");

        // Call parse_essence function
        let result = parse_essence(&temp_eprime_path, &temp_eprimeparam_path);

        if result.is_err() {
            panic!("Bad parse: {:?}", result);
        }
        // Assert that the function returns Ok
        assert!(result.is_ok());

        // Clean up temporary directory
        temp_dir
            .close()
            .expect("Failed to clean up temporary directory");
    }
}
