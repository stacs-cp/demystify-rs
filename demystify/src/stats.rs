/// Global MUS statistics collector.
///
/// Tracks per-MUS-search outcomes (time + size or failure) broken down by
/// which internal function was called, per-SAT-call time and conflict counts,
/// and overall phase timing.
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
            MusFunction::Cake  => "cake",
            MusFunction::Quick => "quick",
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

/// A single recorded MUS search.
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
    (1,     "< 1ms"),
    (5,     "1–5ms"),
    (20,    "5–20ms"),
    (100,   "20–100ms"),
    (500,   "100–500ms"),
    (2_000, "500ms–2s"),
    (u128::MAX, "> 2s"),
];

fn bucket_index(d: Duration) -> usize {
    let ms = d.as_millis();
    BUCKETS.iter().position(|&(limit, _)| ms < limit).unwrap_or(BUCKETS.len() - 1)
}

/// Conflict-count buckets for per-SAT-call distribution table.
const CONFLICT_BUCKETS: &[(usize, &str)] = &[
    (1,      "0 conflicts"),
    (10,     "1–9"),
    (100,    "10–99"),
    (1_000,  "100–999"),
    (10_000, "1k–9k"),
    (usize::MAX, "≥ 10k"),
];

fn conflict_bucket_index(n: usize) -> usize {
    CONFLICT_BUCKETS.iter().position(|&(limit, _)| n < limit).unwrap_or(CONFLICT_BUCKETS.len() - 1)
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
            rustsat::solvers::SolverResult::Sat         => SatResult::Sat,
            rustsat::solvers::SolverResult::Unsat       => SatResult::Unsat,
            rustsat::solvers::SolverResult::Interrupted => SatResult::Interrupted,
        }
    }
}

/// One recorded SAT solver invocation.
#[derive(Debug, Clone)]
pub struct SatCallRecord {
    pub duration:  Duration,
    pub conflicts: usize,
    pub result:    SatResult,
}

#[derive(Debug, Default)]
struct SatStats {
    calls: Vec<SatCallRecord>,
}

impl SatStats {
    fn record(&mut self, duration: Duration, conflicts: usize, result: SatResult) {
        self.calls.push(SatCallRecord { duration, conflicts, result });
    }

