use std::{sync::Arc, time::Duration};

use criterion::{Criterion, criterion_group, criterion_main};
use demystify::{
    problem::{
        planner::{PlannerConfig, PuzzlePlanner},
        solver::PuzzleSolver,
        util::test_utils::build_puzzleparse,
    },
    satcore::{get_solver_calls, reset_solver_calls},
    stats::{print_mus_stats, print_sat_stats, reset_mus_stats, reset_sat_stats},
};

fn bench_solve(c: &mut Criterion, name: &str, eprime: &str, param: &str, max_steps: Option<usize>) {
    // Build PuzzleParse once — calling Conjure is expensive, done outside the loop.
    let puz = Arc::new(build_puzzleparse(eprime, param));

    c.bench_function(name, |b| {
        b.iter_batched(
            || {
                reset_solver_calls();
                reset_mus_stats();
                reset_sat_stats();
                PuzzleSolver::new(Arc::clone(&puz)).expect("solver init failed")
            },
            |solver| {
                let config = PlannerConfig { max_steps, ..PlannerConfig::default() };
                let mut planner = PuzzlePlanner::new_with_config(solver, config);
                let result = planner.quick_solve();
                let calls = get_solver_calls();
                (result, calls)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // Print stats accumulated from the last benchmark iteration.
    eprintln!("\n--- MUS stats for {name} ---");
    print_mus_stats();
    eprintln!("\n--- SAT call stats for {name} ---");
    print_sat_stats();
}

fn binairo(c: &mut Criterion) {
    bench_solve(c, "binairo", "./tst/binairo.eprime", "./tst/binairo-1.param", None);
}

fn minesweeper_wall(c: &mut Criterion) {
    bench_solve(c, "minesweeper_wall", "./tst/minesweeper.eprime", "./tst/minesweeperWall.param", None);
}

fn little_sudoku(c: &mut Criterion) {
    bench_solve(c, "little_sudoku", "./tst/little-sudoku.eprime", "./tst/little-sudoku.param", None);
}

const MIRACLE_EPRIME: &str = "../demystify-web/examples/eprime/miracle.eprime";
const MIRACLE_PARAM: &str = "../demystify-web/examples/eprime/miracle/original.param";

fn bench_miracle(c: &mut Criterion, name: &str, max_steps: Option<usize>, samples: usize, meas: Duration) {
    let puz = Arc::new(build_puzzleparse(MIRACLE_EPRIME, MIRACLE_PARAM));

    let mut group = c.benchmark_group(name);
    group.sample_size(samples);
    group.measurement_time(meas);

    group.bench_function("solve", |b| {
        b.iter_batched(
            || {
                reset_solver_calls();
                reset_mus_stats();
                reset_sat_stats();
                PuzzleSolver::new(Arc::clone(&puz)).expect("solver init failed")
            },
            |solver| {
                let config = PlannerConfig { max_steps, ..PlannerConfig::default() };
                let mut planner = PuzzlePlanner::new_with_config(solver, config);
                let result = planner.quick_solve();
                let calls = get_solver_calls();
                (result, calls)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();

    eprintln!("\n--- MUS stats for {name} ---");
    print_mus_stats();
    eprintln!("\n--- SAT call stats for {name} ---");
    print_sat_stats();
}

fn miracle_sudoku_02(c: &mut Criterion) {
    bench_miracle(c, "miracle_sudoku_02", Some(2), 10, Duration::from_secs(120));
}

criterion_group!(benches, binairo, minesweeper_wall, little_sudoku, miracle_sudoku_02);
criterion_main!(benches);
