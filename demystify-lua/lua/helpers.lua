-- Helper functions for demystify Lua bindings
-- Provides utilities for parsing literal strings into structured tables

local helpers = {}

--- Parse a variable string like "grid[1, 2]" into its components.
-- @param var_str string The variable string to parse
-- @return table|nil A table with {name, indices} or nil on failure
-- @return string|nil Error message if parsing failed
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

--- Parse a literal string like "grid[1, 2]=3" or "grid[1, 2]!=3" into its components.
-- @param lit_str string The literal string to parse
-- @return table|nil A table with {name, indices, equal, value} or nil on failure
-- @return string|nil Error message if parsing failed
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
-- @param literals table Array of literal strings
-- @return table Array of parsed literal tables (skips invalid ones)
-- @return table Array of error messages for invalid literals
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
-- @param lit table A parsed literal table with {name, indices, equal, value}
-- @return string The formatted literal string
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
-- @param var table A parsed variable table with {name, indices}
-- @return string The formatted variable string
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
-- @param lit table A parsed literal table
-- @return boolean True if the literal is an assignment (equal=true)
function helpers.is_assignment(lit)
    return lit and lit.equal == true
end

--- Check if a parsed literal represents a negation (inequality).
-- @param lit table A parsed literal table
-- @return boolean True if the literal is a negation (equal=false)
function helpers.is_negation(lit)
    return lit and lit.equal == false
end

--- Get the variable name from a literal string without full parsing.
-- This is a quick helper for simple filtering.
-- @param lit_str string The literal string
-- @return string|nil The variable name or nil if parsing fails
function helpers.get_variable_name(lit_str)
    if type(lit_str) ~= "string" then
        return nil
    end
    return lit_str:match("^([%w_]+)")
end

return helpers