    fn print_summary(&self) {
        let n = self.calls.len();
        eprintln!("=== SAT Call Statistics ===");
        eprintln!("Total SAT calls: {n}");
        if n == 0 {
            eprintln!("===========================");
            return;
        }

        let total_time: Duration = self.calls.iter().map(|r| r.duration).sum();
        let total_conflicts: usize = self.calls.iter().map(|r| r.conflicts).sum();
        let n_sat         = self.calls.iter().filter(|r| r.result == SatResult::Sat).count();
        let n_unsat       = self.calls.iter().filter(|r| r.result == SatResult::Unsat).count();
        let n_interrupted = self.calls.iter().filter(|r| r.result == SatResult::Interrupted).count();
        eprintln!(
            "sat {n_sat}  unsat {n_unsat}  interrupted {n_interrupted}  \
             total time {:.3}s  total conflicts {total_conflicts}",
            total_time.as_secs_f64(),
        );

        // ── Time bucket table ─────────────────────────────────────────────
        eprintln!();
        eprintln!("  Time distribution:");
        let nb = BUCKETS.len();
        let mut time_sat  = vec![0u32; nb];
        let mut time_unsat= vec![0u32; nb];
        let mut time_int  = vec![0u32; nb];
        let mut bucket_time = vec![Duration::ZERO; nb];
        for r in &self.calls {
            let b = bucket_index(r.duration);
            bucket_time[b] += r.duration;
            match r.result {
                SatResult::Sat         => time_sat[b]   += 1,
                SatResult::Unsat       => time_unsat[b] += 1,
                SatResult::Interrupted => time_int[b]   += 1,
            }
        }
        eprintln!(
            "  {:>12}  {:>7}  {:>7}  {:>11}  {:>7}  {:>9}",
            "bucket", "sat", "unsat", "interrupted", "total", "time (s)"
        );
        eprintln!("  {}", "-".repeat(62));
        for b in 0..nb {
            let row = time_sat[b] + time_unsat[b] + time_int[b];
            if row == 0 { continue; }
            eprintln!(
                "  {:>12}  {:>7}  {:>7}  {:>11}  {:>7}  {:>9.3}",
                BUCKETS[b].1, time_sat[b], time_unsat[b], time_int[b], row,
                bucket_time[b].as_secs_f64(),
            );
        }

        // ── Conflict bucket table ─────────────────────────────────────────
        eprintln!();
        eprintln!("  Conflict distribution:");
        let cb = CONFLICT_BUCKETS.len();
        let mut conf_sat  = vec![0u32; cb];
        let mut conf_unsat= vec![0u32; cb];
        let mut conf_int  = vec![0u32; cb];
        let mut bucket_conf = vec![0usize; cb];
        for r in &self.calls {
            let b = conflict_bucket_index(r.conflicts);
            bucket_conf[b] += r.conflicts;
            match r.result {
                SatResult::Sat         => conf_sat[b]   += 1,
                SatResult::Unsat       => conf_unsat[b] += 1,
                SatResult::Interrupted => conf_int[b]   += 1,
            }
        }
        eprintln!(
            "  {:>14}  {:>7}  {:>7}  {:>11}  {:>7}  {:>12}",
            "bucket", "sat", "unsat", "interrupted", "total", "conflicts"
        );
        eprintln!("  {}", "-".repeat(66));
        for b in 0..cb {
            let row = conf_sat[b] + conf_unsat[b] + conf_int[b];
            if row == 0 { continue; }
            eprintln!(
                "  {:>14}  {:>7}  {:>7}  {:>11}  {:>7}  {:>12}",
                CONFLICT_BUCKETS[b].1, conf_sat[b], conf_unsat[b], conf_int[b], row, bucket_conf[b],
            );
        }

        eprintln!();
        eprintln!("===========================");
    }
}

static SAT_STATS: Mutex<SatStats> = Mutex::new(SatStats { calls: Vec::new() });

/// Record a single SAT solver call into the global stats.
pub fn record_sat_call(duration: Duration, conflicts: usize, result: rustsat::solvers::SolverResult) {
    if let Ok(mut s) = SAT_STATS.lock() {
        s.record(duration, conflicts, result.into());
    }
}

/// Reset SAT call stats to zero.
pub fn reset_sat_stats() {
    if let Ok(mut s) = SAT_STATS.lock() {
        *s = SatStats::default();
    }
}

/// Print a summary of per-SAT-call stats to stderr.
pub fn print_sat_stats() {
    if let Ok(s) = SAT_STATS.lock() {
        s.print_summary();
    }
}

/// All collected statistics.
#[derive(Debug, Default)]
pub struct MusStats {
    /// One record per individual MUS search call.
    pub searches: Vec<MusRecord>,
    /// Time spent inside `get_many_vars_small_mus_quick` (the parallel batch loop).
    pub phase_batch_mus: PhaseStats,
    /// Time spent inside `quick_solve_impl` (the outer planning loop iterations).
    pub phase_solve_step: PhaseStats,
}

impl MusStats {
    fn record_search(&mut self, duration: Duration, outcome: MusOutcome, function: MusFunction) {
        self.searches.push(MusRecord { duration, outcome, function });
    }

