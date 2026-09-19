//! Export normal, three-colour or relaxed-clue Mauve using matched constraint grouping.
//!
//! Usage: mauve_variants normal|three|relaxed LEVEL.json PARSED.json
//! Matches the apps' cell domains and constraint grouping; validates that the
//! supplied reference solution is the unique solution before writing anything.

use anyhow::{Context, Result, bail, ensure};
use demystify::problem::{
    PuzLit, PuzVar, VarValPair,
    parse::PuzzleParse,
    solver::{PuzzleSolver, SolverConfig},
};
use demystify_builder::{PuzzleBuilder, ShowRole};
use serde_json::Value;
use std::{path::Path, sync::Arc};

fn neighbours(i: usize, w: usize, h: usize) -> Vec<usize> {
    let (x, y) = (i % w, i / w);
    let mut out = Vec::new();
    if y > 0 {
        out.push(i - w);
    }
    if x > 0 {
        out.push(i - 1);
    }
    if x + 1 < w {
        out.push(i + 1);
    }
    if y + 1 < h {
        out.push(i + w);
    }
    out
}

fn dimension(d: &Value, key: &str) -> Result<usize> {
    let n = d[key].as_u64().with_context(|| format!("missing {key}"))?;
    ensure!((1..=100).contains(&n), "invalid {key}: {n}");
    Ok(n as usize)
}

