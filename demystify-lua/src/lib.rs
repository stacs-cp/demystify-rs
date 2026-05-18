//! # Demystify Lua Bindings
//!
//! Lua bindings for the demystify constraint solver, providing a complete
//! interface for loading puzzles and solving them step-by-step with explanations.
//!
//! ## Installation
//!
//! Build the library with cargo:
//!
//! ```bash
//! cargo build --release -p demystify-lua
//! ```
//!
//! This creates `libdemystify_lua.so` (Linux), `libdemystify_lua.dylib` (macOS),
//! or `demystify_lua.dll` (Windows) in `target/release/`.
//!
//! To use with LuaJIT, create a symlink:
//!
//! ```bash
//! ln -s target/release/libdemystify_lua.so demystify.so
//! ```
//!
//! ## Quick Start
//!
//! ```lua
//! local demystify = require("demystify")
//!
//! -- Load a pre-parsed puzzle
//! local puzzle = demystify.load_puzzle("sudoku.json")
//!
//! -- Create a planner for step-by-step solving
//! local planner = demystify.Planner.new(puzzle)
//!
//! -- Solve and print each step
//! while not planner:is_solved() do
//!     local step = planner:best_step()
//!     if step then
//!         print("Deduced: " .. table.concat(step.literals, ", "))
//!         print("Using: " .. table.concat(step.constraints, ", "))
//!     end
//! end
//! ```
//!
//! ## Module Contents
//!
//! The module exports:
//!
//! - `demystify.load_puzzle(path)` - Load a pre-parsed puzzle from JSON
//! - `demystify.Planner` - Class for step-by-step solving
//! - `demystify.helpers` - Utility functions for parsing literals
//! - `demystify.version` - Library version string
//!
//! ## Puzzle Object Methods
//!
//! After loading a puzzle with `load_puzzle()`, you can query its properties:
//!
//! ```lua
//! local puzzle = demystify.load_puzzle("puzzle.json")
//!
//! -- Metadata
//! print(puzzle:kind())              -- Puzzle type: "Sudoku", "Binairo", etc.
//! local info = puzzle:info()        -- Array of info strings
//!
//! -- Variables and constraints
//! local vars = puzzle:variables()   -- Array of variable names
//! local cons = puzzle:constraints() -- Array of constraint names
//! local text = puzzle:constraint("row_sum(1)")  -- Get constraint text
//!
//! -- Variable domains (possible values)
//! local domain = puzzle:variable_domain("grid[1, 2]")  -- {1, 2, 3, ...}
//! local all = puzzle:all_domains()  -- All domains as a table
//!
//! -- Constraint structure
//! local vars = puzzle:constraint_variables("row_sum(1)")  -- Variables in constraint
//!
//! -- Parameters
//! if puzzle:has_param("size") then
//!     print(puzzle:param_int("size"))
//! end
//!
//! -- Statistics
//! print("CNF clauses:", puzzle:num_clauses())
//! print("Variable literals:", puzzle:num_var_lits())
//! print("Constraint literals:", puzzle:num_con_lits())
//! ```
//!
//! ## Planner Object Methods
//!
//! The planner provides step-by-step solving with explanations:
//!
//! ```lua
//! local planner = demystify.Planner.new(puzzle)
//!
//! -- Check solving status
//! print("Solved:", planner:is_solved())
//! print("Provable literals:", planner:num_provable())
//!
//! -- Get provable literals
//! local provable = planner:provable_literals()  -- {"grid[1,1]=5", ...}
//!
//! -- Get next solving step (smallest MUS)
//! local step = planner:best_step()
//! if step then
//!     print("Literals:", step.literals)      -- Deduced values
//!     print("Constraints:", step.constraints) -- Justifying constraints
//!     print("MUS size:", step.mus_size)       -- Number of constraints needed
//!     print("Num MUSes:", step.num_muses)     -- Alternative explanations
//! end
//!
//! -- Manually fix a literal (by string)
//! planner:fix_literal("grid[1, 1]=5")
//!
//! -- Manually fix a literal (by components)
//! planner:fix_var("grid", {1, 2}, 5)
//! planner:fix({name = "grid", indices = {1, 2}, value = 5})
//! planner:fix({name = "grid", indices = {1, 2}, value = 5, equal = false})  -- inequality
//!
//! -- Solve entire puzzle at once
//! local steps = planner:quick_solve()
//! for i, step in ipairs(steps) do
//!     for j, mus in ipairs(step) do
//!         print("Step", i, "MUS", j, ":", mus.literals, mus.constraints)
//!     end
//! end
//!
//! -- Get difficulty ratings for all deductions
//! local difficulties = planner:difficulties()
//! for lit, size in pairs(difficulties) do
//!     print(lit, "requires", size, "constraints")
//! end
//!
//! -- Get current state
//! local known = planner:known_literals()  -- All known literals
//! local state = planner:current_state()   -- Nested table of assignments
//! -- state.grid[1][2] = 5
//! ```
//!
//! ## Helpers Module
//!
//! The `demystify.helpers` module provides utilities for parsing literal strings:
//!
//! ```lua
//! local h = demystify.helpers
//!
//! -- Parse a literal string
//! local lit = h.parse_literal("grid[1, 2]=5")
//! print(lit.name)     -- "grid"
//! print(lit.indices)  -- {1, 2}
//! print(lit.value)    -- 5
//! print(lit.equal)    -- true (false for "!=")
//!
//! -- Parse a variable string
//! local var = h.parse_variable("grid[1, 2]")
//! print(var.name)     -- "grid"
//! print(var.indices)  -- {1, 2}
//!
//! -- Format back to string
//! print(h.format_literal(lit))   -- "grid[1, 2]=5"
//! print(h.format_variable(var))  -- "grid[1, 2]"
//!
//! -- Check literal type
//! print(h.is_assignment(lit))  -- true (for "=")
//! print(h.is_negation(lit))    -- false (true for "!=")
//!
//! -- Quick variable name extraction
//! print(h.get_variable_name("grid[1, 2]=5"))  -- "grid"
//!
//! -- Batch parsing
//! local lits, errors = h.parse_literals({"grid[1,1]=1", "invalid", "x=2"})
//! ```
//!
//! ## Preparing Puzzles
//!
//! Puzzles must be pre-parsed using the demystify CLI:
//!
//! ```bash
//! demystify --model puzzle.eprime --param instance.param --save-parsed puzzle.json
//! ```
//!
//! This converts Essence Prime definitions to a JSON format that can be loaded
//! quickly without requiring Conjure at runtime.
//!
//! ## Complete Example
//!
//! ```lua
//! local demystify = require("demystify")
//!
//! -- Load and inspect puzzle
//! local puzzle = demystify.load_puzzle("sudoku.json")
//! print("Puzzle type:", puzzle:kind())
//! print("Variables:", #puzzle:variables())
//! print("Constraints:", #puzzle:constraints())
//!
//! -- Create planner and solve
//! local planner = demystify.Planner.new(puzzle)
//! local step_num = 0
//!
//! while not planner:is_solved() do
//!     local step = planner:best_step()
//!     if not step then break end
//!
//!     step_num = step_num + 1
//!     print(string.format("\nStep %d (MUS size %d):", step_num, step.mus_size))
//!
//!     -- Parse and display deductions
//!     for _, lit_str in ipairs(step.literals) do
//!         local lit = demystify.helpers.parse_literal(lit_str)
//!         if lit.equal then
//!             print(string.format("  %s = %d",
//!                 demystify.helpers.format_variable(lit), lit.value))
//!         end
//!     end
//!
//!     print("  Constraints:", table.concat(step.constraints, ", "))
//! end
//!
//! -- Show final state
//! print("\nSolution:")
//! local state = planner:current_state()
//! for var_name, values in pairs(state) do
//!     print(var_name, "=", vim.inspect(values))
//! end
//! ```

