/// Global MUS statistics collector.
///
/// Tracks per-MUS-search outcomes (time + size or failure) broken down by
/// which internal function was called, per-SAT-call time and conflict counts,
/// and overall phase timing.
///
/// All statistics are stored as pre-aggregated bucket counts so that memory
/// usage is O(1) regardless of how many solves are performed in a process.
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Which MUS-finding function was called.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MusFunction {
    /// Fast size-1 scan run before the main loop (`get_var_mus_size_1`).
    Size1,
    /// Slice-based full MUS search (`get_var_mus_slice`).
    Slice,
    /// Cake-based full MUS search (`get_var_mus_cake`).
    Cake,
    /// Quick MUS search (`get_var_mus_quick`).
    Quick,
}

impl MusFunction {
    fn label(self) -> &'static str {
        match self {
            MusFunction::Size1 => "size_1 (tiny scan)",
            MusFunction::Slice => "slice",
            MusFunction::Cake => "cake",
            MusFunction::Quick => "quick",
        }
    }

    fn index(self) -> usize {
        match self {
            MusFunction::Size1 => 0,
            MusFunction::Slice => 1,
            MusFunction::Cake => 2,
            MusFunction::Quick => 3,
        }
    }
}

/// Outcome of a single MUS search attempt.
#[derive(Debug, Clone)]
pub enum MusOutcome {
    /// A MUS was found with this many constraints.
    Found(usize),
    /// No MUS was found (the literal is not deducible from this constraint set).
    NotFound,
    /// The SAT solver hit its conflict limit and gave up.
    Timeout,
}

/// A single recorded MUS search (kept as a public type for callers that build records externally).
#[derive(Debug, Clone)]
pub struct MusRecord {
    pub duration: Duration,
    pub outcome: MusOutcome,
    pub function: MusFunction,
}

/// Aggregate timing for a named solve phase.
#[derive(Debug, Clone, Default)]
pub struct PhaseStats {
    pub calls: u64,
    pub total_time: Duration,
}

impl PhaseStats {
    fn record(&mut self, d: Duration) {
        self.calls += 1;
        self.total_time += d;
    }
}

/// Time buckets for the distribution table (upper bound in milliseconds, label).
const BUCKETS: &[(u128, &str)] = &[
    (1, "< 1ms"),
    (5, "1–5ms"),
    (20, "5–20ms"),
    (100, "20–100ms"),
    (500, "100–500ms"),
    (2_000, "500ms–2s"),
    (u128::MAX, "> 2s"),
];

const NUM_TIME_BUCKETS: usize = 7;

fn bucket_index(d: Duration) -> usize {
    let ms = d.as_millis();
    BUCKETS
        .iter()
        .position(|&(limit, _)| ms < limit)
        .unwrap_or(BUCKETS.len() - 1)
}

/// Conflict-count buckets for per-SAT-call distribution table.
const CONFLICT_BUCKETS: &[(usize, &str)] = &[
    (1, "0 conflicts"),
    (10, "1–9"),
    (100, "10–99"),
    (1_000, "100–999"),
    (10_000, "1k–9k"),
    (usize::MAX, "≥ 10k"),
];

const NUM_CONF_BUCKETS: usize = 6;

fn conflict_bucket_index(n: usize) -> usize {
    CONFLICT_BUCKETS
        .iter()
        .position(|&(limit, _)| n < limit)
        .unwrap_or(CONFLICT_BUCKETS.len() - 1)
}

// ── SAT call stats ────────────────────────────────────────────────────────────

/// Outcome of a single SAT solver call (from the solver's perspective).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SatResult {
    Sat,
    Unsat,
    Interrupted,
}

impl From<rustsat::solvers::SolverResult> for SatResult {
    fn from(r: rustsat::solvers::SolverResult) -> Self {
        match r {
            rustsat::solvers::SolverResult::Sat => SatResult::Sat,
            rustsat::solvers::SolverResult::Unsat => SatResult::Unsat,
            rustsat::solvers::SolverResult::Interrupted => SatResult::Interrupted,
        }
    }
}

/// One recorded SAT solver invocation (kept as a public type for external callers).
#[derive(Debug, Clone)]
pub struct SatCallRecord {
    pub duration: Duration,
    pub conflicts: usize,
    pub result: SatResult,
}

