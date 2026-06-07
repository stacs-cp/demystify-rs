//! Planner wrapper exposed to JS via `wasm-bindgen`.
//!
//! Mirrors `demystify-lua/src/planner.rs`: step-by-step puzzle solving with
//! per-step explanations (smallest MUS). Method names match the Lua interface.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::serde_helpers::to_js;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use demystify::problem::{
    PuzLit, PuzVar, VarValPair, format_puzlit, format_puzvar,
    planner::PuzzlePlanner,
    solver::{PuzzleSolver, SolverConfig},
};

use crate::puzzle::WasmPuzzle;

/// JS-facing options accepted by [`WasmPlanner::new`].  Every field is
/// optional and defaults to the same value the CLI uses when its flag is
/// omitted.  Pass an empty object `{}` (or `null` / `undefined`) for the
/// all-defaults shape.
///
/// Fields:
/// - `onlyAssignments`: mirrors the CLI's `--only-assign`.  When true, the
///   planner only targets positive value-assignment lits — `=v` puzlits —
///   and never tries individual `!=v` deductions.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct WasmPlannerOptions {
    only_assignments: bool,
}

impl WasmPlannerOptions {
    fn from_js(options: JsValue) -> Result<Self, JsError> {
        if options.is_null() || options.is_undefined() {
            return Ok(Self::default());
        }
        // Route via serde_json::Value so `deny_unknown_fields` actually
        // fires: serde-wasm-bindgen's direct deserialiser reads requested
        // struct fields by name without iterating the JS object's keys,
        // so typos like `onlyAssingments` would silently fall back to
        // defaults.  The intermediate Map round-trip costs nothing in
        // practice for a tiny options object and turns silent typos into
        // a clear error at the FFI boundary.
        let value: serde_json::Value = serde_wasm_bindgen::from_value(options)
            .map_err(|e| JsError::new(&format!("Invalid WasmPlanner options: {e}")))?;
        serde_json::from_value(value)
            .map_err(|e| JsError::new(&format!("Invalid WasmPlanner options: {e}")))
    }
}

#[derive(Serialize)]
struct StepPayload {
    literals: Vec<String>,
    constraints: Vec<String>,
    mus_size: usize,
    num_muses: usize,
}

#[derive(Serialize)]
struct UserMusPayload {
    literals: Vec<String>,
    constraints: Vec<String>,
    fingerprint: String,
    name: Option<String>,
}

/// One constraint of a MUS: its human-readable description plus the
/// puzzle literals (`var=val`, via [`format_puzvar`]) it mentions.
#[derive(Serialize)]
struct ConstraintPayload {
    text: String,
    literals: Vec<String>,
}

/// A full MUS: the deduced literals, every constraint (text + the
/// literals it mentions), the MUS size, its fingerprint and any matched
/// technique name.
#[derive(Serialize)]
struct MusPayload {
    literals: Vec<String>,
    constraints: Vec<ConstraintPayload>,
    mus_size: usize,
    fingerprint: String,
    name: Option<String>,
}

/// Result of [`WasmPlanner::check_uniqueness`].  The tag/field names
/// are the JS-facing shape:
/// `{status: "unsolvable" | "unique" | "multiple", solution?: [...], unfixedVars?: [...]}`.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum SolvabilityPayload {
    Unsolvable,
    Unique {
        /// The unique solution, as `var=val` strings (one per assigned
        /// puzzle literal), in the same `format_puzlit` form as
        /// [`WasmPlanner::provable_literals`].
        solution: Vec<String>,
    },
    Multiple {
        #[serde(rename = "unfixedVars")]
        unfixed_vars: Vec<String>,
    },
}

/// Render a [`VarValPair`] as `var=val` for the FFI surface, reusing
/// [`format_puzvar`] so scalar and indexed variables match `format_puzlit`.
fn format_varval(vv: &VarValPair) -> String {
    format!("{}={}", format_puzvar(vv.var()), vv.val())
}

/// JS-facing planner handle.
#[wasm_bindgen]
pub struct WasmPlanner {
    inner: Arc<Mutex<PuzzlePlanner>>,
}

