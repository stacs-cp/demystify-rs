//! Walkthrough script executor: drives a `PuzzlePlanner` through a sequence
//! of `Step`s, producing zero or more rendered `Section`s for each.
//!
//! State-mutation steps (`Deduce`, `Pin`) advance the planner without
//! emitting output.  Rendering steps (`ShowMus`, `ShowSmallestMuses`,
//! `ShowAllSizeMuses`, `Heatmap`) compute one or more `Problem`s and push
//! them as `Section`s to the executor's accumulator.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};

use crate::json::{DescriptionStatement, Problem};
use crate::problem::PuzLit;
use crate::problem::musdict::MusContext;
use crate::problem::planner::PuzzlePlanner;
use crate::problem::solver::Strategy;

use super::lit::{ParsedLit, parse_lit};
use super::schema::{Script, Step};

/// One rendered piece of the walkthrough output: a heading plus a `Problem`
/// (which the HTML composer turns into an SVG plus statements panel).
#[derive(Debug)]
pub struct Section {
    pub title: String,
    pub problem: Problem,
    /// Per-step annotation (free text) describing why this deduction's
    /// MUS deviates from the source tutorial's reasoning.  Set via the
    /// `note = "..."` field on a show_* TOML step.  Rendered by the HTML
    /// composer below the section heading; intended for later corpus-
    /// wide aggregation.
    pub note: Option<String>,
    /// Free-text narrative prose — separate from `note` so corpus-wide
    /// aggregation of divergence annotations stays clean.  Set via the
    /// `commentary = "..."` field on supported steps (currently
    /// `Heatmap`).  Rendered with its own CSS class.
    pub commentary: Option<String>,
}

pub struct Executor<'a> {
    planner: &'a mut PuzzlePlanner,
    sections: Vec<Section>,
    /// Mirrors `Script::hide_untouched_candidates`.  Threaded through to
    /// every render call so heatmap and show-MUS sections render the same
    /// way.
    hide_untouched_candidates: bool,
}

impl<'a> Executor<'a> {
    pub fn new(planner: &'a mut PuzzlePlanner) -> Self {
        Self {
            planner,
            sections: Vec::new(),
            hide_untouched_candidates: false,
        }
    }

    /// Default `MusConfig::repeats` for tutorial walkthroughs.  Tutorials
    /// are run to surface specific MUSes that match human-authored
    /// reasoning; the standard default of 2 misses too many alternatives
    /// to be useful here.  Overridable per-script via top-level
    /// `repeats = N` and per-step via `repeats = N` on a `[[step]]`.
    const TUTORIAL_DEFAULT_REPEATS: i64 = 50;

    /// Run every step of `script`.  Top-level defaults (`repeats`,
    /// `strategy`) are applied before the first step; per-step overrides
    /// are scoped to that step only.  `repeats` defaults to
    /// [`TUTORIAL_DEFAULT_REPEATS`] when the script does not set it.
    pub fn run(&mut self, script: &Script) -> Result<()> {
        self.planner.config_mut().mus_config.repeats =
            script.repeats.unwrap_or(Self::TUTORIAL_DEFAULT_REPEATS);
        if let Some(s) = script.strategy.as_deref() {
            self.planner.config_mut().mus_config.strategy = parse_strategy(s)?;
        }
        self.hide_untouched_candidates = script.hide_untouched_candidates.unwrap_or(false);

        for (i, step) in script.step.iter().enumerate() {
            self.run_step(i, step)
                .with_context(|| format!("step {} ({:?})", i + 1, step_op_name(step)))?;
        }
        Ok(())
    }

    pub fn into_sections(self) -> Vec<Section> {
        self.sections
    }

