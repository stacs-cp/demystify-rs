//! Puzzle wrapper for Lua.
//!
//! This module provides the [`LuaPuzzle`] type, which wraps a parsed puzzle
//! and exposes it to Lua code. The puzzle object provides methods for:
//!
//! - Querying puzzle metadata (kind, info, parameters)
//! - Listing variables and constraints
//! - Inspecting variable domains
//! - Finding which variables are involved in constraints
//!
//! # Lua Methods
//!
//! | Method | Description |
//! |--------|-------------|
//! | `kind()` | Returns puzzle type (e.g., "Sudoku") |
//! | `info()` | Returns array of info strings |
//! | `variables()` | Returns array of variable names |
//! | `aux_variables()` | Returns array of auxiliary variable names |
//! | `constraints()` | Returns array of constraint names |
//! | `constraint(name)` | Returns constraint text by name |
//! | `variable_domain(var)` | Returns possible values for a variable |
//! | `all_domains()` | Returns all variable domains |
//! | `constraint_variables(name)` | Returns variables involved in a constraint |
//! | `has_param(name)` | Checks if parameter exists |
//! | `param_int(name)` | Gets integer parameter |
//! | `param_bool(name)` | Gets boolean parameter |
//! | `num_clauses()` | Returns number of CNF clauses |
//! | `num_var_lits()` | Returns number of variable literals |
//! | `num_con_lits()` | Returns number of constraint literals |

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use mlua::FromLua;
use mlua::prelude::*;

use demystify::problem::parse::PuzzleParse;
use demystify::problem::{format_puzvar, parse_var_string};

/// A wrapper around PuzzleParse that can be used from Lua.
#[derive(Clone)]
pub struct LuaPuzzle {
    pub(crate) inner: Arc<PuzzleParse>,
}

impl FromLua for LuaPuzzle {
    fn from_lua(value: LuaValue, _lua: &Lua) -> LuaResult<Self> {
        match value {
            LuaValue::UserData(ud) => ud.borrow::<LuaPuzzle>().map(|p| p.clone()),
            _ => Err(LuaError::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaPuzzle".to_string(),
                message: Some("expected a Puzzle userdata".to_string()),
            }),
        }
    }
}

impl LuaUserData for LuaPuzzle {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        // Get the puzzle kind (e.g., "sudoku", "binairo")
        methods.add_method("kind", |_, this, ()| Ok(this.inner.eprime.kind.clone()));

        // Get info strings from the puzzle
        methods.add_method("info", |lua, this, ()| {
            let table = lua.create_table()?;
            for (i, info) in this.inner.eprime.info.iter().enumerate() {
                table.set(i + 1, info.clone())?;
            }
            Ok(table)
        });

        // Get the list of variable names
        methods.add_method("variables", |lua, this, ()| {
            let table = lua.create_table()?;
            for (i, var) in this.inner.eprime.vars.iter().enumerate() {
                table.set(i + 1, var.clone())?;
            }
            Ok(table)
        });

        // Get the list of auxiliary variable names
        methods.add_method("aux_variables", |lua, this, ()| {
            let table = lua.create_table()?;
            for (i, var) in this.inner.eprime.auxvars.iter().enumerate() {
                table.set(i + 1, var.clone())?;
            }
            Ok(table)
        });

        // Get the list of constraint names
        methods.add_method("constraints", |lua, this, ()| {
            let table = lua.create_table()?;
            for (i, (name, _)) in this.inner.eprime.cons.iter().enumerate() {
                table.set(i + 1, name.clone())?;
            }
            Ok(table)
        });

        // Get a constraint by name
        methods.add_method("constraint", |_, this, name: String| {
            Ok(this.inner.eprime.cons.get(&name).cloned())
        });

        // Get the number of CNF clauses
        methods.add_method("num_clauses", |_, this, ()| {
            Ok(this.inner.cnf.as_ref().map(|c| c.len()).unwrap_or(0))
        });

        // Get the number of variables in the direct encoding
        methods.add_method("num_var_lits", |_, this, ()| {
            Ok(this.inner.var_lits.positive().len())
        });

        // Get the number of constraint literals
        methods.add_method("num_con_lits", |_, this, ()| {
            Ok(this.inner.constraints.len())
        });

        // Check if a parameter exists
        methods.add_method("has_param", |_, this, name: String| {
            Ok(this.inner.eprime.has_param(&name))
        });

        // Get a parameter as an integer
        methods.add_method("param_int", |_, this, name: String| {
            this.inner
                .eprime
                .param_i64(&name)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))
        });

        // Get a parameter as a boolean
        methods.add_method("param_bool", |_, this, name: String| {
            this.inner
                .eprime
                .param_bool(&name)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))
        });

        // Get the domain (possible values) for a specific variable
        // Input: variable string like "grid[1, 2]" or "x"
        // Returns: table of possible values, or nil if variable not found
        methods.add_method("variable_domain", |lua, this, var_str: String| {
            let puzvar = match parse_var_string(&var_str) {
                Some(v) => v,
                None => {
                    return Err(LuaError::RuntimeError(format!(
                        "Invalid variable format: {}",
                        var_str
                    )));
                }
            };

            // Look up in domainmap
            if let Some(domain) = this.inner.direct.domainmap.get(&puzvar) {
                let table = lua.create_table()?;
                for (i, &val) in domain.iter().enumerate() {
                    table.set(i + 1, val)?;
                }
                Ok(LuaValue::Table(table))
            } else {
                Ok(LuaValue::Nil)
            }
        });

        // Get all variable domains as a table
        // Returns: { "var_name[indices]" = {values...}, ... }
        methods.add_method("all_domains", |lua, this, ()| {
            let table = lua.create_table()?;

            for (puzvar, domain) in &this.inner.direct.domainmap {
                let var_str = format_puzvar(puzvar);
                let domain_table = lua.create_table()?;
                for (i, &val) in domain.iter().enumerate() {
                    domain_table.set(i + 1, val)?;
                }
                table.set(var_str, domain_table)?;
            }

            Ok(table)
        });

        // Get the variables involved in a constraint
        // Input: constraint name
        // Returns: table of variable strings, or nil if constraint not found
        methods.add_method("constraint_variables", |lua, this, con_name: String| {
            // Look up constraint literal
            let con_lit = if this
                .inner
                .constraints
                .descriptions()
                .any(|n| *n == con_name)
            {
                *this.inner.constraints.lit_for(&con_name)
            } else {
                return Ok(LuaValue::Nil);
            };

            let var_lits = this.inner.constraints.var_lits(&con_lit);

            // Collect unique variable names
            let mut var_names: BTreeSet<String> = BTreeSet::new();
            for lit in var_lits {
                if let Some(puzlits) = this.inner.direct.invlitmap.get(lit) {
                    for puzlit in puzlits {
                        var_names.insert(format_puzvar(&puzlit.var()));
                    }
                }
            }

            // Build result table
            let table = lua.create_table()?;
            for (i, name) in var_names.iter().enumerate() {
                table.set(i + 1, name.clone())?;
            }

            Ok(LuaValue::Table(table))
        });
    }
}

/// Load a puzzle from a JSON file.
///
/// This function loads a pre-parsed puzzle that was saved using
/// `demystify --save-parsed`.
pub fn load_puzzle(_lua: &Lua, path: String) -> LuaResult<LuaPuzzle> {
    let path = PathBuf::from(path);
    let puzzle = PuzzleParse::load_from_json(&path)
        .map_err(|e| LuaError::RuntimeError(format!("Failed to load puzzle: {}", e)))?;

    Ok(LuaPuzzle {
        inner: Arc::new(puzzle),
    })
}