mod builder;
mod planner;
mod puzzle;

use mlua::prelude::*;

/// Embedded helpers.lua module for literal parsing utilities.
const HELPERS_LUA: &str = include_str!("../lua/helpers.lua");

/// Main entry point for the Lua module.
///
/// This function is called when Lua loads the module with `require("demystify")`.
///
/// # Returns
///
/// A table containing:
/// - `load_puzzle`: Function to load a pre-parsed puzzle from JSON
/// - `Planner`: Class for creating puzzle planners
/// - `helpers`: Table of literal parsing utilities
/// - `version`: Library version string
#[mlua::lua_module]
fn demystify(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;

    // Add the load_puzzle function
    exports.set("load_puzzle", lua.create_function(puzzle::load_puzzle)?)?;

    // Add the Planner class
    exports.set("Planner", planner::create_planner_class(lua)?)?;

    // Add the Builder class
    exports.set("Builder", builder::create_builder_class(lua)?)?;

    // Add version info
    exports.set("version", env!("CARGO_PKG_VERSION"))?;

    // Load and expose bundled helpers module
    let helpers: LuaTable = lua
        .load(HELPERS_LUA)
        .set_name("helpers.lua")
        .eval()
        .map_err(|e| LuaError::RuntimeError(format!("Failed to load helpers: {}", e)))?;
    exports.set("helpers", helpers)?;

    Ok(exports)
}
