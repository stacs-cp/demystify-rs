-- Tests for helpers.lua
-- Run with: luajit test_helpers.lua

local helpers = require("helpers")

local test_count = 0
local pass_count = 0

local function test(name, fn)
    test_count = test_count + 1
    local ok, err = pcall(fn)
    if ok then
        pass_count = pass_count + 1
        print("PASS: " .. name)
    else
        print("FAIL: " .. name)
        print("      " .. tostring(err))
    end
end

local function assert_eq(actual, expected, msg)
    if actual ~= expected then
        error(string.format("%s: expected %q, got %q",
            msg or "assertion failed",
            tostring(expected),
            tostring(actual)))
    end
end

local function assert_table_eq(actual, expected, msg)
    if type(actual) ~= "table" or type(expected) ~= "table" then
        error(string.format("%s: expected tables, got %s and %s",
            msg or "assertion failed",
            type(expected),
            type(actual)))
    end

    for k, v in pairs(expected) do
        if type(v) == "table" then
            assert_table_eq(actual[k], v, msg .. "." .. tostring(k))
        else
            assert_eq(actual[k], v, msg .. "." .. tostring(k))
        end
    end

    for k in pairs(actual) do
        if expected[k] == nil then
            error(string.format("%s: unexpected key %q in actual",
                msg or "assertion failed", tostring(k)))
        end
    end
end

print("=== Testing helpers.lua ===")
print()

-- ===== parse_variable tests =====
print("--- parse_variable ---")

test("parse_variable with single index", function()
    local result = helpers.parse_variable("grid[1]")
    assert_table_eq(result, {name = "grid", indices = {1}}, "result")
end)

test("parse_variable with multiple indices", function()
    local result = helpers.parse_variable("grid[1, 2]")
    assert_table_eq(result, {name = "grid", indices = {1, 2}}, "result")
end)

test("parse_variable with three indices", function()
    local result = helpers.parse_variable("cube[1, 2, 3]")
    assert_table_eq(result, {name = "cube", indices = {1, 2, 3}}, "result")
end)

test("parse_variable without indices", function()
    local result = helpers.parse_variable("x")
    assert_table_eq(result, {name = "x", indices = {}}, "result")
end)

test("parse_variable with negative index", function()
    local result = helpers.parse_variable("arr[-1]")
    assert_table_eq(result, {name = "arr", indices = {-1}}, "result")
end)

test("parse_variable with underscore in name", function()
    local result = helpers.parse_variable("my_var[5]")
    assert_table_eq(result, {name = "my_var", indices = {5}}, "result")
end)

test("parse_variable with invalid input returns nil", function()
    local result, err = helpers.parse_variable(123)
    assert_eq(result, nil, "result")
    assert(err ~= nil, "error message expected")
end)

test("parse_variable with invalid format returns nil", function()
    local result, err = helpers.parse_variable("grid[")
    assert_eq(result, nil, "result")
    assert(err ~= nil, "error message expected")
end)

print()

-- ===== parse_literal tests =====
print("--- parse_literal ---")

test("parse_literal equality with single index", function()
    local result = helpers.parse_literal("grid[1]=3")
    assert_table_eq(result, {name = "grid", indices = {1}, equal = true, value = 3}, "result")
end)

test("parse_literal equality with multiple indices", function()
    local result = helpers.parse_literal("grid[1, 2]=5")
    assert_table_eq(result, {name = "grid", indices = {1, 2}, equal = true, value = 5}, "result")
end)

test("parse_literal inequality", function()
    local result = helpers.parse_literal("grid[1, 2]!=4")
    assert_table_eq(result, {name = "grid", indices = {1, 2}, equal = false, value = 4}, "result")
end)

test("parse_literal with spaces around operator", function()
    local result = helpers.parse_literal("grid[1, 2] = 5")
    assert_table_eq(result, {name = "grid", indices = {1, 2}, equal = true, value = 5}, "result")
end)

test("parse_literal inequality with spaces", function()
    local result = helpers.parse_literal("grid[1] != 2")
    assert_table_eq(result, {name = "grid", indices = {1}, equal = false, value = 2}, "result")
end)

test("parse_literal with negative value", function()
    local result = helpers.parse_literal("temp[0]=-10")
    assert_table_eq(result, {name = "temp", indices = {0}, equal = true, value = -10}, "result")
end)

test("parse_literal without indices", function()
    local result = helpers.parse_literal("x=42")
    assert_table_eq(result, {name = "x", indices = {}, equal = true, value = 42}, "result")
end)

test("parse_literal with invalid input returns nil", function()
    local result, err = helpers.parse_literal(nil)
    assert_eq(result, nil, "result")
    assert(err ~= nil, "error message expected")
end)