    /// Print a human-readable summary to stderr, including per-function timing tables.
    pub fn print_summary(&self) {
        let total = self.searches.len();

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

        // Collect per-function records
        let functions = [
            MusFunction::Size1,
            MusFunction::Slice,
            MusFunction::Cake,
            MusFunction::Quick,
        ];

        for func in functions {
            let records: Vec<&MusRecord> = self.searches.iter()
                .filter(|r| r.function == func)
                .collect();

            if records.is_empty() {
                continue;
            }

            let total_time: Duration = records.iter().map(|r| r.duration).sum();
            let n_found    = records.iter().filter(|r| matches!(r.outcome, MusOutcome::Found(_))).count();
            let n_notfound = records.iter().filter(|r| matches!(r.outcome, MusOutcome::NotFound)).count();
            let n_timeout  = records.iter().filter(|r| matches!(r.outcome, MusOutcome::Timeout)).count();

            eprintln!();
            eprintln!(
                "── {} ─── {} calls, {:.3}s total (found {}, not-found {}, timeout {})",
                func.label(), records.len(), total_time.as_secs_f64(),
                n_found, n_notfound, n_timeout,
            );

            // MUS size distribution for Found
            let mut sizes: Vec<usize> = records.iter()
                .filter_map(|r| if let MusOutcome::Found(n) = r.outcome { Some(n) } else { None })
                .collect();
            if !sizes.is_empty() {
                sizes.sort_unstable();
                let min = sizes[0];
                let max = *sizes.last().unwrap();
                let avg = sizes.iter().sum::<usize>() as f64 / sizes.len() as f64;
                eprintln!("   MUS sizes: min={min} max={max} avg={avg:.1}");
            }

            // Timing bucket table
            let nb = BUCKETS.len();
            let mut found_counts    = vec![0u32; nb];
            let mut notfound_counts = vec![0u32; nb];
            let mut timeout_counts  = vec![0u32; nb];
            let mut bucket_time     = vec![Duration::ZERO; nb];

            for r in &records {
                let b = bucket_index(r.duration);
                bucket_time[b] += r.duration;
                match r.outcome {
                    MusOutcome::Found(_) => found_counts[b]    += 1,
                    MusOutcome::NotFound => notfound_counts[b] += 1,
                    MusOutcome::Timeout  => timeout_counts[b]  += 1,
                }
            }

            eprintln!(
                "   {:>12}  {:>7}  {:>10}  {:>8}  {:>7}  {:>9}",
                "bucket", "found", "not-found", "timeout", "total", "time (s)"
            );
            eprintln!("   {}", "-".repeat(62));
            for b in 0..nb {
                let row_total = found_counts[b] + notfound_counts[b] + timeout_counts[b];
                if row_total == 0 { continue; }
                eprintln!(
                    "   {:>12}  {:>7}  {:>10}  {:>8}  {:>7}  {:>9.3}",
                    BUCKETS[b].1,
                    found_counts[b],
                    notfound_counts[b],
                    timeout_counts[b],
                    row_total,
                    bucket_time[b].as_secs_f64(),
                );
            }
        }

        eprintln!();
        eprintln!("======================");
    }
}

static MUS_STATS: Mutex<MusStats> = Mutex::new(MusStats {
    searches: Vec::new(),
    phase_batch_mus: PhaseStats {
        calls: 0,
        total_time: Duration::ZERO,
    },
    phase_solve_step: PhaseStats {
        calls: 0,
        total_time: Duration::ZERO,
    },
});

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

/// Get a snapshot of the current stats.
pub fn get_mus_stats() -> MusStats {
    let stats = MUS_STATS.lock().unwrap();
    MusStats {
        searches: stats.searches.clone(),
        phase_batch_mus: stats.phase_batch_mus.clone(),
        phase_solve_step: stats.phase_solve_step.clone(),
    }
}

/// Reset all stats to zero.
pub fn reset_mus_stats() {
    if let Ok(mut stats) = MUS_STATS.lock() {
        *stats = MusStats::default();
    }
}

/// Print a summary of the current stats to stderr.
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
        Self { start: Instant::now(), kind: PhaseKind::BatchMus }
    }

    pub fn solve_step() -> Self {
        Self { start: Instant::now(), kind: PhaseKind::SolveStep }
    }
}

impl Drop for PhaseTimer {
    fn drop(&mut self) {
        let d = self.start.elapsed();
        match self.kind {
            PhaseKind::BatchMus  => record_batch_mus_phase(d),
            PhaseKind::SolveStep => record_solve_step(d),
        }
    }
}
