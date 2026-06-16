//! Planner wrapper for Lua.
//!
//! This module provides the [`LuaPlanner`] type, which wraps the puzzle planner
//! and exposes step-by-step solving functionality to Lua. The planner computes
//! minimal explanations (MUSes) for each deduction step.
//!
//! # Usage
//!
//! ```lua
//! local planner = demystify.Planner.new(puzzle)
//!
//! while not planner:is_solved() do
//!     local step = planner:best_step()
//!     if step then
//!         print("Deduced:", step.literals)
//!         print("Using:", step.constraints)
//!     end
//! end
//! ```
//!
//! # Lua Methods
//!
//! | Method | Description |
//! |--------|-------------|
//! | `is_solved()` | Returns true if puzzle is fully solved |
//! | `num_provable()` | Returns count of currently provable literals |
//! | `provable_literals()` | Returns array of provable literal strings |
//! | `best_step()` | Returns next deduction step with smallest MUS |
//! | `quick_solve()` | Solves entire puzzle, returns all steps |
//! | `difficulties()` | Returns difficulty (MUS size) for each deduction |
//! | `check_solvability([lits])` | Classifies solvability (optionally under a partial assignment); reports fixed/unfixed vars |
//! | `known_literals()` | Returns array of all known literals |
//! | `current_state()` | Returns nested table of current assignments |
//! | `fix_literal(str)` | Fixes a literal by string; errors if unknown/unfixable |
//! | `fix_var(name, indices, value)` | Fixes literal by components; errors if unknown/unfixable |
//! | `fix(table)` | Fixes literal by `{name, indices, value, equal?}` table; errors if unknown/unfixable |
//!
//! # Step Structure
//!
//! The `best_step()` method returns a table with:
//! - `literals`: Array of deduced literal strings
//! - `constraints`: Array of constraint names used in the proof
//! - `mus_size`: Number of constraints in the minimal proof
//! - `num_muses`: Number of alternative minimal proofs found

use std::sync::{Arc, Mutex};

use mlua::prelude::*;

use demystify::problem::{
    PuzLit, PuzVar, VarValPair, format_puzlit, format_puzvar, planner::PuzzlePlanner,
    solver::PuzzleSolver,
};

use crate::puzzle::LuaPuzzle;

fn fix_reject_error(reason: String) -> LuaError {
    LuaError::RuntimeError(reason)
}