    fn run_step(&mut self, _idx: usize, step: &Step) -> Result<()> {
        match step {
            Step::Deduce { lit } => {
                let p = parse_lit(lit)?;
                let puzlit = p.to_puzlit();
                let lit = self.planner.solver().puzlit_to_lit(&puzlit);
                self.planner.solver().add_known_lit(lit);
            }

            Step::Pin { lit } => {
                let p = parse_lit(lit)?;
                let puzlit = p.to_puzlit();
                let lit = self.planner.solver().puzlit_to_lit(&puzlit);
                self.planner.solver().add_not_provable_known_lit(lit);
            }

            Step::ShowMus {
                lit,
                title,
                repeats,
                strategy,
                note,
            } => {
                let p = parse_lit(lit)?;
                self.with_overrides(*repeats, strategy.as_deref(), |this| {
                    let (problem, _lits) = this.planner.solve_step_for_literal(p.lit_def());
                    this.sections.push(Section {
                        title: section_title(title, &p, "MUS"),
                        problem,
                        note: note.clone(),
                        commentary: None,
                    });
                    Ok(())
                })?;
            }

            Step::ShowSmallestMuses {
                lit,
                title,
                repeats,
                strategy,
                max,
                note,
            } => {
                let p = parse_lit(lit)?;
                self.with_overrides(*repeats, strategy.as_deref(), |this| {
                    let muses = this.planner.all_muses_for_literal(p.lit_def());
                    let min = muses.iter().map(|m| m.mus_len()).min().unwrap_or(0);
                    let smallest: Vec<_> = muses
                        .into_iter()
                        .filter(|m| m.mus_len() == min)
                        .take(max.unwrap_or(usize::MAX))
                        .collect();
                    this.push_one_per_mus(
                        &p,
                        title.as_deref(),
                        note.as_deref(),
                        &smallest,
                        "smallest MUSes",
                    )
                })?;
            }

            Step::ShowAllSizeMuses {
                lit,
                title,
                repeats,
                strategy,
                max_size,
                max,
                note,
            } => {
                let p = parse_lit(lit)?;
                self.with_overrides(*repeats, strategy.as_deref(), |this| {
                    let muses = this.planner.all_muses_for_literal(p.lit_def());
                    let muses: Vec<_> = muses
                        .into_iter()
                        .filter(|m| max_size.is_none_or(|c| m.mus_len() <= c))
                        .take(max.unwrap_or(usize::MAX))
                        .collect();
                    this.push_one_per_mus(
                        &p,
                        title.as_deref(),
                        note.as_deref(),
                        &muses,
                        "MUSes (all sizes)",
                    )
                })?;
            }

            Step::Heatmap {
                title,
                note,
                commentary,
            } => {
                let problem = self
                    .planner
                    .difficulty_problem(self.hide_untouched_candidates);
                self.sections.push(Section {
                    title: title.clone().unwrap_or_else(|| "Heatmap".to_string()),
                    problem,
                    note: note.clone(),
                    commentary: commentary.clone(),
                });
            }

            Step::DeduceTrivial {
                max_mus_size,
                title,
                note,
                skip,
            } => {
                let mut total_deduced: usize = 0;
                let mut max_seen_size: usize = 0;
                // Pre-resolve skip-list lits so we can compare cheaply
                // inside the loop.
                let skip_lits: BTreeSet<rustsat::types::Lit> = skip
                    .iter()
                    .map(|s| {
                        let p = parse_lit(s)?;
                        let puzlit = p.to_puzlit();
                        Ok::<_, anyhow::Error>(self.planner.solver().puzlit_to_lit(&puzlit))
                    })
                    .collect::<Result<_>>()?;
                // Saturating loop with a generous safety cap.  Each iteration
                // must deduce at least one new lit (else we'd loop forever).
                for _ in 0..1000 {
                    let muses = self.planner.smallest_muses();
                    if muses.is_empty() {
                        break;
                    }
                    let min_size = muses.iter().map(|m| m.mus_len()).min().unwrap();
                    if min_size > *max_mus_size {
                        // Reached the natural stop: the next available
                        // deduction needs a bigger MUS than this step
                        // claims to handle.  Everything we did apply was
                        // <= max_mus_size, so the assertion holds.
                        break;
                    }
                    if min_size > max_seen_size {
                        max_seen_size = min_size;
                    }
                    // Apply all lits proven by the smallest MUSes, except
                    // those in the skip list.
                    let mut applied_any = false;
                    let lits_to_add: Vec<_> = muses
                        .iter()
                        .flat_map(|mc| mc.lits.iter().copied())
                        .filter(|lit| !skip_lits.contains(lit))
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    for lit in lits_to_add {
                        if !self.planner.get_all_known_lits().contains(&lit) {
                            self.planner.solver().add_known_lit(lit);
                            total_deduced += 1;
                            applied_any = true;
                        }
                    }
                    if !applied_any {
                        // Either every lit was skipped, or only already-known
                        // lits remained — stop cleanly.
                        break;
                    }
                }

                let title_str = title.clone().unwrap_or_else(|| {
                    format!(
                        "Trivial cleanup: {} cell{} via MUSes of size ≤ {} (max actual: {})",
                        total_deduced,
                        if total_deduced == 1 { "" } else { "s" },
                        max_mus_size,
                        max_seen_size,
                    )
                });
                let problem = self
                    .planner
                    .difficulty_problem(self.hide_untouched_candidates);
                self.sections.push(Section {
                    title: title_str,
                    problem,
                    note: note.clone(),
                    commentary: None,
                });
            }
        }
        Ok(())
    }

