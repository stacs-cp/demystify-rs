-- Test script for demystify Lua bindings
-- Requires LuaJIT and the demystify library to be built

local function test_version()
    local demystify = require("demystify")
    assert(demystify.version, "version should be defined")
    print("Version: " .. demystify.version)
    print("PASS: version")
end

local function test_load_puzzle(puzzle_path)
    local demystify = require("demystify")

    local puzzle = demystify.load_puzzle(puzzle_path)
    assert(puzzle, "puzzle should be loaded")

    -- Test kind
    local kind = puzzle:kind()
    print("Puzzle kind: " .. (kind or "nil"))
    assert(kind == "binairo" or kind == "sudoku" or kind ~= nil, "kind should be valid")
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
        if step.literals then
            print("  Literals: " .. table.concat(step.literals, ", "))
        end
        if step.constraints then
            print("  Constraints: " .. table.concat(step.constraints, ", "))
        end
    end
    print("PASS: quick_solve")

    -- After quick_solve, puzzle should be solved
    solved = planner:is_solved()
    print("Is solved after quick_solve: " .. tostring(solved))
    assert(solved, "puzzle should be solved after quick_solve")
    print("PASS: puzzle solved")

    return planner
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
    print()

    print("--- Test: version ---")
    test_version()
    print()

    print("--- Test: load_puzzle ---")
    local puzzle = test_load_puzzle(puzzle_path)
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