/// Pre-aggregated SAT call statistics. Stores O(1) bucket data rather than
/// O(N) individual records, so memory does not grow with the number of solves.
#[derive(Debug, Clone, Copy)]
struct SatStats {
    total: usize,
    n_sat: usize,
    n_unsat: usize,
    n_interrupted: usize,
    total_time: Duration,
    total_conflicts: usize,
    /// Per time bucket: [sat_count, unsat_count, interrupted_count]
    time_by_result: [[u32; 3]; NUM_TIME_BUCKETS],
    time_bucket_dur: [Duration; NUM_TIME_BUCKETS],
    /// Per conflict bucket: [sat_count, unsat_count, interrupted_count]
    conf_by_result: [[u32; 3]; NUM_CONF_BUCKETS],
    conf_bucket_total: [usize; NUM_CONF_BUCKETS],
}

const SAT_STATS_ZERO: SatStats = SatStats {
    total: 0,
    n_sat: 0,
    n_unsat: 0,
    n_interrupted: 0,
    total_time: Duration::ZERO,
    total_conflicts: 0,
    time_by_result: [[0; 3]; NUM_TIME_BUCKETS],
    time_bucket_dur: [Duration::ZERO; NUM_TIME_BUCKETS],
    conf_by_result: [[0; 3]; NUM_CONF_BUCKETS],
    conf_bucket_total: [0; NUM_CONF_BUCKETS],
};

impl SatStats {
    fn record(&mut self, duration: Duration, conflicts: usize, result: SatResult) {
        self.total += 1;
        self.total_time += duration;
        self.total_conflicts += conflicts;
        let ri = match result {
            SatResult::Sat => {
                self.n_sat += 1;
                0
            }
            SatResult::Unsat => {
                self.n_unsat += 1;
                1
            }
            SatResult::Interrupted => {
                self.n_interrupted += 1;
                2
            }
        };
        let tb = bucket_index(duration);
        self.time_by_result[tb][ri] += 1;
        self.time_bucket_dur[tb] += duration;
        let cb = conflict_bucket_index(conflicts);
        self.conf_by_result[cb][ri] += 1;
        self.conf_bucket_total[cb] += conflicts;
    }

    fn print_summary(&self) {
        eprintln!("=== SAT Call Statistics ===");
        eprintln!("Total SAT calls: {}", self.total);
        if self.total == 0 {
            eprintln!("===========================");
            return;
        }

        eprintln!(
            "sat {}  unsat {}  interrupted {}  \
             total time {:.3}s  total conflicts {}",
            self.n_sat,
            self.n_unsat,
            self.n_interrupted,
            self.total_time.as_secs_f64(),
            self.total_conflicts,
        );

        // ── Time bucket table ─────────────────────────────────────────────
        eprintln!();
        eprintln!("  Time distribution:");
        eprintln!(
            "  {:>12}  {:>7}  {:>7}  {:>11}  {:>7}  {:>9}",
            "bucket", "sat", "unsat", "interrupted", "total", "time (s)"
        );
        eprintln!("  {}", "-".repeat(62));
        for b in 0..NUM_TIME_BUCKETS {
            let [sat, unsat, int] = self.time_by_result[b];
            let row = sat + unsat + int;
            if row == 0 {
                continue;
            }
            eprintln!(
                "  {:>12}  {:>7}  {:>7}  {:>11}  {:>7}  {:>9.3}",
                BUCKETS[b].1,
                sat,
                unsat,
                int,
                row,
                self.time_bucket_dur[b].as_secs_f64(),
            );
        }

        // ── Conflict bucket table ─────────────────────────────────────────
        eprintln!();
        eprintln!("  Conflict distribution:");
        eprintln!(
            "  {:>14}  {:>7}  {:>7}  {:>11}  {:>7}  {:>12}",
            "bucket", "sat", "unsat", "interrupted", "total", "conflicts"
        );
        eprintln!("  {}", "-".repeat(66));
        for b in 0..NUM_CONF_BUCKETS {
            let [sat, unsat, int] = self.conf_by_result[b];
            let row = sat + unsat + int;
            if row == 0 {
                continue;
            }
            eprintln!(
                "  {:>14}  {:>7}  {:>7}  {:>11}  {:>7}  {:>12}",
                CONFLICT_BUCKETS[b].1,
                sat,
                unsat,
                int,
                row,
                self.conf_bucket_total[b],
            );
        }

        eprintln!();
        eprintln!("===========================");
    }
}