/// Install demystify's `tracing` subscriber into the browser console.
///
/// Call once early (e.g. right after `await init()` on the JS side); it
/// pipes every `info!`/`debug!` produced by demystify's solver and
/// planner into `console.log` / `console.debug`, which is the cleanest
/// way to see what `difficulties()` is doing in the browser without
/// rebuilding wasm.  Safe to call multiple times — the second and later
/// calls are no-ops.
///
/// `max_level` is one of `"trace" | "debug" | "info" | "warn" | "error"`.
/// Defaults to `"info"`.
#[wasm_bindgen(js_name = enableConsoleLogs)]
pub fn enable_console_logs(max_level: Option<String>) {
    use std::sync::Once;
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let lvl = max_level.as_deref().unwrap_or("info");
        let level = match lvl {
            "trace" => tracing::Level::TRACE,
            "debug" => tracing::Level::DEBUG,
            "info" => tracing::Level::INFO,
            "warn" => tracing::Level::WARN,
            "error" => tracing::Level::ERROR,
            other => {
                web_sys::console::warn_1(
                    &format!("enableConsoleLogs: unknown level {other:?}; using info").into(),
                );
                tracing::Level::INFO
            }
        };
        let cfg = tracing_wasm::WASMLayerConfigBuilder::new()
            .set_max_level(level)
            .build();
        tracing_wasm::set_as_global_default_with_config(cfg);
    });
}

#[wasm_bindgen]
impl WasmPlanner {
    /// Build a planner from a puzzle.  `options` is a required JS object
    /// whose fields are documented on [`WasmPlannerOptions`]; pass `{}` (or
    /// `null` / `undefined`) for all-defaults.  Errors if `options` is
    /// malformed or the SAT setup fails.
    #[wasm_bindgen(constructor)]
    pub fn new(puzzle: &WasmPuzzle, options: JsValue) -> Result<WasmPlanner, JsError> {
        let opts = WasmPlannerOptions::from_js(options)?;
        let solver_config = SolverConfig {
            only_assignments: opts.only_assignments,
        };
        let solver = PuzzleSolver::new_with_config(puzzle.arc(), solver_config)
            .map_err(|e| JsError::new(&format!("Failed to create solver: {e}")))?;
        let planner = PuzzlePlanner::new(solver);
        Ok(WasmPlanner {
            inner: Arc::new(Mutex::new(planner)),
        })
    }

    #[wasm_bindgen(js_name = isSolved)]
    pub fn is_solved(&self) -> bool {
        let planner = self.inner.lock().unwrap();
        !planner.get_all_known_lits().is_empty()
            && planner.puzzle().var_lits.positive().iter().all(|lit| {
                planner.get_all_known_lits().contains(lit)
                    || planner.get_all_known_lits().contains(&(!*lit))
            })
    }

    #[wasm_bindgen(js_name = numProvable)]
    pub fn num_provable(&self) -> usize {
        let mut planner = self.inner.lock().unwrap();
        planner.get_provable_varlits().len()
    }

    #[wasm_bindgen(js_name = provableLiterals)]
    pub fn provable_literals(&self) -> Result<JsValue, JsError> {
        let mut planner = self.inner.lock().unwrap();
        let provable = planner.get_provable_varlits();
        let mut out = Vec::new();
        for lit in &provable {
            for puzlit in planner.puzzle().lit_to_vars(lit) {
                out.push(format_puzlit(puzlit));
            }
        }
        to_js(&out).map_err(Into::into)
    }

    /// Classify the puzzle's solution status. Runs propagation to a
    /// fixed point on a *cloned* planner (so this call does not mutate
    /// the caller's state) and returns one of:
    ///
    /// - `{status: "unsolvable"}` — no satisfying assignment exists.
    /// - `{status: "unique", solution: ["grid[1,1]=5", ...]}` — exactly
    ///   one solution; `solution` lists every assigned puzzle literal.
    /// - `{status: "multiple", unfixedVars: ["grid[2,3]", ...]}` —
    ///   multiple solutions; `unfixedVars` lists the puzzle variables
    ///   whose value is not pinned by the current clues.
    ///
    /// REVEAL targets are propagated automatically because the
    /// fixed-point loop keeps deducing as new triggers fire.
    #[wasm_bindgen(js_name = checkUniqueness)]
    pub fn check_uniqueness(&self) -> Result<JsValue, JsError> {
        let mut planner: PuzzlePlanner = self.inner.lock().unwrap().clone();
        let payload = match planner.check_solvability() {
            None => SolvabilityPayload::Unsolvable,
            Some(0) => {
                // check_solvability propagated to a fixed point, so every
                // puzzle variable is now decided.  The solution is the
                // equality (`var=val`) puzlit each variable resolved to; the
                // matching `!=` puzlits are dropped by the `sign()` filter.
                let known: std::collections::HashSet<_> =
                    planner.get_all_known_lits().iter().copied().collect();
                let mut solution = Vec::new();
                for lit in planner.puzzle().var_lits.positive() {
                    if known.contains(lit) {
                        for puzlit in planner.puzzle().lit_to_vars(lit) {
                            if puzlit.sign() {
                                solution.push(format_puzlit(puzlit));
                            }
                        }
                    }
                }
                SolvabilityPayload::Unique { solution }
            }
            Some(_) => {
                let vars = planner.unsolved_vars_after_solve();
                let unfixed_vars = vars.iter().map(format_puzvar).collect();
                SolvabilityPayload::Multiple { unfixed_vars }
            }
        };
        to_js(&payload).map_err(Into::into)
    }

