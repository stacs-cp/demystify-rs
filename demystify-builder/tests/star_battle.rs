//! End-to-end test: build a 5×5 1-star star-battle puzzle entirely in
//! Rust using `demystify-builder`, hand it to demystify's solver, and
//! verify it solves to a valid star-battle solution.

use std::sync::Arc;

use demystify::problem::PuzLit;
use demystify::problem::planner::PuzzlePlanner;
use demystify::problem::solver::PuzzleSolver;
use demystify_builder::{PuzzleBuilder, PuzzleParse, ShowRole};

/// 5×5 1-star star-battle.  Cages match `eprime/star-battle/star-battle-1.param`.
fn build_star_battle_1() -> PuzzleParse {
    let grid_size: i64 = 5;
    let starcount: i64 = 1;
    let cages: Vec<Vec<i64>> = vec![
        vec![5, 1, 1, 1, 2],
        vec![5, 5, 3, 3, 2],
        vec![5, 5, 3, 3, 2],
        vec![5, 5, 3, 3, 4],
        vec![5, 5, 4, 4, 4],
    ];

    let mut b = PuzzleBuilder::new();
    b.kind("star-battle");

    let stars = b.var_bool_matrix("stars", &[1..=grid_size, 1..=grid_size]);
    b.show("stars", ShowRole::Main);

    let rowup = b.con_bool_matrix("rowup", &[1..=grid_size]);
    let rowdown = b.con_bool_matrix("rowdown", &[1..=grid_size]);
    let colup = b.con_bool_matrix("colup", &[1..=grid_size]);
    let coldown = b.con_bool_matrix("coldown", &[1..=grid_size]);
    let blockup = b.con_bool_matrix("blockup", &[1..=grid_size]);
    let blockdown = b.con_bool_matrix("blockdown", &[1..=grid_size]);
    // For star battle's adj atoms we need negative indices on the offsets;
    // 1-based ranges don't suit so we use the actual offset range -1..=1.
    let adj = b.con_bool_matrix("adj", &[1..=grid_size, 1..=grid_size, -1..=1, -1..=1]);

    // Row sums.
    for i in 1..=grid_size {
        let row: Vec<_> = (1..=grid_size).map(|j| stars.get(&[i, j]).pos()).collect();
        let gu = b
            .guard(
                rowup.get(&[i]),
                "rowup",
                format!("at least {starcount} star(s) in row ({i})"),
            )
            .unwrap();
        b.sum_ge(gu, &row, starcount).unwrap();
        let gd = b
            .guard(
                rowdown.get(&[i]),
                "rowdown",
                format!("at most {starcount} star(s) in row ({i})"),
            )
            .unwrap();
        b.sum_le(gd, &row, starcount).unwrap();
    }

    // Column sums.
    for j in 1..=grid_size {
        let col: Vec<_> = (1..=grid_size).map(|i| stars.get(&[i, j]).pos()).collect();
        let gu = b
            .guard(
                colup.get(&[j]),
                "colup",
                format!("at least {starcount} star(s) in column ({j})"),
            )
            .unwrap();
        b.sum_ge(gu, &col, starcount).unwrap();
        let gd = b
            .guard(
                coldown.get(&[j]),
                "coldown",
                format!("at most {starcount} star(s) in column ({j})"),
            )
            .unwrap();
        b.sum_le(gd, &col, starcount).unwrap();
    }

    // Block sums.
    for block in 1..=grid_size {
        let mut cells: Vec<_> = Vec::new();
        for i in 1..=grid_size {
            for j in 1..=grid_size {
                if cages[(i - 1) as usize][(j - 1) as usize] == block {
                    cells.push(stars.get(&[i, j]).pos());
                }
            }
        }
        let gu = b
            .guard(
                blockup.get(&[block]),
                "blockup",
                format!("at least {starcount} star(s) in box ({block})"),
            )
            .unwrap();
        b.sum_ge(gu, &cells, starcount).unwrap();
        let gd = b
            .guard(
                blockdown.get(&[block]),
                "blockdown",
                format!("at most {starcount} star(s) in box ({block})"),
            )
            .unwrap();
        b.sum_le(gd, &cells, starcount).unwrap();
    }

    // Adjacency: for each (i, j) and each (k, l) ≠ (0, 0)/(0, 1), if both
    // stars[i,j] and stars[i+k,j+l] would exist on-board, then the
    // adjacency atom forbids both being true.  The eprime model's
    // `((k=0 ∧ l=0) ∨ (k=0 ∧ l=1))` exclusion is just a deduplication
    // trick to avoid emitting `(i,j)–(i,j)` (trivial) and to choose one of
    // each pair only once; we reproduce that here.
    for i in 1..=grid_size {
        for j in 1..=grid_size {
            for k in 0..=1_i64 {
                for l in -1..=1_i64 {
                    if k == 0 && (l == 0 || l == 1) {
                        continue;
                    }
                    let i2 = i + k;
                    let j2 = j + l;
                    if !(1..=grid_size).contains(&i2) || !(1..=grid_size).contains(&j2) {
                        // adj[i,j,k,l] still exists as an atom but the
                        // constraint is vacuous off-board.  Skip cleanly so
                        // we don't post an empty-sum constraint.
                        continue;
                    }
                    let g = b
                        .guard(
                            adj.get(&[i, j, k, l]),
                            "adj",
                            format!("({i}, {j}) and ({i2}, {j2}) are adjacent"),
                        )
                        .unwrap();
                    b.sum_le(
                        g,
                        &[stars.get(&[i, j]).pos(), stars.get(&[i2, j2]).pos()],
                        1,
                    )
                    .unwrap();
                }
            }
        }
    }

    // Suppress unused-variable warnings — adj is referenced via the
    // closures above but the compiler can't see that.  No setup clauses
    // are needed: demystify's solver assumes every $#CON activation lit
    // is true on every SAT call (see PuzzleSolver::is_currently_solvable
    // and friends), so pinning them with unit clauses would only
    // interfere with MUS reasoning.
    let _ = (rowup, rowdown, colup, coldown, blockup, blockdown, adj);

    b.build().expect("build star battle")
}

