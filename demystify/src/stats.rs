/// Global MUS statistics collector.
///
/// Tracks per-MUS-search outcomes (time + size or failure) and overall
/// phase timing so we can identify where the solver spends its time.
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
    fn record_search(&mut self, duration: Duration, outcome: MusOutcome) {
        self.searches.push(MusRecord { duration, outcome });
    }

    /// Print a human-readable summary to stderr.
    pub fn print_summary(&self) {
        let total_searches = self.searches.len();
        let found: Vec<_> = self
            .searches
            .iter()
            .filter_map(|r| {
                if let MusOutcome::Found(n) = r.outcome {
                    Some((n, r.duration))
                } else {
                    None
                }
            })
            .collect();
        let not_found: Vec<_> = self
            .searches
            .iter()
            .filter(|r| matches!(r.outcome, MusOutcome::NotFound))
            .collect();
        let timeouts: Vec<_> = self
            .searches
            .iter()
            .filter(|r| matches!(r.outcome, MusOutcome::Timeout))
            .collect();

        let found_time: Duration = found.iter().map(|(_, d)| *d).sum();
        let not_found_time: Duration = not_found.iter().map(|r| r.duration).sum();
        let timeout_time: Duration = timeouts.iter().map(|r| r.duration).sum();
        let total_search_time: Duration = self.searches.iter().map(|r| r.duration).sum();

        eprintln!("=== MUS Statistics ===");
        eprintln!("Total MUS searches:   {total_searches}");
        eprintln!(
            "  Found:            {} ({:.1}%) — total {:.3}s, avg {:.3}ms",
            found.len(),
            100.0 * found.len() as f64 / total_searches.max(1) as f64,
            found_time.as_secs_f64(),
            found_time.as_secs_f64() * 1000.0 / found.len().max(1) as f64,
        );
        if !found.is_empty() {
            let sizes: Vec<usize> = found.iter().map(|(n, _)| *n).collect();
            let min = sizes.iter().min().copied().unwrap_or(0);
            let max = sizes.iter().max().copied().unwrap_or(0);
            let avg = sizes.iter().sum::<usize>() as f64 / sizes.len() as f64;
            eprintln!("  MUS sizes:        min={min}, max={max}, avg={avg:.1}");
            // Distribution bucketed by size
            let mut buckets: std::collections::BTreeMap<usize, usize> =
                std::collections::BTreeMap::new();
            for &s in &sizes {
                *buckets.entry(s).or_insert(0) += 1;
            }
            let entries: Vec<_> = buckets.iter().collect();
            if entries.len() <= 10 {
                let dist: Vec<String> = entries.iter().map(|(k, v)| format!("{k}:{v}")).collect();
                eprintln!("  Size distribution: {}", dist.join(", "));
            } else {
                // Show top 10 most common sizes
                let mut sorted = entries.clone();
                sorted.sort_by_key(|(_, v)| std::cmp::Reverse(**v));
                let top: Vec<String> = sorted
                    .iter()
                    .take(10)
                    .map(|(k, v)| format!("{k}:{v}"))
                    .collect();
                eprintln!("  Top sizes (size:count): {}", top.join(", "));
            }
        }
        eprintln!(
            "  Not found:        {} ({:.1}%) — total {:.3}s, avg {:.3}ms",
            not_found.len(),
            100.0 * not_found.len() as f64 / total_searches.max(1) as f64,
            not_found_time.as_secs_f64(),
            not_found_time.as_secs_f64() * 1000.0 / not_found.len().max(1) as f64,
        );
        eprintln!(
            "  Timeout:          {} ({:.1}%) — total {:.3}s",
            timeouts.len(),
            100.0 * timeouts.len() as f64 / total_searches.max(1) as f64,
            timeout_time.as_secs_f64(),
        );
        eprintln!(
            "Total search time:    {:.3}s",
            total_search_time.as_secs_f64()
        );
        eprintln!(
            "Batch MUS phase:      {} calls, {:.3}s total",
            self.phase_batch_mus.calls,
            self.phase_batch_mus.total_time.as_secs_f64(),
        );
        eprintln!(
            "Solve step phase:     {} steps, {:.3}s total",
            self.phase_solve_step.calls,
            self.phase_solve_step.total_time.as_secs_f64(),
        );
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
pub fn record_mus_search(duration: Duration, outcome: MusOutcome) {
    if let Ok(mut stats) = MUS_STATS.lock() {
        stats.record_search(duration, outcome);
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