static SAT_STATS: Mutex<SatStats> = Mutex::new(SAT_STATS_ZERO);

/// Record a single SAT solver call into the global stats.
pub fn record_sat_call(
    duration: Duration,
    conflicts: usize,
    result: rustsat::solvers::SolverResult,
) {
    if let Ok(mut s) = SAT_STATS.lock() {
        s.record(duration, conflicts, result.into());
    }
}

/// Reset SAT call stats to zero.
pub fn reset_sat_stats() {
    if let Ok(mut s) = SAT_STATS.lock() {
        *s = SAT_STATS_ZERO;
    }
}

/// Print a summary of per-SAT-call stats to stderr.
pub fn print_sat_stats() {
    if let Ok(s) = SAT_STATS.lock() {
        s.print_summary();
    }
}

// ── MUS search stats ──────────────────────────────────────────────────────────

const NUM_MUS_FUNCTIONS: usize = 4;

/// Pre-aggregated stats for a single MUS-finding function. O(1) memory.
#[derive(Debug, Clone, Copy)]
struct MusFuncStats {
    total: usize,
    n_found: usize,
    n_notfound: usize,
    n_timeout: usize,
    total_time: Duration,
    /// For computing min/max/avg of Found MUS sizes.
    size_min: usize, // usize::MAX when n_found == 0
    size_max: usize,
    size_sum: usize,
    /// Per time bucket: [found_count, notfound_count, timeout_count]
    time_by_outcome: [[u32; 3]; NUM_TIME_BUCKETS],
    time_bucket_dur: [Duration; NUM_TIME_BUCKETS],
}

const MUS_FUNC_ZERO: MusFuncStats = MusFuncStats {
    total: 0,
    n_found: 0,
    n_notfound: 0,
    n_timeout: 0,
    total_time: Duration::ZERO,
    size_min: usize::MAX,
    size_max: 0,
    size_sum: 0,
    time_by_outcome: [[0; 3]; NUM_TIME_BUCKETS],
    time_bucket_dur: [Duration::ZERO; NUM_TIME_BUCKETS],
};

impl MusFuncStats {
    fn record(&mut self, duration: Duration, outcome: &MusOutcome) {
        self.total += 1;
        self.total_time += duration;
        let oi = match outcome {
            MusOutcome::Found(n) => {
                self.n_found += 1;
                if *n < self.size_min {
                    self.size_min = *n;
                }
                if *n > self.size_max {
                    self.size_max = *n;
                }
                self.size_sum += n;
                0
            }
            MusOutcome::NotFound => {
                self.n_notfound += 1;
                1
            }
            MusOutcome::Timeout => {
                self.n_timeout += 1;
                2
            }
        };
        let tb = bucket_index(duration);
        self.time_by_outcome[tb][oi] += 1;
        self.time_bucket_dur[tb] += duration;
    }

    fn print_summary(&self, label: &str) {
        if self.total == 0 {
            return;
        }
        eprintln!();
        eprintln!(
            "── {} ─── {} calls, {:.3}s total (found {}, not-found {}, timeout {})",
            label,
            self.total,
            self.total_time.as_secs_f64(),
            self.n_found,
            self.n_notfound,
            self.n_timeout,
        );
        if self.n_found > 0 {
            let avg = self.size_sum as f64 / self.n_found as f64;
            eprintln!(
                "   MUS sizes: min={} max={} avg={:.1}",
                self.size_min, self.size_max, avg
            );
        }
        eprintln!(
            "   {:>12}  {:>7}  {:>10}  {:>8}  {:>7}  {:>9}",
            "bucket", "found", "not-found", "timeout", "total", "time (s)"
        );
        eprintln!("   {}", "-".repeat(62));
        for b in 0..NUM_TIME_BUCKETS {
            let [found, notfound, timeout] = self.time_by_outcome[b];
            let row_total = found + notfound + timeout;
            if row_total == 0 {
                continue;
            }
            eprintln!(
                "   {:>12}  {:>7}  {:>10}  {:>8}  {:>7}  {:>9.3}",
                BUCKETS[b].1,
                found,
                notfound,
                timeout,
                row_total,
                self.time_bucket_dur[b].as_secs_f64(),
            );
        }
    }
}