    /// Mark a literal as deduced. Returns true if the literal was found, false
    /// otherwise. Matches `LuaPlanner::fix_literal`.
    #[wasm_bindgen(js_name = fixLiteral)]
    pub fn fix_literal(&self, lit_str: &str) -> bool {
        let mut planner = self.inner.lock().unwrap();
        let provable = planner.get_provable_varlits();

        for lit in &provable {
            for puzlit in planner.puzzle().lit_to_vars(lit) {
                if format_puzlit(puzlit) == lit_str {
                    if planner
                        .puzzle()
                        .check_fixable_var(puzlit.var().name())
                        .is_err()
                    {
                        return false;
                    }
                    let lit_copy = *lit;
                    planner.mark_lit_as_fixed(&lit_copy);
                    return true;
                }
            }
        }

        let matching = {
            let mut found = None;
            for (puzlit, sat_lit) in planner.puzzle().direct.litmap.iter() {
                if format_puzlit(puzlit) == lit_str {
                    found = Some((*sat_lit, puzlit.var().name().clone()));
                    break;
                }
            }
            found
        };

        if let Some((lit, var_name)) = matching {
            if planner.puzzle().check_fixable_var(&var_name).is_err() {
                return false;
            }
            planner.mark_lit_as_fixed(&lit);
            return true;
        }
        false
    }

    /// Mark a literal as deduced by name + indices + value.
    #[wasm_bindgen(js_name = fixVar)]
    pub fn fix_var(&self, name: &str, indices: Vec<i64>, value: i64) -> bool {
        self.fix_internal(name, indices, value, true)
    }

    /// Mark a literal as deduced from a `{name, indices, value, equal?}` object.
    pub fn fix(&self, args: JsValue) -> Result<bool, JsError> {
        #[derive(serde::Deserialize)]
        struct FixArgs {
            name: String,
            indices: Vec<i64>,
            value: i64,
            #[serde(default = "default_true")]
            equal: bool,
        }
        fn default_true() -> bool {
            true
        }

        let args: FixArgs = serde_wasm_bindgen::from_value(args)
            .map_err(|e| JsError::new(&format!("Invalid fix() argument: {e}")))?;
        Ok(self.fix_internal(&args.name, args.indices, args.value, args.equal))
    }

    fn fix_internal(&self, name: &str, indices: Vec<i64>, value: i64, equal: bool) -> bool {
        let mut planner = self.inner.lock().unwrap();
        if planner.puzzle().check_fixable_var(name).is_err() {
            return false;
        }
        let puzvar = PuzVar::new(name, indices);
        let varval = VarValPair::new(&puzvar, value);
        let puzlit = if equal {
            PuzLit::new_eq(varval)
        } else {
            PuzLit::new_neq(varval)
        };
        if let Some(&sat_lit) = planner.puzzle().direct.litmap.get(&puzlit) {
            planner.mark_lit_as_fixed(&sat_lit);
            true
        } else {
            false
        }
    }

    /// Compute the next-best step: returns `null` if nothing more to deduce.
    /// On success, returns `{literals, constraints, mus_size, num_muses}` and
    /// marks the deduced literals as known (parity with the Lua wrapper).
    #[wasm_bindgen(js_name = bestStep)]
    pub fn best_step(&self) -> Result<JsValue, JsError> {
        let mut planner = self.inner.lock().unwrap();
        let muses = planner.smallest_muses_with_config();
        if muses.is_empty() {
            return Ok(JsValue::NULL);
        }

        let mut all_lits = Vec::new();
        let mut all_constraints = Vec::new();
        for mus in &muses {
            let user_mus = planner.mus_to_user_mus(mus);
            for lit in user_mus.lits {
                all_lits.push(format_puzlit(&lit));
            }
            all_constraints.extend(user_mus.constraints);
        }

        for mus in &muses {
            for lit in &mus.lits {
                planner.mark_lit_as_deduced(lit);
            }
        }

        let payload = StepPayload {
            literals: all_lits,
            constraints: all_constraints,
            mus_size: muses.first().map(|m| m.mus_len()).unwrap_or(0),
            num_muses: muses.len(),
        };
        to_js(&payload).map_err(Into::into)
    }

