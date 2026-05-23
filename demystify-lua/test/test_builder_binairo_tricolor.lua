-- Lua port of demystify-builder/tests/binairo_tricolor.rs.
--
-- Three-colour binairo (Takuzu) built with multi-valued `var_int_matrix`
-- cells and the `table` constraint: colour balance is `sum_eq`, while
-- "no three in a row" and the `mod`/sequence rule are `table`s with a
-- small forbidden set (one $#CON per rule instance).
--
-- 3x3 instance with grid[1,1] = 0: at n=3 each row/column holds one of
-- each colour, so the mod rule forces every consecutive pair to advance —
-- the grid is the addition table grid[i,j] = (i-1)+(j-1) mod 3.

local demystify = require("demystify")

local COLOURS = {0, 1, 2}
local MOD_FORBIDDEN = {{0, 2}, {1, 0}, {2, 1}}

local function three_same_forbidden()
    local out = {}
    for _, k in ipairs(COLOURS) do
        table.insert(out, {k, k, k})
    end
    return out
end

local function build_binairo_tricolor(h, w, givens)
    local third_w = math.floor(w / 3)
    local third_h = math.floor(h / 3)

    local b = demystify.Builder.new()
    b:kind("binairo-tricolor")

    local grid = b:var_int_matrix("grid", {{1, h}, {1, w}}, COLOURS)
    b:show("grid", "main")

    -- Row colour balance.
    local rowcolor = {}
    for _, k in ipairs(COLOURS) do
        rowcolor[k] = b:con_bool_matrix("rowcolor" .. k, {{1, h}})
    end
    for _, k in ipairs(COLOURS) do
        for i = 1, h do
            local lits = {}
            for j = 1, w do
                table.insert(lits, grid:cell({i, j}):eq(k))
            end
            local g = b:guard(rowcolor[k]:get({i}), "rowcolor" .. k,
                string.format("row %d has exactly %d cells of colour %d", i, third_w, k))
            b:sum_eq(g, lits, third_w)
        end
    end

    -- Column colour balance.
    local colcolor = {}
    for _, k in ipairs(COLOURS) do
        colcolor[k] = b:con_bool_matrix("colcolor" .. k, {{1, w}})
    end
    for _, k in ipairs(COLOURS) do
        for j = 1, w do
            local lits = {}
            for i = 1, h do
                table.insert(lits, grid:cell({i, j}):eq(k))
            end
            local g = b:guard(colcolor[k]:get({j}), "colcolor" .. k,
                string.format("col %d has exactly %d cells of colour %d", j, third_h, k))
            b:sum_eq(g, lits, third_h)
        end
    end

    -- No three consecutive identical colours: rows then columns.
    local rowmatch = b:con_bool_matrix("rowmatch", {{1, h}, {1, w - 2}})
    for i = 1, h do
        for j = 1, w - 2 do
            local cells = { grid:cell({i, j}), grid:cell({i, j + 1}), grid:cell({i, j + 2}) }
            local g = b:guard(rowmatch:get({i, j}), "rowmatch",
                string.format("row %d has no three identical colours from column %d", i, j))
            b:table(g, cells, three_same_forbidden())
        end
    end
    local colmatch = b:con_bool_matrix("colmatch", {{1, w}, {1, h - 2}})
    for j = 1, w do
        for i = 1, h - 2 do
            local cells = { grid:cell({i, j}), grid:cell({i + 1, j}), grid:cell({i + 2, j}) }
            local g = b:guard(colmatch:get({j, i}), "colmatch",
                string.format("col %d has no three identical colours from row %d", j, i))
            b:table(g, cells, three_same_forbidden())
        end
    end

    -- mod / sequence rule: rows then columns.
    local rowseq = b:con_bool_matrix("rowseq", {{1, h}, {1, w - 1}})
    for i = 1, h do
        for j = 1, w - 1 do
            local cells = { grid:cell({i, j}), grid:cell({i, j + 1}) }
            local g = b:guard(rowseq:get({i, j}), "rowseq",
                string.format("row %d colour at column %d stays or advances by one", i, j))
            b:table(g, cells, MOD_FORBIDDEN)
        end
    end
    local colseq = b:con_bool_matrix("colseq", {{1, w}, {1, h - 1}})
    for j = 1, w do
        for i = 1, h - 1 do
            local cells = { grid:cell({i, j}), grid:cell({i + 1, j}) }
            local g = b:guard(colseq:get({j, i}), "colseq",
                string.format("col %d colour at row %d stays or advances by one", j, i))
            b:table(g, cells, MOD_FORBIDDEN)
        end
    end

    -- Givens.
    for _, gv in ipairs(givens) do
        b:sum_eq_unguarded({ grid:cell({gv[1], gv[2]}):eq(gv[3]) }, 1)
    end

    return b:build()
end

local function test_builds_with_expected_constraints()
    local puzzle = build_binairo_tricolor(3, 3, {{1, 1, 0}})
    assert(puzzle:kind() == "binairo-tricolor", "kind should round-trip")

    -- 10 $#CON families: rowcolor0/1/2, colcolor0/1/2, rowmatch, colmatch,
    -- rowseq, colseq.
    local cons = puzzle:constraints()
    assert(#cons == 10, "expected 10 $#CON families; got " .. tostring(#cons))

    -- One constraint entry per family instance: row/col balance 3*3 each
    -- (18), rowmatch/colmatch 3*1 each (6), rowseq/colseq 3*2 each (12).
    assert(puzzle:num_con_lits() == 36,
        "expected 36 constraint entries; got " .. tostring(puzzle:num_con_lits()))

    print("PASS: builds_with_expected_constraints")
end

local function test_solves_3x3_addition_table()
    local puzzle = build_binairo_tricolor(3, 3, {{1, 1, 0}})
    local planner = demystify.Planner.new(puzzle)
    planner:quick_solve()
    assert(planner:is_solved(), "binairo-tricolor 3x3 should solve fully")

    local state = planner:current_state()
    assert(state.grid ~= nil, "current_state should expose grid")

    for i = 1, 3 do
        for j = 1, 3 do
            local want = ((i - 1) + (j - 1)) % 3
            local got = state.grid[i][j]
            assert(got == want,
                string.format("grid[%d,%d] = %s, expected %d", i, j, tostring(got), want))
        end
    end

    print("PASS: solves_3x3_addition_table")
end

-- Note: `table`'s error paths (bad arity / out-of-domain value) are covered
-- by the Rust integration tests in demystify-builder/tests/binairo_tricolor.rs.
-- We don't pcall them here: error unwinding across the mlua/LuaJIT boundary
-- aborts on macOS (the reason demystify-lua is CI-excluded there).

test_builds_with_expected_constraints()
test_solves_3x3_addition_table()

print("All builder binairo-tricolor tests passed")
