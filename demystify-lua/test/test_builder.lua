-- Test script for the demystify-lua Builder bindings.
-- Builds a tiny 2x2 boolean puzzle whose only constraint forces all four
-- cells true, then checks that the Planner deduces every cell.

local demystify = require("demystify")

local function test_builder_class_present()
    assert(demystify.Builder ~= nil, "Builder class should be present")
    assert(type(demystify.Builder.new) == "function", "Builder.new should be a function")
    print("PASS: builder class present")
end

local function test_build_and_solve_tiny_puzzle()
    local b = demystify.Builder.new()
    b:kind("toy-builder-test")

    local g = b:var_bool_matrix("g", {{1, 2}, {1, 2}})
    assert(g:name() == "g", "matrix name should round-trip")

    -- Sanity-check the matrix API: every (1..=2, 1..=2) cell yields a fresh atom.
    local atoms = g:atoms()
    assert(#atoms == 4, "2x2 matrix should have 4 atoms, got " .. tostring(#atoms))

    local rule = b:con_bool("rule")

    -- rule -> (g[1,1] + g[1,2] + g[2,1] + g[2,2] >= 4) forces every cell true.
    b:sum_ge(
        rule,
        {
            g:get({1, 1}):pos(),
            g:get({1, 2}):pos(),
            g:get({2, 1}):pos(),
            g:get({2, 2}):pos(),
        },
        4,
        "rule",
        "all four cells must be true"
    )

    local puzzle = b:build()
    assert(puzzle ~= nil, "build() should yield a puzzle")
    assert(puzzle:kind() == "toy-builder-test", "kind should round-trip")

    local vars = puzzle:variables()
    -- variables() returns variable *names* (the matrix name, not each cell).
    assert(#vars == 1, "puzzle should have one variable name; got " .. tostring(#vars))
    assert(vars[1] == "g", "variable name should be 'g'")

    -- puzzle:constraints() returns family names (the $#CON family ids),
    -- not the human-readable descriptions; the description is what we look
    -- up via constraint_variables.
    local cons = puzzle:constraints()
    assert(#cons == 1, "puzzle should have exactly one $#CON family")
    assert(cons[1] == "rule", "family name should round-trip as 'rule'; got " .. tostring(cons[1]))

    -- The constraint must reference our variable.
    local con_vars = puzzle:constraint_variables("all four cells must be true")
    assert(con_vars ~= nil, "constraint_variables should return a table")
    assert(#con_vars == 4, "constraint should reference all 4 cells; got " .. tostring(#con_vars))

    -- Plan the puzzle. With one constraint and a sum_ge >= 4, every cell is
    -- forced to true, so the planner should deduce all four "g[i,j]=1" lits.
    local planner = demystify.Planner.new(puzzle)
    local steps = planner:quick_solve()
    assert(type(steps) == "table", "quick_solve should return a table")
    assert(planner:is_solved(), "tiny puzzle must be fully solved by quick_solve")

    local state = planner:current_state()
    for i = 1, 2 do
        for j = 1, 2 do
            assert(
                state.g and state.g[i] and state.g[i][j] == 1,
                string.format("g[%d,%d] should be deduced to 1; got %s", i, j, tostring(state.g and state.g[i] and state.g[i][j]))
            )
        end
    end

    print("PASS: build_and_solve_tiny_puzzle")
end

local function test_signed_negation()
    -- Same puzzle, but encoded with negated literals: sum(¬g) <= 0
    -- (i.e. zero cells false ⇒ all cells true).
    local b = demystify.Builder.new()
    local g = b:var_bool_matrix("g", {{1, 2}, {1, 2}})
    local rule = b:con_bool("rule")

    b:sum_le(
        rule,
        {
            g:get({1, 1}):neg(),
            g:get({1, 2}):neg(),
            g:get({2, 1}):neg(),
            g:get({2, 2}):neg(),
        },
        0,
        "rule",
        "no cells false"
    )

    local puzzle = b:build()
    local planner = demystify.Planner.new(puzzle)
    planner:quick_solve()
    assert(planner:is_solved(), "negation-encoded puzzle should also fully solve")

    local state = planner:current_state()
    for i = 1, 2 do
        for j = 1, 2 do
            assert(state.g[i][j] == 1, "g[" .. i .. "," .. j .. "] should be 1 via negation encoding")
        end
    end

    print("PASS: signed_negation")
end

local function test_build_consumes_builder()
    local b = demystify.Builder.new()
    local g = b:var_bool_matrix("g", {{1, 1}})
    local rule = b:con_bool("rule")
    b:sum_ge(rule, { g:get({1}):pos() }, 1, "rule", "force g[1] true")
    b:build()

    local ok, err = pcall(function() b:build() end)
    assert(not ok, "second build() should error after the first consumed the builder")
    local err_str = tostring(err or "")
    assert(
        string.find(err_str, "consumed") ~= nil,
        "error should mention consumed; got: " .. err_str
    )
    print("PASS: build_consumes_builder")
end

local function test_invalid_role_errors()
    local b = demystify.Builder.new()
    b:var_bool_matrix("g", {{1, 1}})
    local ok, err = pcall(function() b:show("g", "nonsense") end)
    assert(not ok, "unknown show role should error")
    local err_str = tostring(err or "")
    assert(
        string.find(err_str, "show role") ~= nil,
        "error should mention show role; got: " .. err_str
    )
    print("PASS: invalid_role_errors")
end

test_builder_class_present()
test_build_and_solve_tiny_puzzle()
test_signed_negation()
test_build_consumes_builder()
test_invalid_role_errors()

print("All builder tests passed")