    /// Every deduction whose smallest MUS is of the globally-minimum size,
    /// returned as a `Vec<MusPayload>` — full MUS detail (deduced literals,
    /// each constraint's English text and the literals it mentions, size,
    /// fingerprint, technique name).
    ///
    /// Sits between [`Self::best_step`] (returns a single smallest MUS, and
    /// marks it deduced) and [`Self::difficulties`] (sizes every literal far
    /// past the minimum, the slow path).  Unlike `bestStep`, this is a pure
    /// query: it deduces nothing, so it can be called repeatedly from a fixed
    /// state.
    #[wasm_bindgen(js_name = allMinSteps)]
    pub fn all_min_steps(&self) -> Result<JsValue, JsError> {
        let mut planner = self.inner.lock().unwrap();
        let muses = planner.all_minimum_size_muses();
        let mut out: Vec<MusPayload> = Vec::with_capacity(muses.len());
        for mc in &muses {
            let user_mus = planner.mus_to_user_mus(mc);
            let parse = planner.puzzle();
            let constraints: Vec<ConstraintPayload> = mc
                .mus
                .iter()
                .map(|c| ConstraintPayload {
                    text: parse.lit_to_con(c).clone(),
                    literals: parse
                        .constraint_scope_for_lit(c)
                        .iter()
                        .map(format_varval)
                        .collect(),
                })
                .collect();
            out.push(MusPayload {
                literals: user_mus.lits.iter().map(format_puzlit).collect(),
                constraints,
                mus_size: mc.mus_len(),
                fingerprint: user_mus.fingerprint,
                name: user_mus.name,
            });
        }
        to_js(&out).map_err(Into::into)
    }

    /// Solve the whole puzzle, returning `Vec<Vec<UserMusPayload>>`.
    #[wasm_bindgen(js_name = quickSolve)]
    pub fn quick_solve(&self) -> Result<JsValue, JsError> {
        let mut planner = self.inner.lock().unwrap();
        let solve = planner.quick_solve();
        let payload: Vec<Vec<UserMusPayload>> = solve
            .into_iter()
            .map(|step| {
                step.into_iter()
                    .map(|um| UserMusPayload {
                        literals: um.lits.iter().map(format_puzlit).collect(),
                        constraints: um.constraints,
                        fingerprint: um.fingerprint,
                        name: um.name,
                    })
                    .collect()
            })
            .collect();
        to_js(&payload).map_err(Into::into)
    }

    /// Difficulty (smallest MUS size) for every provable literal.
    /// Returns an object keyed by literal string.
    ///
    /// Provable lits that the MUS search couldn't size — e.g. because
    /// the per-call conflict limit hit, or no MUS was returned — are
    /// reported with a sentinel value equal to the puzzle's total
    /// constraint count.  No real MUS can have more constraints than
    /// exist, so the sentinel sits strictly above every genuine
    /// difficulty.  In the UI this gives a clean "deducible, but we
    /// don't know how hard" bucket that always sorts to the bottom.
    pub fn difficulties(&self) -> Result<JsValue, JsError> {
        let mut planner = self.inner.lock().unwrap();
        let muses = planner.all_muses_with_larger();
        let mut out: BTreeMap<String, usize> = BTreeMap::new();
        // Expand every puzlit each SAT lit can witness, matching the
        // shape of provableLiterals().  One SAT lit can correspond to
        // multiple PuzLits via the direct encoding (e.g. cell=0 is
        // equivalent to cell!=1 and cell!=2 for a 0..2 domain), and
        // callers expect difficulties() to cover all of them.
        for (lit, mus_set) in muses.muses() {
            let Some(min_len) = mus_set.iter().map(|mc| mc.mus_len()).min() else {
                continue;
            };
            for puzlit in planner.puzzle().lit_to_vars(lit) {
                out.insert(format_puzlit(puzlit), min_len);
            }
        }
        let sentinel = planner.puzzle().constraints.len();
        let provable = planner.get_provable_varlits().clone();
        for lit in &provable {
            for puzlit in planner.puzzle().lit_to_vars(lit) {
                out.entry(format_puzlit(puzlit)).or_insert(sentinel);
            }
        }
        to_js(&out).map_err(Into::into)
    }

