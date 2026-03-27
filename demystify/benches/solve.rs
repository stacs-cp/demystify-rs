use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use demystify::{
    problem::{
        planner::PuzzlePlanner,
        solver::PuzzleSolver,
        util::test_utils::build_puzzleparse,
    },
    satcore::{get_solver_calls, reset_solver_calls},
    stats::{print_mus_stats, reset_mus_stats},
};

fn bench_solve(c: &mut Criterion, name: &str, eprime: &str, param: &str) {
    // Fix rayon to 1 thread for reproducibility. build_global is idempotent.
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build_global()
        .ok();

    // Build PuzzleParse once — calling Conjure is expensive, done outside the loop.
    let puz = Arc::new(build_puzzleparse(eprime, param));

    c.bench_function(name, |b| {
        b.iter_batched(
            || {
                reset_solver_calls();
                reset_mus_stats();
                PuzzleSolver::new(Arc::clone(&puz)).expect("solver init failed")
            },
            |solver| {
                let mut planner = PuzzlePlanner::new(solver);
                let result = planner.quick_solve();
                let calls = get_solver_calls();
                (result, calls)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // Print MUS stats accumulated across all iterations of this benchmark.
    eprintln!("\n--- MUS stats for {name} ---");
    print_mus_stats();
}

fn binairo(c: &mut Criterion) {
    bench_solve(
        c,
        "binairo",
        "./tst/binairo.eprime",
        "./tst/binairo-1.param",
    );
}

fn minesweeper_wall(c: &mut Criterion) {
    bench_solve(
        c,
        "minesweeper_wall",
        "./tst/minesweeper.eprime",
        "./tst/minesweeperWall.param",
    );
}

fn little_sudoku(c: &mut Criterion) {
    bench_solve(
        c,
        "little_sudoku",
        "./tst/little-sudoku.eprime",
        "./tst/little-sudoku.param",
    );
}

fn miracle_sudoku(c: &mut Criterion) {
    bench_solve(
        c,
        "miracle_sudoku",
        "../demystify-web/examples/eprime/miracle.eprime",
        "../demystify-web/examples/eprime/miracle/original.param",
    );
}

criterion_group!(benches, binairo, minesweeper_wall, little_sudoku, miracle_sudoku);
criterion_main!(benches);