    /// Render one Problem per MUS, sharing a common state snapshot.  The
    /// planner state is NOT mutated — `show_*` ops are read-only.
    fn push_one_per_mus(
        &mut self,
        target: &ParsedLit,
        user_title: Option<&str>,
        note: Option<&str>,
        muses: &[MusContext],
        label: &str,
    ) -> Result<()> {
        if muses.is_empty() {
            // Render an empty section so the reader sees that the lit had no
            // explanation under the current state — louder than silence.
            self.sections.push(Section {
                title: format!(
                    "{}: no single-target MUSes found",
                    section_title(&user_title.map(str::to_string), target, label)
                ),
                problem: self
                    .planner
                    .difficulty_problem(self.hide_untouched_candidates),
                note: note.map(str::to_string),
                commentary: None,
            });
            return Ok(());
        }

        let target_puzlit = target.to_puzlit();

        for (i, mc) in muses.iter().enumerate() {
            let problem = render_one_mus(
                self.planner,
                mc,
                &target_puzlit,
                self.hide_untouched_candidates,
            )?;
            let base = section_title(&user_title.map(str::to_string), target, label);
            let title = if muses.len() == 1 {
                base
            } else {
                format!(
                    "{base} — #{}/{} (size {})",
                    i + 1,
                    muses.len(),
                    mc.mus_len()
                )
            };
            self.sections.push(Section {
                title,
                problem,
                note: note.map(str::to_string),
                commentary: None,
            });
        }
        Ok(())
    }

    /// Apply per-step `repeats` / `strategy` overrides, run `f`, restore.
    fn with_overrides<R>(
        &mut self,
        repeats: Option<i64>,
        strategy: Option<&str>,
        f: impl FnOnce(&mut Self) -> Result<R>,
    ) -> Result<R> {
        let saved_repeats = self.planner.config_mut().mus_config.repeats;
        let saved_strategy = self.planner.config_mut().mus_config.strategy;
        if let Some(r) = repeats {
            self.planner.config_mut().mus_config.repeats = r;
        }
        if let Some(s) = strategy {
            self.planner.config_mut().mus_config.strategy = parse_strategy(s)?;
        }
        let result = f(self);
        self.planner.config_mut().mus_config.repeats = saved_repeats;
        self.planner.config_mut().mus_config.strategy = saved_strategy;
        result
    }
}

/// Build a `Problem` showing one specific MUS as the explanation for a
/// single deduced literal, without mutating planner state.
fn render_one_mus(
    planner: &mut PuzzlePlanner,
    mc: &MusContext,
    target: &PuzLit,
    hide_untouched_candidates: bool,
) -> Result<Problem> {
    let user_mus = planner.mus_to_user_mus(mc);
    let varlits = planner.solver().get_provable_varlits().clone();
    let known_lits = planner.get_all_known_lits().clone();

    let solver = planner.solver();
    let tosolve_varvals: BTreeSet<_> = varlits
        .iter()
        .flat_map(|x| solver.lit_to_puzlit(x))
        .map(crate::problem::PuzLit::varval)
        .collect();
    let known_puzlits: BTreeSet<PuzLit> = known_lits
        .iter()
        .flat_map(|x| solver.lit_to_puzlit(x))
        .cloned()
        .collect();

    let deduced: BTreeSet<PuzLit> = std::iter::once(target.clone()).collect();

    // Show ONLY the target literal in the deduction text.  The MUS may
    // technically prove other lits as a byproduct (`user_mus.lits` could
    // be a superset of {target}), but for tutorial walkthroughs we want
    // to focus on the one deduction the step is about — the executor
    // upstream filters MUSes that prove >1 lit, so this is the
    // single-target case by construction.
    let result_text = PuzLit::nice_puzlit_list_html(std::iter::once(target));

    let description_list = vec![DescriptionStatement {
        result: result_text,
        constraints: user_mus.constraints,
        name: user_mus.name,
        fingerprint: Some(user_mus.fingerprint),
    }];

    Problem::new_from_puzzle_and_mus(
        solver,
        &tosolve_varvals,
        &known_puzlits,
        &deduced,
        &description_list,
        &format!("MUS of size {}", mc.mus_len()),
        hide_untouched_candidates,
    )
    .context("Cannot make puzzle json")
}

fn parse_strategy(s: &str) -> Result<Strategy> {
    match s.to_lowercase().as_str() {
        "quick" => Ok(Strategy::Quick),
        "slice" => Ok(Strategy::Slice),
        "cake" => Ok(Strategy::Cake),
        "dynamic" => Ok(Strategy::Dynamic),
        other => bail!("unknown strategy '{other}' (expected quick|slice|cake|dynamic)"),
    }
}

fn step_op_name(step: &Step) -> &'static str {
    match step {
        Step::Deduce { .. } => "deduce",
        Step::Pin { .. } => "pin",
        Step::ShowMus { .. } => "show_mus",
        Step::ShowSmallestMuses { .. } => "show_smallest_muses",
        Step::ShowAllSizeMuses { .. } => "show_all_size_muses",
        Step::Heatmap { .. } => "heatmap",
        Step::DeduceTrivial { .. } => "deduce_trivial",
    }
}

fn section_title(title: &Option<String>, target: &ParsedLit, fallback: &str) -> String {
    if let Some(t) = title {
        return t.clone();
    }
    let op = if target.is_eq { "=" } else { "!=" };
    let idx = target
        .indices
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{} for {}[{}]{}{}",
        fallback, target.name, idx, op, target.val
    )
}