    /// Diagnostic wrapper around [`Self::difficulties`].  Logs every
    /// phase of the underlying `all_muses_with_larger` pass via
    /// `console.log` and returns a JS object with the same per-phase
    /// counts so a JS harness can assert on them.
    ///
    /// Use only for debugging — it's slower than `difficulties()`
    /// because it re-walks the dict to surface per-phase stats and
    /// it also drives the dict-walk twice (once for the summary, once
    /// for the returned mapping).
    ///
    /// Companion to [`enable_console_logs`]: call that once to get
    /// demystify's own tracing logs as well.
    ///
    /// The returned object has the shape:
    /// `{ provable: number, dict_entries: number, lits_with_sized_mus: number,
    ///    min_mus_size: number | null, difficulties: { lit: size, ... } }`
    #[wasm_bindgen(js_name = difficultiesDebug)]
    pub fn difficulties_debug(&self) -> Result<JsValue, JsError> {
        use web_sys::console;
        console::log_1(&"difficultiesDebug: locking planner".into());
        let mut planner = self.inner.lock().unwrap();

        let provable = planner.get_provable_varlits().len();
        console::log_1(&format!("difficultiesDebug: provable_varlits = {provable}").into());

        console::log_1(&"difficultiesDebug: calling all_muses_with_larger".into());
        let muses = planner.all_muses_with_larger();
        let dict_entries = muses.muses().len();
        console::log_1(&format!("difficultiesDebug: dict entries = {dict_entries}").into());

        let mut lits_with_sized_mus = 0_usize;
        let mut min_mus_size: Option<usize> = None;
        let mut out: BTreeMap<String, usize> = BTreeMap::new();
        for (lit, mus_set) in muses.muses() {
            let Some(min_len) = mus_set.iter().map(|mc| mc.mus_len()).min() else {
                continue;
            };
            if min_len >= 1 {
                lits_with_sized_mus += 1;
                min_mus_size = Some(min_mus_size.map_or(min_len, |m| m.min(min_len)));
            }
            let puzlits = planner.puzzle().lit_to_vars(lit);
            if let Some(first) = puzlits.iter().next() {
                out.insert(format_puzlit(first), min_len);
            }
        }
        console::log_1(
            &format!(
                "difficultiesDebug: lits_with_sized_mus = {lits_with_sized_mus}, \
                 min_mus_size = {min_mus_size:?}, difficulties.len() = {}",
                out.len()
            )
            .into(),
        );

        #[derive(Serialize)]
        struct DebugPayload {
            provable: usize,
            dict_entries: usize,
            lits_with_sized_mus: usize,
            min_mus_size: Option<usize>,
            difficulties: BTreeMap<String, usize>,
        }
        to_js(&DebugPayload {
            provable,
            dict_entries,
            lits_with_sized_mus,
            min_mus_size,
            difficulties: out,
        })
        .map_err(Into::into)
    }

    /// All literals currently known (deduced or fixed).
    #[wasm_bindgen(js_name = knownLiterals)]
    pub fn known_literals(&self) -> Result<JsValue, JsError> {
        let planner = self.inner.lock().unwrap();
        let mut out = Vec::new();
        for lit in planner.get_all_known_lits() {
            let puzlits = planner.puzzle().lit_to_vars(lit);
            if let Some(first) = puzlits.iter().next() {
                out.push(format_puzlit(first));
            }
        }
        to_js(&out).map_err(Into::into)
    }

    /// Current assignments as a nested object: `{var_name: {idx1: {idx2: val}}}`.
    /// For unindexed variables: `{var_name: val}`.
    #[wasm_bindgen(js_name = currentState)]
    pub fn current_state(&self) -> Result<JsValue, JsError> {
        use serde_json::Value;
        let planner = self.inner.lock().unwrap();
        let mut root: serde_json::Map<String, Value> = serde_json::Map::new();

        for lit in planner.get_all_known_lits() {
            let puzlits = planner.puzzle().lit_to_vars(lit);
            for puzlit in puzlits {
                if !puzlit.sign() {
                    continue;
                }
                let var_name = puzlit.var().name().clone();
                let indices = puzlit.var().indices().clone();
                let val = Value::Number(puzlit.val().into());

                if indices.is_empty() {
                    root.insert(var_name, val);
                    continue;
                }

                let entry = root
                    .entry(var_name)
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                let mut current = entry;
                for (pos, idx) in indices.iter().enumerate() {
                    let key = idx.to_string();
                    let obj = match current {
                        Value::Object(o) => o,
                        _ => unreachable!("nested state node must be an object"),
                    };
                    if pos == indices.len() - 1 {
                        obj.insert(key, val.clone());
                        break;
                    }
                    let child = obj
                        .entry(key)
                        .or_insert_with(|| Value::Object(serde_json::Map::new()));
                    current = child;
                }
            }
        }

        to_js(&Value::Object(root)).map_err(Into::into)
    }
}