/// Normalise a literal string for matching by dropping all whitespace, so
/// callers needn't reproduce `format_puzlit`'s exact spacing: `grid[1,1]=5`,
/// `grid[1, 1]=5`, and `grid[1, 1] = 5` all match the canonical form.  This
/// is collision-free because two distinct puzzle literals never differ only
/// in whitespace (variable names are alphanumeric).
fn lit_match_key(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// A wrapper around PuzzlePlanner that can be used from Lua.
pub struct LuaPlanner {
    inner: Arc<Mutex<PuzzlePlanner>>,
}

impl LuaUserData for LuaPlanner {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        // Check if the puzzle is fully solved
        methods.add_method("is_solved", |_, this, ()| {
            let planner = this.inner.lock().unwrap();
            Ok(!planner.get_all_known_lits().is_empty()
                && planner.puzzle().var_lits.positive().iter().all(|lit| {
                    planner.get_all_known_lits().contains(lit)
                        || planner.get_all_known_lits().contains(&(!*lit))
                }))
        });

        // Get the number of remaining provable literals
        methods.add_method_mut("num_provable", |_, this, ()| {
            let mut planner = this.inner.lock().unwrap();
            Ok(planner.get_provable_varlits().len())
        });

        // Get the list of provable literals
        methods.add_method_mut("provable_literals", |lua, this, ()| {
            let mut planner = this.inner.lock().unwrap();
            let provable = planner.get_provable_varlits();

            let result = lua.create_table()?;
            let mut idx = 1;
            for lit in &provable {
                let puzlits = planner.puzzle().lit_to_vars(lit);
                for puzlit in puzlits {
                    result.set(idx, format_puzlit(puzlit))?;
                    idx += 1;
                }
            }

            Ok(result)
        });

        // Manually fix/deduce a literal by its string representation, in the
        // form "var[i,j,...]=val" or "var[i,j,...]!=val" (whitespace-insensitive).
        // Errors if the string names no puzzle literal, or its variable can't
        // be fixed.
        methods.add_method_mut("fix_literal", |_, this, lit_str: String| {
            let mut planner = this.inner.lock().unwrap();
            let key = lit_match_key(&lit_str);

            let matching_lit = {
                let mut found = None;
                for (puzlit, sat_lit) in planner.puzzle().direct.litmap.iter() {
                    if lit_match_key(&format_puzlit(puzlit)) == key {
                        found = Some((*sat_lit, puzlit.var().name().clone()));
                        break;
                    }
                }
                found
            };

            let (lit, var_name) = matching_lit
                .ok_or_else(|| LuaError::RuntimeError(format!("unknown literal: {lit_str}")))?;
            if let Err(reason) = planner.puzzle().check_fixable_var(&var_name) {
                return Err(fix_reject_error(reason));
            }
            planner.mark_lit_as_fixed(&lit);
            Ok(())
        });

        // Fix a literal by its components (positional arguments)
        // Args: name (string), indices (table of integers), value (integer)
        // Returns: true if literal was found and fixed, false otherwise
        methods.add_method_mut(
            "fix_var",
            |_, this, (name, indices, value): (String, LuaTable, i64)| {
                let mut planner = this.inner.lock().unwrap();

                // Convert Lua table to Vec<i64>
                let mut idx_vec: Vec<i64> = Vec::new();
                for pair in indices.pairs::<i64, i64>() {
                    let (_, idx) = pair?;
                    idx_vec.push(idx);
                }

                // Build PuzVar and PuzLit
                let puzvar = PuzVar::new(&name, idx_vec);
                if let Err(reason) = planner.puzzle().check_fixable_var(&name) {
                    return Err(fix_reject_error(reason));
                }
                let varval = VarValPair::new(&puzvar, value);
                let puzlit = PuzLit::new_eq(varval);

                // Look up in litmap
                match planner.puzzle().direct.litmap.get(&puzlit) {
                    Some(&sat_lit) => {
                        planner.mark_lit_as_fixed(&sat_lit);
                        Ok(())
                    }
                    None => Err(LuaError::RuntimeError(format!(
                        "unknown literal: {}",
                        format_puzlit(&puzlit)
                    ))),
                }
            },
        );

        // Fix a literal by its components (table argument)
        // Args: table with {name = string, indices = table, value = integer, [equal = boolean]}
        // Returns: true if literal was found and fixed, false otherwise
        methods.add_method_mut("fix", |_, this, args: LuaTable| {
            let mut planner = this.inner.lock().unwrap();

            // Extract name
            let name: String = args
                .get("name")
                .map_err(|_| LuaError::RuntimeError("missing 'name' field".to_string()))?;

            // Extract indices
            let indices_table: LuaTable = args
                .get("indices")
                .map_err(|_| LuaError::RuntimeError("missing 'indices' field".to_string()))?;

            let mut idx_vec: Vec<i64> = Vec::new();
            for pair in indices_table.pairs::<i64, i64>() {
                let (_, idx) = pair?;
                idx_vec.push(idx);
            }

            // Extract value
            let value: i64 = args
                .get("value")
                .map_err(|_| LuaError::RuntimeError("missing 'value' field".to_string()))?;

            // Extract optional 'equal' field (defaults to true)
            let equal: bool = args.get("equal").unwrap_or(true);

            // Build PuzVar and PuzLit
            let puzvar = PuzVar::new(&name, idx_vec);
            if let Err(reason) = planner.puzzle().check_fixable_var(&name) {
                return Err(fix_reject_error(reason));
            }
            let varval = VarValPair::new(&puzvar, value);
            let puzlit = if equal {
                PuzLit::new_eq(varval)
            } else {
                PuzLit::new_neq(varval)
            };

            // Look up in litmap
            match planner.puzzle().direct.litmap.get(&puzlit) {
                Some(&sat_lit) => {
                    planner.mark_lit_as_fixed(&sat_lit);
                    Ok(())
                }
                None => Err(LuaError::RuntimeError(format!(
                    "unknown literal: {}",
                    format_puzlit(&puzlit)
                ))),
            }
        });

        // Get the best next step
        methods.add_method_mut("best_step", |lua, this, ()| {
            let mut planner = this.inner.lock().unwrap();

            let muses = planner.smallest_muses_with_config();

            if muses.is_empty() {
                return Ok(LuaValue::Nil);
            }

            // Convert to user-friendly representation
            let mut all_lits = Vec::new();
            let mut all_constraints = Vec::new();

            for mus in &muses {
                let user_mus = planner.mus_to_user_mus(mus);
                for lit in user_mus.lits {
                    all_lits.push(format_puzlit(&lit));
                }
                all_constraints.extend(user_mus.constraints);
            }

            // Mark the literals as deduced
            for mus in &muses {
                for lit in &mus.lits {
                    planner.mark_lit_as_deduced(lit);
                }
            }

            // Build the result table
            let result = lua.create_table()?;

            let literals_table = lua.create_table()?;
            for (i, lit) in all_lits.iter().enumerate() {
                literals_table.set(i + 1, lit.clone())?;
            }
            result.set("literals", literals_table)?;

            let constraints_table = lua.create_table()?;
            for (i, con) in all_constraints.iter().enumerate() {
                constraints_table.set(i + 1, con.clone())?;
            }
            result.set("constraints", constraints_table)?;

            result.set("mus_size", muses.first().map(|m| m.mus_len()).unwrap_or(0))?;
            result.set("num_muses", muses.len())?;

            Ok(LuaValue::Table(result))
        });

        // Solve the entire puzzle and return all steps
        methods.add_method_mut("quick_solve", |lua, this, ()| {
            let mut planner = this.inner.lock().unwrap();
            let solve = planner.quick_solve();

            let result = lua.create_table()?;
            for (step_idx, step) in solve.iter().enumerate() {
                let step_table = lua.create_table()?;

                for (mus_idx, um) in step.iter().enumerate() {
                    let mus_table = lua.create_table()?;

                    let literals_table = lua.create_table()?;
                    for (i, lit) in um.lits.iter().enumerate() {
                        literals_table.set(i + 1, format_puzlit(lit))?;
                    }
                    mus_table.set("literals", literals_table)?;

                    let constraints_table = lua.create_table()?;
                    for (i, con) in um.constraints.iter().enumerate() {
                        constraints_table.set(i + 1, con.clone())?;
                    }
                    mus_table.set("constraints", constraints_table)?;

                    mus_table.set("fingerprint", um.fingerprint.clone())?;
                    if let Some(name) = &um.name {
                        mus_table.set("name", name.clone())?;
                    }

                    step_table.set(mus_idx + 1, mus_table)?;
                }

                result.set(step_idx + 1, step_table)?;
            }

            Ok(result)
        });

        // Get difficulties for every provable lit.  Lits the MUS search
        // couldn't size are reported with a sentinel equal to the
        // puzzle's total constraint count — a strict upper bound on any
        // real MUS size, so they always sort to the bottom.  One SAT
        // lit can witness multiple PuzLits via the direct encoding
        // (cell=0 ⇔ cell!=1 ∧ cell!=2 for a 0..2 domain); we expand all
        // of them so the result matches provable_literals().
        methods.add_method_mut("difficulties", |lua, this, ()| {
            let mut planner = this.inner.lock().unwrap();
            let muses = planner.all_muses_with_larger();

            let result = lua.create_table()?;
            let mut sized: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for (lit, mus_set) in muses.muses() {
                if let Some(min_len) = mus_set.iter().map(|mc| mc.mus_len()).min() {
                    for puzlit in planner.puzzle().lit_to_vars(lit) {
                        let lit_str = format_puzlit(puzlit);
                        result.set(lit_str.clone(), min_len)?;
                        sized.insert(lit_str);
                    }
                }
            }

            let sentinel = planner.puzzle().constraints.len();
            let provable = planner.get_provable_varlits().clone();
            for lit in &provable {
                for puzlit in planner.puzzle().lit_to_vars(lit) {
                    let lit_str = format_puzlit(puzlit);
                    if !sized.contains(&lit_str) {
                        result.set(lit_str, sentinel)?;
                    }
                }
            }

            Ok(result)
        });

        // Classify solvability, optionally under a partial assignment,
        // without mutating the caller's state (it solves a clone). The
        // optional argument is a list of "var=val" / "var!=val" strings,
        // each pinned as an assumption before solving -- so this doubles as
        // a "is this partial assignment solvable, and what does it force?"
        // check. Returns a table { status = "unsolvable" | "unique" |
        // "multiple", ... }:
        //   - "unique"   adds `fixed_vars`: the "var=val" each variable
        //     resolved to (every variable is determined).
        //   - "multiple" adds `fixed_vars` (the variables propagation pins,
        //     with values) and `unfixed_vars` (names of those still free).
        // Errors if a supplied literal doesn't resolve to a known puzzle
        // literal. Mirrors `WasmPlanner::checkSolvability`.
        methods.add_method_mut(
            "check_solvability",
            |lua, this, literals: Option<LuaTable>| {
                let mut planner = this.inner.lock().unwrap().clone();

                if let Some(tbl) = literals {
                    let mut assume: Vec<String> = Vec::new();
                    for s in tbl.sequence_values::<String>() {
                        assume.push(s?);
                    }
                    if !assume.is_empty() {
                        let lookup: std::collections::HashMap<String, _> = planner
                            .puzzle()
                            .direct
                            .litmap
                            .iter()
                            .map(|(puzlit, sat_lit)| {
                                (lit_match_key(&format_puzlit(puzlit)), *sat_lit)
                            })
                            .collect();
                        for s in &assume {
                            match lookup.get(&lit_match_key(s)) {
                                Some(lit) => planner.mark_lit_as_fixed(lit),
                                None => {
                                    return Err(LuaError::RuntimeError(format!(
                                        "unknown literal: {s}"
                                    )));
                                }
                            }
                        }
                    }
                }

                let result = lua.create_table()?;
                if planner.check_solvability().is_none() {
                    result.set("status", "unsolvable")?;
                    return Ok(result);
                }

                // Every decided variable contributes its "var=val" equality
                // puzlit; the matching "!=" puzlits are dropped by the sign filter.
                let known: std::collections::HashSet<_> =
                    planner.get_all_known_lits().iter().copied().collect();
                let fixed = lua.create_table()?;
                let mut idx = 1;
                for lit in planner.puzzle().var_lits.positive() {
                    if known.contains(lit) {
                        for puzlit in planner.puzzle().lit_to_vars(lit) {
                            if puzlit.sign() {
                                fixed.set(idx, format_puzlit(puzlit))?;
                                idx += 1;
                            }
                        }
                    }
                }
                result.set("fixed_vars", fixed)?;

                let unsolved = planner.unsolved_vars_after_solve();
                if unsolved.is_empty() {
                    result.set("status", "unique")?;
                } else {
                    result.set("status", "multiple")?;
                    let unfixed = lua.create_table()?;
                    for (i, var) in unsolved.iter().enumerate() {
                        unfixed.set(i + 1, format_puzvar(var))?;
                    }
                    result.set("unfixed_vars", unfixed)?;
                }
                Ok(result)
            },
        );

        // Cheaply test whether the puzzle has exactly one solution, optionally
        // under a partial assignment.  Returns a boolean: true iff exactly one
        // solution exists (unsolvable and multiple-solution puzzles return
        // false).  Runs on a cloned planner, so the caller's state is untouched.
        methods.add_method_mut(
            "is_uniquely_solvable",
            |_lua, this, literals: Option<LuaTable>| {
                let mut planner = this.inner.lock().unwrap().clone();

                if let Some(tbl) = literals {
                    let mut assume: Vec<String> = Vec::new();
                    for s in tbl.sequence_values::<String>() {
                        assume.push(s?);
                    }
                    if !assume.is_empty() {
                        let lookup: std::collections::HashMap<String, _> = planner
                            .puzzle()
                            .direct
                            .litmap
                            .iter()
                            .map(|(puzlit, sat_lit)| {
                                (lit_match_key(&format_puzlit(puzlit)), *sat_lit)
                            })
                            .collect();
                        for s in &assume {
                            match lookup.get(&lit_match_key(s)) {
                                Some(lit) => planner.mark_lit_as_fixed(lit),
                                None => {
                                    return Err(LuaError::RuntimeError(format!(
                                        "unknown literal: {s}"
                                    )));
                                }
                            }
                        }
                    }
                }

                Ok(planner.is_uniquely_solvable())
            },
        );

        // Get all known literals
        methods.add_method("known_literals", |lua, this, ()| {
            let planner = this.inner.lock().unwrap();
            let result = lua.create_table()?;

            let mut idx = 1;
            for lit in planner.get_all_known_lits() {
                for puzlit in planner.puzzle().lit_to_vars(lit) {
                    result.set(idx, format_puzlit(puzlit))?;
                    idx += 1;
                }
            }

            Ok(result)
        });

        // Get current assignments as a table
        methods.add_method("current_state", |lua, this, ()| {
            let planner = this.inner.lock().unwrap();
            let result = lua.create_table()?;

            for lit in planner.get_all_known_lits() {
                let puzlits = planner.puzzle().lit_to_vars(lit);
                for puzlit in puzlits {
                    if puzlit.sign() {
                        // This is a positive assignment (var = val)
                        let var_name = puzlit.var().name().clone();
                        let indices = puzlit.var().indices().clone();
                        let val = puzlit.val();

                        // Build nested table structure for indexed variables
                        let mut current = result.clone();

                        // Get or create the variable's table
                        let var_table: LuaTable = match current.get::<LuaTable>(var_name.clone()) {
                            Ok(t) => t,
                            Err(_) => {
                                let t = lua.create_table()?;
                                current.set(var_name.clone(), t.clone())?;
                                t
                            }
                        };
                        current = var_table;

                        // Navigate/create nested tables for indices
                        for (idx_pos, &idx) in indices.iter().enumerate() {
                            if idx_pos == indices.len() - 1 {
                                // Last index - set the value
                                current.set(idx, val)?;
                            } else {
                                // Not the last - get or create nested table
                                let next: LuaTable = match current.get::<LuaTable>(idx) {
                                    Ok(t) => t,
                                    Err(_) => {
                                        let t = lua.create_table()?;
                                        current.set(idx, t.clone())?;
                                        t
                                    }
                                };
                                current = next;
                            }
                        }

                        // Handle variables with no indices
                        if indices.is_empty() {
                            result.set(var_name, val)?;
                        }
                    }
                }
            }

            Ok(result)
        });
    }
}

/// Create a new planner from a puzzle
fn new_planner(_lua: &Lua, puzzle: LuaPuzzle) -> LuaResult<LuaPlanner> {
    let solver = PuzzleSolver::new(puzzle.inner)
        .map_err(|e| LuaError::RuntimeError(format!("Failed to create solver: {}", e)))?;

    let planner = PuzzlePlanner::new(solver);

    Ok(LuaPlanner {
        inner: Arc::new(Mutex::new(planner)),
    })
}

/// Create the Planner class table for Lua
pub fn create_planner_class(lua: &Lua) -> LuaResult<LuaTable> {
    let class = lua.create_table()?;

    class.set("new", lua.create_function(new_planner)?)?;

    Ok(class)
}