#[test]
fn builds_and_solves_to_valid_star_battle() {
    let puzzle = Arc::new(build_star_battle_1());

    let mut solver = PuzzleSolver::new(puzzle.clone()).expect("PuzzleSolver::new");

    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    let mut rng = ChaCha20Rng::seed_from_u64(42);
    let solution = solver
        .random_solution(&mut rng, None)
        .expect("puzzle is satisfiable");

    // Extract `stars[i,j] = 1` puzlits from the solution.
    let grid_size = 5usize;
    let mut stars_grid = vec![vec![false; grid_size]; grid_size];
    for lit in &solution {
        // Each SAT lit corresponds to one or more PuzLits via the
        // direct encoding.  We look for `stars[i,j] = 1` lits.
        if let Some(puzlits) = puzzle.direct.invlitmap.get(lit) {
            for pl in puzlits {
                if pl.var().name() == "stars" && pl.sign() && pl.val() == 1 {
                    let var = pl.var();
                    let idx = var.indices();
                    let i = idx[0] as usize - 1;
                    let j = idx[1] as usize - 1;
                    stars_grid[i][j] = true;
                }
            }
        }
    }

    let total: usize = stars_grid
        .iter()
        .map(|row| row.iter().filter(|&&b| b).count())
        .sum();
    assert_eq!(total, grid_size, "expected exactly {grid_size} stars total");

    // Exactly one star per row.
    for (i, row) in stars_grid.iter().enumerate() {
        let count = row.iter().filter(|&&b| b).count();
        assert_eq!(count, 1, "row {i} has {count} stars, expected 1");
    }
    // Exactly one star per column.
    #[allow(clippy::needless_range_loop)]
    for j in 0..grid_size {
        let count = (0..grid_size).filter(|&i| stars_grid[i][j]).count();
        assert_eq!(count, 1, "column {j} has {count} stars, expected 1");
    }
    // Exactly one star per block.
    let cages = [
        [5, 1, 1, 1, 2],
        [5, 5, 3, 3, 2],
        [5, 5, 3, 3, 2],
        [5, 5, 3, 3, 4],
        [5, 5, 4, 4, 4],
    ];
    for block in 1..=5 {
        let count = (0..grid_size)
            .flat_map(|i| (0..grid_size).map(move |j| (i, j)))
            .filter(|&(i, j)| cages[i][j] == block && stars_grid[i][j])
            .count();
        assert_eq!(count, 1, "block {block} has {count} stars, expected 1");
    }
    // No two stars adjacent (including diagonally).
    for i in 0..grid_size {
        for j in 0..grid_size {
            if !stars_grid[i][j] {
                continue;
            }
            for di in -1..=1_i64 {
                for dj in -1..=1_i64 {
                    if di == 0 && dj == 0 {
                        continue;
                    }
                    let i2 = i as i64 + di;
                    let j2 = j as i64 + dj;
                    if (0..grid_size as i64).contains(&i2)
                        && (0..grid_size as i64).contains(&j2)
                        && stars_grid[i2 as usize][j2 as usize]
                    {
                        panic!(
                            "stars at ({}, {}) and ({}, {}) are adjacent",
                            i + 1,
                            j + 1,
                            i2 + 1,
                            j2 + 1
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn demystify_pipeline_deduces_all_stars() {
    // The full demystify solving pipeline should be able to reason
    // step-by-step about this 5×5 1-star puzzle, which has a unique
    // solution.  We assert that quick_solve produces at least one step
    // and that the final set of deduced `stars[i,j] = 1` puzlits has
    // exactly 5 entries.
    let puzzle = Arc::new(build_star_battle_1());
    let solver = PuzzleSolver::new(puzzle.clone()).expect("PuzzleSolver::new");
    let mut planner = PuzzlePlanner::new(solver);

    let steps = planner.quick_solve();
    assert!(!steps.is_empty(), "expected at least one solve step");

    // Collect every deduced puzlit across every step.
    let mut deduced: Vec<&PuzLit> = Vec::new();
    for step in &steps {
        for mus in step {
            for pl in &mus.lits {
                deduced.push(pl);
            }
        }
    }
    let star_positive_count = deduced
        .iter()
        .filter(|pl| pl.var().name() == "stars" && pl.sign() && pl.val() == 1)
        .count();
    assert_eq!(
        star_positive_count, 5,
        "expected demystify to deduce all 5 star positions"
    );
}
