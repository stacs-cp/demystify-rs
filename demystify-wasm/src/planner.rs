//! Planner wrapper exposed to JS via `wasm-bindgen`.
//!
//! Mirrors `demystify-lua/src/planner.rs`: step-by-step puzzle solving with
//! per-step explanations (smallest MUS). Method names match the Lua interface.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_wasm_bindgen::to_value;
use wasm_bindgen::prelude::*;

use demystify::problem::{
    PuzLit, PuzVar, VarValPair, planner::PuzzlePlanner, solver::PuzzleSolver,
};

use crate::puzzle::WasmPuzzle;

/// Format a `PuzLit` as `"grid[1, 2]=5"` (or `"!="` for negations).
/// Matches the formatting used by `demystify-lua`.
fn format_puzlit(lit: &PuzLit) -> String {
    let var = lit.var();
    let val = lit.val();
    let sign = if lit.sign() { "=" } else { "!=" };
    if var.indices().is_empty() {
        format!("{}{}{}", var.name(), sign, val)
    } else {
        format!("{}{:?}{}{}", var.name(), var.indices(), sign, val)
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

/// JS-facing planner handle.
#[wasm_bindgen]
pub struct WasmPlanner {
    inner: Arc<Mutex<PuzzlePlanner>>,
}

#[wasm_bindgen]
impl WasmPlanner {
    /// Build a planner from a puzzle. Errors if the SAT setup fails.
    #[wasm_bindgen(constructor)]
    pub fn new(puzzle: &WasmPuzzle) -> Result<WasmPlanner, JsError> {
        let solver = PuzzleSolver::new(puzzle.arc())
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
        to_value(&out).map_err(Into::into)
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
                    let lit_copy = *lit;
                    planner.mark_lit_as_deduced(&lit_copy);
                    return true;
                }
            }
        }

        let matching = {
            let mut found = None;
            for (puzlit, sat_lit) in planner.puzzle().direct.litmap.iter() {
                if format_puzlit(puzlit) == lit_str {
                    found = Some(*sat_lit);
                    break;
                }
            }
            found
        };

        if let Some(lit) = matching {
            planner.mark_lit_as_deduced(&lit);
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
        let puzvar = PuzVar::new(name, indices);
        let varval = VarValPair::new(&puzvar, value);
        let puzlit = if equal {
            PuzLit::new_eq(varval)
        } else {
            PuzLit::new_neq(varval)
        };
        if let Some(&sat_lit) = planner.puzzle().direct.litmap.get(&puzlit) {
            planner.mark_lit_as_deduced(&sat_lit);
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
        to_value(&payload).map_err(Into::into)
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
        to_value(&payload).map_err(Into::into)
    }

    /// Difficulty (smallest MUS size) for each provable literal.
    /// Returns an object keyed by literal string.
    pub fn difficulties(&self) -> Result<JsValue, JsError> {
        let mut planner = self.inner.lock().unwrap();
        let muses = planner.all_muses_with_larger();
        let mut out: BTreeMap<String, usize> = BTreeMap::new();
        for (lit, mus_set) in muses.muses() {
            let Some(min_len) = mus_set.iter().map(|mc| mc.mus_len()).min() else {
                continue;
            };
            let puzlits = planner.puzzle().lit_to_vars(lit);
            if let Some(first) = puzlits.iter().next() {
                out.insert(format_puzlit(first), min_len as usize);
            }
        }
        to_value(&out).map_err(Into::into)
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
        to_value(&out).map_err(Into::into)
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

        to_value(&Value::Object(root)).map_err(Into::into)
    }
}
