--- Demystify Lua Helpers
-- @module demystify.helpers
-- @description Utility functions for parsing and formatting literal strings.
--
-- This module provides functions to parse literal strings like "grid[1, 2]=5"
-- into structured tables, and to format them back to strings.
--
-- @usage
-- local h = require("demystify").helpers
--
-- -- Parse a literal
-- local lit = h.parse_literal("grid[1, 2]=5")
-- print(lit.name)     -- "grid"
-- print(lit.indices)  -- {1, 2}
-- print(lit.value)    -- 5
-- print(lit.equal)    -- true
--
-- -- Format back to string
-- print(h.format_literal(lit))  -- "grid[1, 2]=5"

local helpers = {}

--- Parse a variable string into its components.
-- Parses strings like "grid[1, 2]" or "x" into a structured table.
--
-- @tparam string var_str The variable string to parse
-- @treturn table|nil A table with `name` (string) and `indices` (array of integers), or nil on failure
-- @treturn string|nil Error message if parsing failed
-- @usage
-- local var = helpers.parse_variable("grid[1, 2]")
-- -- Returns: {name = "grid", indices = {1, 2}}
--
-- local simple = helpers.parse_variable("x")
-- -- Returns: {name = "x", indices = {}}
--
-- local var, err = helpers.parse_variable("invalid[")
-- -- Returns: nil, "invalid variable format: invalid["
function helpers.parse_variable(var_str)
    if type(var_str) ~= "string" then
        return nil, "expected string, got " .. type(var_str)
    end

    -- Match name and optional indices: "grid[1, 2]" or just "grid"
    local name, indices_str = var_str:match("^([%w_]+)%[(.*)%]$")
    if not name then
        -- Try without indices
        name = var_str:match("^([%w_]+)$")
        if name then
            return { name = name, indices = {} }
        end
        return nil, "invalid variable format: " .. var_str
    end

    -- Parse indices (comma-separated integers)
    local indices = {}
    for idx in indices_str:gmatch("%-?%d+") do
        table.insert(indices, tonumber(idx))
    end

    return { name = name, indices = indices }
end

--- Parse a literal string into its components.
-- Parses strings like "grid[1, 2]=3" or "grid[1, 2]!=3" into a structured table.
--
-- @tparam string lit_str The literal string to parse
-- @treturn table|nil A table with fields:
--   - `name` (string): Variable name
--   - `indices` (table): Array of integer indices
--   - `equal` (boolean): true for "=", false for "!="
--   - `value` (number): The assigned value
-- @treturn string|nil Error message if parsing failed
-- @usage
-- local lit = helpers.parse_literal("grid[1, 2]=5")
-- -- Returns: {name = "grid", indices = {1, 2}, equal = true, value = 5}
--
-- local neg = helpers.parse_literal("x!=3")
-- -- Returns: {name = "x", indices = {}, equal = false, value = 3}
function helpers.parse_literal(lit_str)
    if type(lit_str) ~= "string" then
        return nil, "expected string, got " .. type(lit_str)
    end

    -- Try to match inequality first (!=)
    local var_part, value = lit_str:match("^(.-)!=(.-)$")
    local equal = false

    if not var_part then
        -- Try equality (=)
        var_part, value = lit_str:match("^(.-)=(.-)$")
        equal = true
    end

    if not var_part or not value then
        return nil, "invalid literal format: " .. lit_str
    end

    -- Trim whitespace
    var_part = var_part:match("^%s*(.-)%s*$")
    value = value:match("^%s*(.-)%s*$")

    -- Parse the variable part
    local var, err = helpers.parse_variable(var_part)
    if not var then
        return nil, err
    end

    -- Parse the value
    local num_value = tonumber(value)
    if not num_value then
        return nil, "invalid value (not a number): " .. value
    end

    return {
        name = var.name,
        indices = var.indices,
        equal = equal,
        value = num_value
    }
end

