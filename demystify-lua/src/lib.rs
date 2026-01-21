//! Lua bindings for the demystify constraint solver.
//!
//! This module provides a Lua interface for loading pre-parsed puzzles
//! and solving them step-by-step with explanations.
//!
//! # Example
//!
//! ```lua
//! local demystify = require("demystify")
//!
//! -- Load a pre-parsed puzzle
//! local puzzle = demystify.load_puzzle("sudoku.json")
//!
//! -- Create a planner
//! local planner = demystify.Planner.new(puzzle)
//!
//! -- Solve step by step
//! while not planner:is_solved() do
//!     local step = planner:best_step()
//!     print("Deduced:", step.literals)
//!     print("Using constraints:", table.concat(step.constraints, ", "))
//! end
//! ```

mod planner;
mod puzzle;

use mlua::prelude::*;

/// Main entry point for the Lua module.
///
/// This function is called when Lua loads the module with `require("demystify")`.
#[mlua::lua_module]
fn demystify(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;

    // Add the load_puzzle function
    exports.set(
        "load_puzzle",
        lua.create_function(puzzle::load_puzzle)?,
    )?;

    // Add the Planner class
    exports.set("Planner", planner::create_planner_class(lua)?)?;

    // Add version info
    exports.set("version", env!("CARGO_PKG_VERSION"))?;

    Ok(exports)
}
