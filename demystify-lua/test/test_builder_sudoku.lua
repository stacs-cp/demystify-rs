-- Lua port of demystify-builder/tests/sudoku_4x4.rs.
--
-- Builds a 4x4 sudoku via the demystify Builder bindings, runs the planner,
-- and checks the deduced grid matches the unique reference solution.

local demystify = require("demystify")

local N = 4

local SOLUTION = {
    {1, 2, 3, 4},
    {3, 4, 1, 2},
    {2, 1, 4, 3},
    {4, 3, 2, 1},
}

local GIVENS = {
    {1, 1, 1},
    {1, 4, 4},
    {2, 1, 3},
    {2, 4, 2},
    {3, 2, 1},
    {4, 3, 2},
}

-- Exclusions chosen to be consistent with the unique solution above; they
-- exercise the negated-literal path through sum_eq_unguarded.
local EXCLUSIONS = {
    {1, 2, 1},
    {1, 2, 4},
    {4, 4, 3},
}

local function cells_in_box(b)
    local br = math.floor((b - 1) / 2) * 2 + 1
    local bc = ((b - 1) % 2) * 2 + 1
    local out = {}
    for dr = 0, 1 do
        for dc = 0, 1 do
            table.insert(out, {br + dr, bc + dc})
        end
    end
    return out
end

local function build_sudoku_4x4()
    local b = demystify.Builder.new()
    b:kind("sudoku-4x4")

    local cell = b:var_bool_matrix("cell", {{1, N}, {1, N}, {1, N}})

    -- 1. Each cell holds exactly one value (structural, unguarded).
    for r = 1, N do
        for c = 1, N do
            local lits = {}
            for v = 1, N do
                table.insert(lits, cell:get({r, c, v}):pos())
            end
            b:sum_eq_unguarded(lits, 1)
        end
    end

    -- 2. Row uniqueness.
    local row_uniq = b:con_bool_matrix("row_uniq", {{1, N}, {1, N}})
    for r = 1, N do
        for v = 1, N do
            local lits = {}
            for c = 1, N do
                table.insert(lits, cell:get({r, c, v}):pos())
            end
            local g = b:guard(row_uniq:get({r, v}), "row_uniq",
                string.format("value %d appears exactly once in row %d", v, r))
            b:sum_eq(g, lits, 1)
        end
    end

    -- 3. Column uniqueness.
    local col_uniq = b:con_bool_matrix("col_uniq", {{1, N}, {1, N}})
    for c = 1, N do
        for v = 1, N do
            local lits = {}
            for r = 1, N do
                table.insert(lits, cell:get({r, c, v}):pos())
            end
            local g = b:guard(col_uniq:get({c, v}), "col_uniq",
                string.format("value %d appears exactly once in column %d", v, c))
            b:sum_eq(g, lits, 1)
        end
    end

    -- 4. Box uniqueness.
    local box_uniq = b:con_bool_matrix("box_uniq", {{1, N}, {1, N}})
    for bi = 1, N do
        for v = 1, N do
            local lits = {}
            for _, rc in ipairs(cells_in_box(bi)) do
                table.insert(lits, cell:get({rc[1], rc[2], v}):pos())
            end
            local g = b:guard(box_uniq:get({bi, v}), "box_uniq",
                string.format("value %d appears exactly once in box %d", v, bi))
            b:sum_eq(g, lits, 1)
        end
    end

    -- 5. Givens — single positive literal forced via sum_eq_unguarded.
    for _, g in ipairs(GIVENS) do
        b:sum_eq_unguarded({ cell:get({g[1], g[2], g[3]}):pos() }, 1)
    end

    -- 6. Exclusions — single negated literal via sum_eq_unguarded.
    for _, e in ipairs(EXCLUSIONS) do
        b:sum_eq_unguarded({ cell:get({e[1], e[2], e[3]}):neg() }, 1)
    end

    return b:build()
end

local function decode_grid(state)
    local grid = {}
    for r = 1, N do
        grid[r] = {}
        for c = 1, N do
            for v = 1, N do
                if state.cell and state.cell[r] and state.cell[r][c]
                    and state.cell[r][c][v] == 1
                then
                    grid[r][c] = v
                end
            end
        end
    end
    return grid
end

local function test_builds_uniquely_solvable_puzzle()
    local puzzle = build_sudoku_4x4()
    assert(puzzle:kind() == "sudoku-4x4", "kind should round-trip")

    -- Three $#CON families (row_uniq / col_uniq / box_uniq).
    local cons = puzzle:constraints()
    assert(#cons == 3,
        "expected 3 $#CON families (row/col/box uniq); got " .. tostring(#cons))

    -- Each family registers N*N = 16 descriptions, so 48 total constraints
    -- in the constraint store (the unguarded structural / givens / exclusion
    -- constraints don't appear in the store).
    assert(puzzle:num_con_lits() == 3 * N * N,
        "expected " .. (3 * N * N) .. " constraint entries; got " .. tostring(puzzle:num_con_lits()))

    print("PASS: builds_uniquely_solvable_puzzle")
end

local function test_planner_deduces_full_solution()
    local puzzle = build_sudoku_4x4()
    local planner = demystify.Planner.new(puzzle)
    planner:quick_solve()

    local state = planner:current_state()
    local grid = decode_grid(state)

    for r = 1, N do
        for c = 1, N do
            assert(grid[r][c] == SOLUTION[r][c],
                string.format("grid[%d,%d] = %s, expected %d",
                    r, c, tostring(grid[r][c]), SOLUTION[r][c]))
        end
    end

    print("PASS: planner_deduces_full_solution")
end

local function test_check_uniqueness_returns_solution()
    local puzzle = build_sudoku_4x4()
    local planner = demystify.Planner.new(puzzle)

    local res = planner:check_uniqueness()
    assert(res.status == "unique",
        "expected unique status, got " .. tostring(res.status))
    assert(res.solution ~= nil, "unique result must carry a solution")

    -- Every cell[r,c,v] boolean is decided, so the solution is one equality
    -- per cell variable: N*N*N entries, of which the N*N "=1" ones spell out
    -- the grid.
    assert(#res.solution == N * N * N,
        "expected " .. (N * N * N) .. " assignments; got " .. tostring(#res.solution))

    local grid = {}
    for r = 1, N do grid[r] = {} end
    local ones = 0
    for _, s in ipairs(res.solution) do
        local r, c, v = string.match(s, "cell%[(%d+),%s*(%d+),%s*(%d+)%]=1$")
        if r ~= nil then
            grid[tonumber(r)][tonumber(c)] = tonumber(v)
            ones = ones + 1
        end
    end
    assert(ones == N * N,
        "expected " .. (N * N) .. " placed cells; got " .. tostring(ones))

    for r = 1, N do
        for c = 1, N do
            assert(grid[r][c] == SOLUTION[r][c],
                string.format("solution[%d,%d] = %s, expected %d",
                    r, c, tostring(grid[r][c]), SOLUTION[r][c]))
        end
    end

    -- The call solves a clone, so the caller's planner is untouched.
    assert(not planner:is_solved(),
        "check_uniqueness should not advance the caller's planner state")

    print("PASS: check_uniqueness_returns_solution")
end

test_builds_uniquely_solvable_puzzle()
test_planner_deduces_full_solution()
test_check_uniqueness_returns_solution()

print("All sudoku-4x4 builder tests passed")