--- Parse multiple literal strings into structured tables.
-- Parses an array of literal strings, collecting both successful parses and errors.
--
-- @tparam table literals Array of literal strings to parse
-- @treturn table Array of successfully parsed literal tables
-- @treturn table Array of error messages for invalid literals (format: "[index] error")
-- @usage
-- local lits, errors = helpers.parse_literals({"grid[1,1]=1", "invalid", "x=2"})
-- -- lits: {{name="grid", indices={1,1}, equal=true, value=1}, {name="x", ...}}
-- -- errors: {"[2] invalid literal format: invalid"}
function helpers.parse_literals(literals)
    if type(literals) ~= "table" then
        return {}, {"expected table, got " .. type(literals)}
    end

    local results = {}
    local errors = {}

    for i, lit_str in ipairs(literals) do
        local parsed, err = helpers.parse_literal(lit_str)
        if parsed then
            table.insert(results, parsed)
        else
            table.insert(errors, string.format("[%d] %s", i, err or "unknown error"))
        end
    end

    return results, errors
end

--- Format a parsed literal back to a string.
-- Converts a literal table back to its string representation.
--
-- @tparam table lit A parsed literal table with `name`, `indices`, `equal`, and `value` fields
-- @treturn string|nil The formatted literal string, or nil on error
-- @treturn string|nil Error message if formatting failed
-- @usage
-- local str = helpers.format_literal({name="grid", indices={1,2}, equal=true, value=5})
-- -- Returns: "grid[1, 2]=5"
--
-- local str = helpers.format_literal({name="x", indices={}, equal=false, value=3})
-- -- Returns: "x!=3"
function helpers.format_literal(lit)
    if type(lit) ~= "table" then
        return nil, "expected table, got " .. type(lit)
    end

    local var_str = lit.name
    if lit.indices and #lit.indices > 0 then
        var_str = var_str .. "[" .. table.concat(lit.indices, ", ") .. "]"
    end

    local op = lit.equal and "=" or "!="
    return var_str .. op .. tostring(lit.value)
end

--- Format a parsed variable back to a string.
-- Converts a variable table back to its string representation.
--
-- @tparam table var A parsed variable table with `name` and `indices` fields
-- @treturn string|nil The formatted variable string, or nil on error
-- @treturn string|nil Error message if formatting failed
-- @usage
-- local str = helpers.format_variable({name="grid", indices={1, 2}})
-- -- Returns: "grid[1, 2]"
--
-- local str = helpers.format_variable({name="x", indices={}})
-- -- Returns: "x"
function helpers.format_variable(var)
    if type(var) ~= "table" then
        return nil, "expected table, got " .. type(var)
    end

    local var_str = var.name
    if var.indices and #var.indices > 0 then
        var_str = var_str .. "[" .. table.concat(var.indices, ", ") .. "]"
    end

    return var_str
end

--- Check if a parsed literal represents an assignment (equality).
-- Returns true if the literal uses "=" (variable equals value).
--
-- @tparam table lit A parsed literal table
-- @treturn boolean True if the literal is an assignment (equal=true)
-- @usage
-- local lit = helpers.parse_literal("grid[1,1]=5")
-- print(helpers.is_assignment(lit))  -- true
--
-- local neg = helpers.parse_literal("grid[1,1]!=5")
-- print(helpers.is_assignment(neg))  -- false
function helpers.is_assignment(lit)
    return lit and lit.equal == true
end

--- Check if a parsed literal represents a negation (inequality).
-- Returns true if the literal uses "!=" (variable not equals value).
--
-- @tparam table lit A parsed literal table
-- @treturn boolean True if the literal is a negation (equal=false)
-- @usage
-- local neg = helpers.parse_literal("grid[1,1]!=5")
-- print(helpers.is_negation(neg))  -- true
--
-- local lit = helpers.parse_literal("grid[1,1]=5")
-- print(helpers.is_negation(lit))  -- false
function helpers.is_negation(lit)
    return lit and lit.equal == false
end

--- Get the variable name from a literal string without full parsing.
-- A quick helper for simple filtering when you only need the variable name.
-- This is faster than parse_literal() when you don't need the full structure.
--
-- @tparam string lit_str The literal string
-- @treturn string|nil The variable name, or nil if parsing fails
-- @usage
-- print(helpers.get_variable_name("grid[1, 2]=5"))  -- "grid"
-- print(helpers.get_variable_name("foo_bar!=3"))   -- "foo_bar"
-- print(helpers.get_variable_name(nil))            -- nil
function helpers.get_variable_name(lit_str)
    if type(lit_str) ~= "string" then
        return nil
    end
    return lit_str:match("^([%w_]+)")
end

return helpers
