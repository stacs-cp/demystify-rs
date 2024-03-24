use anyhow::bail;
use regex::Regex;
use rustsat::instances::{self, BasicVarManager, SatInstance};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::fs;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;
use tracing::info;

use std::fs::File;
use std::io;

struct DimacsParse {
    vars: BTreeSet<String>,
    auxvars: BTreeSet<String>,
    cons: BTreeMap<String, String>,
    satinstance: SatInstance,
}

fn parse_dimacs(in_path: &PathBuf) -> anyhow::Result<DimacsParse> {
    info!(target: "parser", "reading DIMACS {:?}", in_path);

    let mut vars: BTreeSet<String> = BTreeSet::new();
    let mut auxvars: BTreeSet<String> = BTreeSet::new();

    let mut cons: BTreeMap<String, String> = BTreeMap::new();

    let mut satinstance = instances::SatInstance::<BasicVarManager>::from_dimacs_path(in_path)?;

    let conmatch = Regex::new(r#"\$\#CON (.*) \"(.*)\""#).unwrap();

    let file = File::open(in_path)?;
    let reader = io::BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        if line.contains("$#") {
            let parts: Vec<&str> = line.trim().split_whitespace().collect();

            if line.starts_with("$#VAR") {
                let v = parts[1].to_string();
                println!("Found VAR: '{}'", v);

                if vars.contains(&v) || auxvars.contains(&v) {
                    bail!(format!("variable {} defined twice", v));
                }
                vars.insert(v);
            } else if line.starts_with("$#CON") {
                println!("{}", line);
                let captures = conmatch.captures(&line).unwrap();

                let con_name = captures.get(1).unwrap().as_str().to_string();
                let con_value = captures.get(2).unwrap().as_str().to_string();

                println!("Found CON: '{}' '{}'", con_name, con_value);

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

    Ok(DimacsParse {
        vars,
        auxvars,
        cons,
        satinstance,
    })
}

fn read_savilerow_annotations(in_path: &PathBuf, dimacs: &DimacsParse) -> anyhow::Result<()> {
    let dvarmatch = Regex::new(r"c Var '(.*)' direct represents '(.*)' with '(.*)'").unwrap();
    let ovarmatch = Regex::new(r"c Var '(.*)' order represents '(.*)' with '(.*)'").unwrap();

    let file = File::open(in_path)?;
    let reader = io::BufReader::new(file);

    let mut varmap = HashMap::new();
    let mut ordervarmap = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        if line.starts_with("c Var") {
            let dmatch = dvarmatch.captures(&line);
            let omatch = ovarmatch.captures(&line);
            if !(dmatch.is_some() || omatch.is_some()) {
                bail!("Failed to parse '{:?}'", line);
            }

            let (fillmap, match_) = if let Some(dmatch) = dmatch {
                (&mut varmap, dmatch)
            } else {
                (&mut ordervarmap, omatch.unwrap())
            };

            if !match_[1].starts_with("aux") {
                let var = crate::problem::util::parse_savile_row_name(
                    &dimacs.vars,
                    &dimacs.auxvars,
                    &match_[1], // TODO: dimacs.vars should include constraints
                )?;

                if let Some((var0, var1)) = var {
                    fillmap
                        .entry(var0)
                        .or_insert_with(HashMap::new)
                        .entry(var1)
                        .or_insert_with(HashMap::new)
                        .insert(
                            match_[2].parse::<i32>().unwrap(),
                            match_[3].parse::<i32>().unwrap(),
                        );
                }
            }
        }
    }

    println!("{:?}", varmap);

    Ok(())
}

pub fn parse_essence(eprime: &str, eprimeparam: &str) -> anyhow::Result<()> {
    //let mut varmap = HashMap::new();
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

    let finaleprime: String;
    let finaleprimeparam: String;

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

        finaleprime = format!("{}/model000001.eprime", tdir.path().to_str().unwrap());
        finaleprimeparam = fs::read_dir(tdir.path())
            .unwrap()
            .filter_map(Result::ok)
            .find(|d| d.path().extension().and_then(|s| s.to_str()) == Some("param"))
            .map(|d| d.path().to_str().unwrap().to_string())
            .unwrap_or_else(|| String::from(""));
    } else {
        finaleprime = eprime.to_string();
        finaleprimeparam = eprimeparam.to_string();
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

    let in_path = PathBuf::from(finaleprimeparam + ".dimacs");

    let dimacs = parse_dimacs(&in_path);

    Ok(())
}
