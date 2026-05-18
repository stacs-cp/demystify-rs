-- Lua port of demystify-builder/tests/minesweeper_reveal.rs.
--
-- Builds a 3x3 minesweeper via the Builder bindings, wires up grid -> facts
-- via $#REVEAL, and checks that the planner deduces every cell.

local demystify = require("demystify")

local N = 3

-- (row, col, clue) for revealed cells; the same layout as the Rust test:
--   0 0 .
--   0 1 .
--   . . .
local CLUES = {
    {1, 1, 0},
    {1, 2, 0},
    {2, 1, 0},
    {2, 2, 1},
}

local EXPECTED = {
    {{1, 3}, 0},
    {{2, 3}, 0},
    {{3, 1}, 0},
    {{3, 2}, 0},
    {{3, 3}, 1},
}

local function build_minesweeper()
    local b = demystify.Builder.new()
    b:kind("minesweeper-reveal-lua")

    local grid = b:var_bool_matrix("grid", {{1, N}, {1, N}})
    b:show("grid", "main")

    local facts = b:reveal_bool_matrix("facts", {{1, N}, {1, N}, {0, 1}})
    b:reveal("grid", "facts")

    local sumcheck = b:con_bool_matrix("sumcheck", {{1, N}, {1, N}})

    -- Pin revealed cells to "not a mine".
    for _, clue in ipairs(CLUES) do
        local r, c = clue[1], clue[2]
        b:sum_eq_unguarded({ grid:get({r, c}):neg() }, 1)
    end

    -- Per-clue neighbour-count constraint, guarded by sumcheck[r,c].
    for _, clue in ipairs(CLUES) do
        local r, c, n_mines = clue[1], clue[2], clue[3]
        local neighbours = {}
        for dr = -1, 1 do
            for dc = -1, 1 do
                if not (dr == 0 and dc == 0) then
                    local nr, nc = r + dr, c + dc
                    if nr >= 1 and nr <= N and nc >= 1 and nc <= N then
                        table.insert(neighbours, grid:get({nr, nc}):pos())
                    end
                end
            end
        end
        b:sum_eq(
            sumcheck:get({r, c}),
            neighbours,
            n_mines,
            "sumcheck",
            string.format("exactly %d mines around (%d, %d)", n_mines, r, c)
        )
    end

    -- Tie facts to grid in CNF so the SAT instance has a ground truth.
    --   (grid[r,c] == d) → facts[r,c,d]
    -- encoded as `sum_eq_unguarded([grid_signed, facts.neg()]) = 1`.
    for r = 1, N do
        for c = 1, N do
            for d = 0, 1 do
                local grid_signed = (d == 1) and grid:get({r, c}):pos() or grid:get({r, c}):neg()
                local facts_signed = facts:get({r, c, d}):neg()
                b:sum_eq_unguarded({ grid_signed, facts_signed }, 1)
            end
        end
    end

    return b:build()
end

local function test_set_reveal_rejects_unknown_source()
    local b = demystify.Builder.new()
    b:reveal_bool_matrix("facts", {{1, 1}, {0, 1}})
    local ok, err = pcall(function() b:reveal("nope", "facts") end)
    assert(not ok, "set_reveal with unknown source should error")
    local err_str = tostring(err or "")
    assert(
        string.find(err_str, "nope") ~= nil,
        "error should name the missing source; got: " .. err_str
    )
    print("PASS: set_reveal_rejects_unknown_source")
end

local function test_set_reveal_rejects_unknown_target()
    local b = demystify.Builder.new()
    b:var_bool_matrix("grid", {{1, 2}})
    local ok, err = pcall(function() b:reveal("grid", "facts") end)
    assert(not ok, "set_reveal with unknown target should error")
    local err_str = tostring(err or "")
    assert(
        string.find(err_str, "facts") ~= nil,
        "error should name the missing target; got: " .. err_str
    )
    print("PASS: set_reveal_rejects_unknown_target")
end

local function test_builds_and_solves_minesweeper()
    local puzzle = build_minesweeper()
    local planner = demystify.Planner.new(puzzle)
    planner:quick_solve()
    assert(planner:is_solved(), "minesweeper puzzle should solve fully")

    local state = planner:current_state()
    assert(state.grid ~= nil, "current_state should expose grid")

    for _, expectation in ipairs(EXPECTED) do
        local rc, want = expectation[1], expectation[2]
        local r, c = rc[1], rc[2]
        local got = state.grid[r][c]
        assert(
            got == want,
            string.format("grid[%d,%d] = %s, expected %d", r, c, tostring(got), want)
        )
    end

    print("PASS: builds_and_solves_minesweeper")
end

test_set_reveal_rejects_unknown_source()
test_set_reveal_rejects_unknown_target()
test_builds_and_solves_minesweeper()

print("All builder minesweeper-via-REVEAL tests passed")
