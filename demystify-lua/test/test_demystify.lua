-- Test script for demystify Lua bindings
-- Requires LuaJIT and the demystify library to be built

local function test_version()
    local demystify = require("demystify")
    assert(demystify.version, "version should be defined")
    print("Version: " .. demystify.version)
    print("PASS: version")
end

local function test_bundled_helpers()
    local demystify = require("demystify")

    -- Test that helpers is bundled
    assert(demystify.helpers ~= nil, "helpers should be bundled")
    assert(type(demystify.helpers) == "table", "helpers should be a table")

    -- Test parse_literal
    local lit = demystify.helpers.parse_literal("grid[1, 2]=5")
    assert(lit ~= nil, "parse_literal should return a result")
    assert(lit.name == "grid", "name should be 'grid'")
    assert(#lit.indices == 2, "should have 2 indices")
    assert(lit.indices[1] == 1, "first index should be 1")
    assert(lit.indices[2] == 2, "second index should be 2")
    assert(lit.value == 5, "value should be 5")
    assert(lit.equal == true, "should be equality")

    -- Test parse_variable
    local var = demystify.helpers.parse_variable("x[3]")
    assert(var ~= nil, "parse_variable should return a result")
    assert(var.name == "x", "name should be 'x'")
    assert(#var.indices == 1, "should have 1 index")
    assert(var.indices[1] == 3, "index should be 3")

    -- Test format_literal
    local formatted = demystify.helpers.format_literal(lit)
    assert(formatted == "grid[1, 2]=5", "formatted should match: " .. tostring(formatted))

    print("PASS: bundled_helpers")
end

local function test_load_puzzle(puzzle_path)
    local demystify = require("demystify")

    local puzzle = demystify.load_puzzle(puzzle_path)
    assert(puzzle, "puzzle should be loaded")

    -- Test kind
    local kind = puzzle:kind()
    print("Puzzle kind: " .. (kind or "nil"))
    assert(kind == nil or type(kind) == "string", "kind should be nil or string")
    print("PASS: kind")

    -- Test variables
    local vars = puzzle:variables()
    assert(type(vars) == "table", "variables should return a table")
    print("Number of variables: " .. #vars)
    if #vars > 0 then
        print("First variable: " .. vars[1])
    end
    print("PASS: variables")

    -- Test constraints
    local cons = puzzle:constraints()
    assert(type(cons) == "table", "constraints should return a table")
    print("Number of constraints: " .. #cons)
    if #cons > 0 then
        print("First constraint: " .. cons[1])
    end
    print("PASS: constraints")

    -- Test clause count
    local num_clauses = puzzle:num_clauses()
    print("Number of CNF clauses: " .. num_clauses)
    print("PASS: num_clauses")

    return puzzle
end

local function test_variable_domain(puzzle)
    -- Test variable_domain method
    local vars = puzzle:variables()
    if #vars > 0 then
        -- Get the first variable that has domains (try a few)
        local domain = nil
        local tested_var = nil
        for _, var in ipairs(vars) do
            domain = puzzle:variable_domain(var)
            if domain then
                tested_var = var
                break
            end
        end

        if domain then
            assert(type(domain) == "table", "domain should be a table")
            print("Domain of " .. tested_var .. " has " .. #domain .. " values")
            if #domain > 0 then
                print("  First value: " .. domain[1])
            end
        else
            print("No domains found (puzzle may use aux variables only)")
        end
    end

    -- Test all_domains method
    local all_domains = puzzle:all_domains()
    assert(type(all_domains) == "table", "all_domains should return a table")
    local domain_count = 0
    for _ in pairs(all_domains) do
        domain_count = domain_count + 1
    end
    print("Total variables with domains: " .. domain_count)

    print("PASS: variable_domain")
end

local function test_constraint_variables(puzzle)
    -- Test constraint_variables method
    local cons = puzzle:constraints()
    if #cons > 0 then
        local con_vars = puzzle:constraint_variables(cons[1])
        if con_vars then
            assert(type(con_vars) == "table", "constraint_variables should return a table")
            print("Constraint '" .. cons[1] .. "' involves " .. #con_vars .. " variables")
            if #con_vars > 0 then
                print("  First variable: " .. con_vars[1])
            end
        else
            print("No variables found for constraint (may be structural)")
        end
    end

    -- Test non-existent constraint returns nil
    local bad_result = puzzle:constraint_variables("nonexistent_constraint_xyz")
    assert(bad_result == nil, "nonexistent constraint should return nil")

    print("PASS: constraint_variables")
end

local function test_planner(puzzle)
    local demystify = require("demystify")

    -- Create a planner
    local planner = demystify.Planner.new(puzzle)
    assert(planner, "planner should be created")
    print("PASS: planner creation")

    -- Test is_solved (should be false initially)
    local solved = planner:is_solved()
    assert(type(solved) == "boolean", "is_solved should return a boolean")
    print("Is solved: " .. tostring(solved))
    print("PASS: is_solved")

    -- Test num_provable
    local num_provable = planner:num_provable()
    print("Number of provable literals: " .. num_provable)
    print("PASS: num_provable")

    -- Test quick_solve
    local steps = planner:quick_solve()
    assert(type(steps) == "table", "quick_solve should return a table")
    print("Number of solution steps: " .. #steps)

    if #steps > 0 then
        print("First step:")
        local step = steps[1]
        if step[1] and step[1].literals then
            print("  Literals: " .. table.concat(step[1].literals, ", "))
        end
        if step[1] and step[1].constraints then
            print("  Constraints: " .. table.concat(step[1].constraints, ", "))
        end
    end
    print("PASS: quick_solve")

    -- After quick_solve, puzzle should be solved (or already was)
    solved = planner:is_solved()
    print("Is solved after quick_solve: " .. tostring(solved))
    -- Some puzzles might be trivial or have special conditions
    -- Only assert if we had provable literals to begin with
    if num_provable > 0 then
        assert(solved, "puzzle should be solved after quick_solve")
    else
        print("(Puzzle had no provable literals - may be trivial or require givens)")
    end
    print("PASS: puzzle solved")

    return planner
end

local function test_fix_methods(puzzle)
    local demystify = require("demystify")

    -- Create a fresh planner for testing fix methods
    local planner = demystify.Planner.new(puzzle)
    assert(planner, "planner should be created")

    -- Test fix_var method - try to fix a value that exists
    -- We'll get provable literals and try to fix one
    local num_provable = planner:num_provable()
    if num_provable > 0 then
        local provable = planner:provable_literals()
        if #provable > 0 then
            -- Parse the first provable literal to get components
            local lit = demystify.helpers.parse_literal(provable[1])
            if lit and lit.equal then
                -- Try fix_var with positional args
                local result = planner:fix_var(lit.name, lit.indices, lit.value)
                assert(type(result) == "boolean", "fix_var should return boolean")
                print("fix_var result: " .. tostring(result))
            end
        end
    end

    -- Test fix method with table args
    -- Create another fresh planner
    local planner2 = demystify.Planner.new(puzzle)
    num_provable = planner2:num_provable()
    if num_provable > 0 then
        local provable = planner2:provable_literals()
        if #provable > 0 then
            local lit = demystify.helpers.parse_literal(provable[1])
            if lit and lit.equal then
                -- Try fix with table args
                local result = planner2:fix({
                    name = lit.name,
                    indices = lit.indices,
                    value = lit.value
                })
                assert(type(result) == "boolean", "fix should return boolean")
                print("fix (table) result: " .. tostring(result))
            end
        end
    end

    -- Test that fix with bad args returns false
    local planner3 = demystify.Planner.new(puzzle)
    local bad_result = planner3:fix_var("nonexistent_var_xyz", {999, 999}, -999)
    assert(bad_result == false, "fix_var with bad args should return false")
    print("fix_var with bad args correctly returns false")

    print("PASS: fix_methods")
end

-- Main test runner
local function main()
    local puzzle_path = arg[1]
    if not puzzle_path then
        print("Usage: luajit test_demystify.lua <puzzle.json>")
        print("  puzzle.json should be created with: demystify --save-parsed puzzle.json ...")
        os.exit(1)
    end

    print("=== Testing demystify Lua bindings ===")
    print("Puzzle file: " .. puzzle_path)
    print()

    print("--- Test: version ---")
    test_version()
    print()

    print("--- Test: bundled_helpers ---")
    test_bundled_helpers()
    print()

    print("--- Test: load_puzzle ---")
    local puzzle = test_load_puzzle(puzzle_path)
    print()

    print("--- Test: variable_domain ---")
    test_variable_domain(puzzle)
    print()

    print("--- Test: constraint_variables ---")
    test_constraint_variables(puzzle)
    print()

    print("--- Test: fix_methods ---")
    test_fix_methods(puzzle)
    print()

    print("--- Test: planner ---")
    test_planner(puzzle)
    print()

    print("=== All tests passed! ===")
end

-- Run tests
local ok, err = pcall(main)
if not ok then
    print("FAIL: " .. tostring(err))
    os.exit(1)
end