test("parse_literal with invalid format returns nil", function()
    local result, err = helpers.parse_literal("no_operator_here")
    assert_eq(result, nil, "result")
    assert(err ~= nil, "error message expected")
end)

test("parse_literal with non-numeric value returns nil", function()
    local result, err = helpers.parse_literal("grid[1]=abc")
    assert_eq(result, nil, "result")
    assert(err ~= nil, "error message expected")
end)

print()

-- ===== parse_literals tests =====
print("--- parse_literals ---")

test("parse_literals with valid array", function()
    local results, errors = helpers.parse_literals({"grid[1]=1", "grid[2]!=2"})
    assert_eq(#results, 2, "result count")
    assert_eq(#errors, 0, "error count")
    assert_table_eq(results[1], {name = "grid", indices = {1}, equal = true, value = 1}, "first")
    assert_table_eq(results[2], {name = "grid", indices = {2}, equal = false, value = 2}, "second")
end)

test("parse_literals with mixed valid and invalid", function()
    local results, errors = helpers.parse_literals({"grid[1]=1", "invalid", "grid[2]=2"})
    assert_eq(#results, 2, "result count")
    assert_eq(#errors, 1, "error count")
end)

test("parse_literals with empty array", function()
    local results, errors = helpers.parse_literals({})
    assert_eq(#results, 0, "result count")
    assert_eq(#errors, 0, "error count")
end)

test("parse_literals with non-table returns error", function()
    local results, errors = helpers.parse_literals("not a table")
    assert_eq(#results, 0, "result count")
    assert_eq(#errors, 1, "error count")
end)

print()

-- ===== format_literal tests =====
print("--- format_literal ---")

test("format_literal equality", function()
    local result = helpers.format_literal({name = "grid", indices = {1, 2}, equal = true, value = 5})
    assert_eq(result, "grid[1, 2]=5", "result")
end)

test("format_literal inequality", function()
    local result = helpers.format_literal({name = "grid", indices = {1}, equal = false, value = 3})
    assert_eq(result, "grid[1]!=3", "result")
end)

test("format_literal without indices", function()
    local result = helpers.format_literal({name = "x", indices = {}, equal = true, value = 42})
    assert_eq(result, "x=42", "result")
end)

print()

-- ===== format_variable tests =====
print("--- format_variable ---")

test("format_variable with indices", function()
    local result = helpers.format_variable({name = "grid", indices = {1, 2, 3}})
    assert_eq(result, "grid[1, 2, 3]", "result")
end)

test("format_variable without indices", function()
    local result = helpers.format_variable({name = "x", indices = {}})
    assert_eq(result, "x", "result")
end)

print()

-- ===== is_assignment / is_negation tests =====
print("--- is_assignment / is_negation ---")

test("is_assignment returns true for equality", function()
    local lit = {name = "x", indices = {}, equal = true, value = 1}
    assert_eq(helpers.is_assignment(lit), true, "result")
end)

test("is_assignment returns false for inequality", function()
    local lit = {name = "x", indices = {}, equal = false, value = 1}
    assert_eq(helpers.is_assignment(lit), false, "result")
end)

test("is_negation returns true for inequality", function()
    local lit = {name = "x", indices = {}, equal = false, value = 1}
    assert_eq(helpers.is_negation(lit), true, "result")
end)

test("is_negation returns false for equality", function()
    local lit = {name = "x", indices = {}, equal = true, value = 1}
    assert_eq(helpers.is_negation(lit), false, "result")
end)

print()

-- ===== get_variable_name tests =====
print("--- get_variable_name ---")

test("get_variable_name from literal", function()
    assert_eq(helpers.get_variable_name("grid[1, 2]=5"), "grid", "result")
end)

test("get_variable_name with underscore", function()
    assert_eq(helpers.get_variable_name("my_var[1]!=2"), "my_var", "result")
end)

test("get_variable_name without indices", function()
    assert_eq(helpers.get_variable_name("x=42"), "x", "result")
end)

test("get_variable_name with nil returns nil", function()
    assert_eq(helpers.get_variable_name(nil), nil, "result")
end)

print()

-- ===== roundtrip tests =====
print("--- roundtrip tests ---")

test("parse then format equality preserves structure", function()
    local original = "grid[1, 2]=5"
    local parsed = helpers.parse_literal(original)
    local formatted = helpers.format_literal(parsed)
    assert_eq(formatted, original, "roundtrip")
end)

test("parse then format inequality preserves structure", function()
    local original = "arr[3]!=7"
    local parsed = helpers.parse_literal(original)
    local formatted = helpers.format_literal(parsed)
    assert_eq(formatted, original, "roundtrip")
end)

print()

-- ===== Summary =====
print(string.format("=== Results: %d/%d tests passed ===", pass_count, test_count))

if pass_count < test_count then
    os.exit(1)
end