fn mauve(d: &Value, w: usize, h: usize, variant: &str) -> Result<PuzzleParse> {
    ensure!(
        d["rules"]["lightsThroughOpposite"] == true,
        "only transparent opposite-colour bulbs are supported"
    );
    let walls: Vec<bool> = serde_json::from_value(d["walls"].clone())?;
    ensure!(walls.len() == w * h, "walls have incorrect length");
    let clues = d["clues"].as_array().context("missing clues")?;
    let colours = if variant == "three" {
        vec![(1, "R", "red"), (2, "B", "blue"), (3, "G", "green")]
    } else {
        vec![(1, "R", "red"), (2, "B", "blue")]
    };
    let values: Vec<i64> = (0..=colours.len() as i64).collect();
    let mut b = PuzzleBuilder::new();
    b.kind("mauve");
    let cells = b.var_int_matrix("cell", &[0..=(w * h) as i64 - 1], &values);
    b.show("cell", ShowRole::Main);
    let has = |i: usize, c: i64| cells.cell(&[i as i64]).eq(c);
    for (i, &wall) in walls.iter().enumerate() {
        if wall {
            b.sum_eq_unguarded(&[has(i, 0)], 1)?;
        }
    }
    let mut runs = Vec::new();
    for vertical in [false, true] {
        let (outer, inner) = if vertical { (w, h) } else { (h, w) };
        for a in 0..outer {
            let mut run = Vec::new();
            for z in 0..inner {
                let i = if vertical { z * w + a } else { a * w + z };
                if walls[i] {
                    if !run.is_empty() {
                        runs.push(std::mem::take(&mut run));
                    }
                } else {
                    run.push(i);
                }
            }
            if !run.is_empty() {
                runs.push(run);
            }
        }
    }
    ensure!(!runs.is_empty(), "no playable cells");
    for &(c, name, colour) in &colours {
        let see_name = format!("see-{name}");
        let lit_name = format!("lit-{name}");
        let see = b.con_bool_matrix(&see_name, &[0..=runs.len() as i64 - 1]);
        let lit = b.con_bool_matrix(&lit_name, &[0..=(w * h) as i64 - 1]);
        for (ri, run) in runs.iter().enumerate() {
            if run.len() < 2 {
                continue;
            }
            let guard = b.guard(
                see.get(&[ri as i64]),
                &see_name,
                format!("two {colour} bulbs may not see each other (run {ri})"),
            )?;
            b.sum_le(
                guard,
                &run.iter().map(|&i| has(i, c)).collect::<Vec<_>>(),
                1,
            )?;
        }
        for (i, &wall) in walls.iter().enumerate() {
            if wall {
                continue;
            }
            let mut cross = Vec::new();
            for run in &runs {
                if run.contains(&i) {
                    for &j in run {
                        if !cross.contains(&j) {
                            cross.push(j);
                        }
                    }
                }
            }
            let guard = b.guard(
                lit.get(&[i as i64]),
                &lit_name,
                format!("cell ({},{}) must be lit by {colour}", i % w, i / w),
            )?;
            b.sum_ge(
                guard,
                &cross.iter().map(|&j| has(j, c)).collect::<Vec<_>>(),
                1,
            )?;
        }
    }
    if !clues.is_empty() {
        let count = b.con_bool_matrix("clue-count", &[0..=clues.len() as i64 - 1]);
        let ban = (variant != "relaxed")
            .then(|| b.con_bool_matrix("clue-ban", &[0..=clues.len() as i64 - 1]));
        for (ci, clue) in clues.iter().enumerate() {
            let i = clue["idx"].as_u64().context("invalid clue index")? as usize;
            ensure!(i < walls.len() && walls[i], "clue must be on a wall");
            let c = match clue["color"].as_str() {
                Some("R" | "red") => 1,
                Some("B" | "blue") => 2,
                Some("G" | "green") if variant == "three" => 3,
                _ => bail!("invalid clue colour"),
            };
            let n = clue["count"].as_i64().context("invalid clue count")?;
            let adj: Vec<_> = neighbours(i, w, h)
                .into_iter()
                .filter(|&j| !walls[j])
                .collect();
            ensure!((0..=adj.len() as i64).contains(&n), "invalid clue count");
            let guard = b.guard(
                count.get(&[ci as i64]),
                "clue-count",
                format!(
                    "Wall {},{} needs {n} {} bulbs beside it (clue {ci})",
                    i % w,
                    i / w,
                    colours[(c - 1) as usize].2
                ),
            )?;
            b.sum_eq(
                guard,
                &adj.iter().map(|&j| has(j, c)).collect::<Vec<_>>(),
                n,
            )?;
            if !adj.is_empty() && variant != "relaxed" {
                let guard = b.guard(
                    ban.as_ref()
                        .expect("exclusive clues have a ban")
                        .get(&[ci as i64]),
                    "clue-ban",
                    format!(
                        "No other-colour bulb beside wall {},{} (clue {ci})",
                        i % w,
                        i / w
                    ),
                )?;
                b.sum_eq(
                    guard,
                    &adj.iter()
                        .flat_map(|&j| {
                            colours
                                .iter()
                                .filter(move |&&(other, _, _)| other != c)
                                .map(move |&(other, _, _)| has(j, other))
                        })
                        .collect::<Vec<_>>(),
                    0,
                )?;
            }
        }
    }
    Ok(b.build()?)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4,
        "usage: mauve_variants normal|three|relaxed LEVEL.json PARSED.json"
    );
    ensure!(!Path::new(&args[3]).exists(), "output already exists");
    let d: Value = serde_json::from_reader(std::fs::File::open(&args[2])?)?;
    let (w, h) = (dimension(&d, "width")?, dimension(&d, "height")?);
    let puzzle = Arc::new(match args[1].as_str() {
        "normal" | "three" | "relaxed" => mauve(&d, w, h, &args[1])?,
        _ => bail!("expected normal, three or relaxed"),
    });
    let mut solver = PuzzleSolver::new_with_config(
        puzzle.clone(),
        SolverConfig {
            only_assignments: true,
        },
    )?;
    ensure!(
        solver.is_currently_solvable(),
        "encoded level is unsatisfiable"
    );
    let reference = d["solution"]
        .as_array()
        .context("missing reference solution")?;
    ensure!(reference.len() == w * h, "reference has incorrect length");
    let proven = solver.get_provable_varlits();
    for (i, value) in reference.iter().enumerate() {
        let (var, val) = (
            PuzVar::new("cell", vec![i as i64]),
            match value.as_str() {
                Some("empty") => 0,
                Some("R") => 1,
                Some("B") => 2,
                Some("G") if args[1] == "three" => 3,
                _ => bail!("invalid reference colour"),
            },
        );
        let lit = PuzLit::new_eq(VarValPair::new(&var, val));
        let sat_lit = puzzle
            .direct
            .litmap
            .get(&lit)
            .context("reference outside domain")?;
        ensure!(
            proven.contains(sat_lit),
            "reference cell {i} is not uniquely implied"
        );
    }
    puzzle.save_to_json(Path::new(&args[3]))?;
    println!(
        "{} {w}x{h}: every reference value is uniquely implied; saved {}",
        args[1], args[3]
    );
    Ok(())
}