/// All collected MUS statistics. Pre-aggregated — O(1) memory regardless of
/// how many MUS searches are performed.
#[derive(Debug, Clone)]
pub struct MusStats {
    /// Per-function aggregated stats. Index via `MusFunction::index()`.
    per_func: [MusFuncStats; NUM_MUS_FUNCTIONS],
    /// Time spent inside `get_many_vars_small_mus_quick` (the parallel batch loop).
    pub phase_batch_mus: PhaseStats,
    /// Time spent inside `quick_solve_impl` (the outer planning loop iterations).
    pub phase_solve_step: PhaseStats,
}

const MUS_STATS_ZERO: MusStats = MusStats {
    per_func: [MUS_FUNC_ZERO; NUM_MUS_FUNCTIONS],
    phase_batch_mus: PhaseStats {
        calls: 0,
        total_time: Duration::ZERO,
    },
    phase_solve_step: PhaseStats {
        calls: 0,
        total_time: Duration::ZERO,
    },
};

impl Default for MusStats {
    fn default() -> Self {
        MUS_STATS_ZERO.clone()
    }
}

impl MusStats {
    fn record_search(&mut self, duration: Duration, outcome: MusOutcome, function: MusFunction) {
        self.per_func[function.index()].record(duration, &outcome);
    }

    fn total_searches(&self) -> usize {
        self.per_func.iter().map(|f| f.total).sum()
    }

    /// Print a human-readable summary to stderr, including per-function timing tables.
    pub fn print_summary(&self) {
        let total = self.total_searches();

        eprintln!("=== MUS Statistics ===");
        eprintln!(
            "Total searches: {}   Batch phase: {} calls {:.3}s   Solve steps: {} {:.3}s",
            total,
            self.phase_batch_mus.calls,
            self.phase_batch_mus.total_time.as_secs_f64(),
            self.phase_solve_step.calls,
            self.phase_solve_step.total_time.as_secs_f64(),
        );

        if total == 0 {
            eprintln!("(no MUS searches recorded)");
            eprintln!("======================");
            return;
        }

        let functions = [
            MusFunction::Size1,
            MusFunction::Slice,
            MusFunction::Cake,
            MusFunction::Quick,
        ];
        for func in functions {
            self.per_func[func.index()].print_summary(func.label());
        }

        eprintln!();
        eprintln!("======================");
    }
}

static MUS_STATS: Mutex<MusStats> = Mutex::new(MUS_STATS_ZERO);

/// Record a single MUS search result into the global stats.
pub fn record_mus_search(duration: Duration, outcome: MusOutcome, function: MusFunction) {
    if let Ok(mut stats) = MUS_STATS.lock() {
        stats.record_search(duration, outcome, function);
    }
}

/// Record time spent in the batch MUS phase.
pub fn record_batch_mus_phase(duration: Duration) {
    if let Ok(mut stats) = MUS_STATS.lock() {
        stats.phase_batch_mus.record(duration);
    }
}

/// Record time spent in a single solve-step iteration.
pub fn record_solve_step(duration: Duration) {
    if let Ok(mut stats) = MUS_STATS.lock() {
        stats.phase_solve_step.record(duration);
    }
}

/// Reset all MUS stats to zero.
pub fn reset_mus_stats() {
    if let Ok(mut stats) = MUS_STATS.lock() {
        *stats = MUS_STATS_ZERO.clone();
    }
}

/// Print a summary of the current MUS stats to stderr.
pub fn print_mus_stats() {
    if let Ok(stats) = MUS_STATS.lock() {
        stats.print_summary();
    }
}

/// A small RAII guard that records a phase duration on drop.
pub struct PhaseTimer {
    start: Instant,
    kind: PhaseKind,
}

pub enum PhaseKind {
    BatchMus,
    SolveStep,
}

impl PhaseTimer {
    pub fn batch_mus() -> Self {
        Self {
            start: Instant::now(),
            kind: PhaseKind::BatchMus,
        }
    }

    pub fn solve_step() -> Self {
        Self {
            start: Instant::now(),
            kind: PhaseKind::SolveStep,
        }
    }
}

impl Drop for PhaseTimer {
    fn drop(&mut self) {
        let d = self.start.elapsed();
        match self.kind {
            PhaseKind::BatchMus => record_batch_mus_phase(d),
            PhaseKind::SolveStep => record_solve_step(d),
        }
    }
}
