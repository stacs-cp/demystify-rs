//! Puzzle wrapper for Lua.

use std::path::PathBuf;
use std::sync::Arc;

use mlua::FromLua;
use mlua::prelude::*;

use demystify::problem::parse::PuzzleParse;

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
            Ok(this.inner.varset_lits.len())
        });

        // Get the number of constraint literals
        methods.add_method("num_con_lits", |_, this, ()| {
            Ok(this.inner.conset_lits.len())
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
