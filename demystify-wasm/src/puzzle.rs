//! Puzzle wrapper exposed to JS via `wasm-bindgen`.
//!
//! Mirrors `demystify-lua/src/puzzle.rs`: load a pre-parsed puzzle JSON and
//! query its metadata, variables, constraints, and domains. Method names match
//! the Lua interface so the two can be cross-checked.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::Serialize;
use serde_wasm_bindgen::to_value;
use wasm_bindgen::prelude::*;

use demystify::problem::PuzVar;
use demystify::problem::parse::PuzzleParse;

/// Parse a variable string like `"grid[1, 2]"` or `"x"` into a `PuzVar`.
fn parse_var_string(s: &str) -> Option<PuzVar> {
    let s = s.trim();

    if let Some(bracket_pos) = s.find('[') {
        let name = s[..bracket_pos].trim();
        if name.is_empty() {
            return None;
        }
        let close_bracket = s.rfind(']')?;
        if close_bracket <= bracket_pos {
            return None;
        }
        let indices_str = &s[bracket_pos + 1..close_bracket];
        let mut indices = Vec::new();
        for part in indices_str.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let idx: i64 = part.parse().ok()?;
            indices.push(idx);
        }
        Some(PuzVar::new(name, indices))
    } else {
        if s.is_empty() || !s.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        Some(PuzVar::new(s, vec![]))
    }
}

/// Format a `PuzVar` as `"grid[1, 2]"` or `"x"`.
pub(crate) fn format_puzvar(var: &PuzVar) -> String {
    if var.indices().is_empty() {
        var.name().clone()
    } else {
        format!(
            "{}[{}]",
            var.name(),
            var.indices()
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// JS-facing puzzle handle. Wraps a parsed puzzle behind an `Arc` so it can
/// be cheaply shared with the planner.
#[wasm_bindgen]
pub struct WasmPuzzle {
    pub(crate) inner: Arc<PuzzleParse>,
}

impl WasmPuzzle {
    pub(crate) fn arc(&self) -> Arc<PuzzleParse> {
        self.inner.clone()
    }
}

/// Load a pre-parsed puzzle from a JSON string.
///
/// Use `demystify --model puzzle.eprime --param p.param --save-parsed p.json`
/// to produce the JSON, then pass its contents to this function.
#[wasm_bindgen]
pub fn load_puzzle(json: &str) -> Result<WasmPuzzle, JsError> {
    let inner = PuzzleParse::load_from_json_str(json)
        .map_err(|e| JsError::new(&format!("Failed to load puzzle: {e}")))?;
    Ok(WasmPuzzle {
        inner: Arc::new(inner),
    })
}

#[wasm_bindgen]
impl WasmPuzzle {
    pub fn kind(&self) -> Option<String> {
        self.inner.eprime.kind.clone()
    }

    pub fn info(&self) -> Result<JsValue, JsError> {
        let v: Vec<String> = self.inner.eprime.info.clone();
        to_value(&v).map_err(Into::into)
    }

    pub fn variables(&self) -> Result<JsValue, JsError> {
        let v: Vec<String> = self.inner.eprime.vars.iter().cloned().collect();
        to_value(&v).map_err(Into::into)
    }

    #[wasm_bindgen(js_name = auxVariables)]
    pub fn aux_variables(&self) -> Result<JsValue, JsError> {
        let v: Vec<String> = self.inner.eprime.auxvars.iter().cloned().collect();
        to_value(&v).map_err(Into::into)
    }

    pub fn constraints(&self) -> Result<JsValue, JsError> {
        let v: Vec<String> = self.inner.eprime.cons.keys().cloned().collect();
        to_value(&v).map_err(Into::into)
    }

    pub fn constraint(&self, name: &str) -> Option<String> {
        self.inner.eprime.cons.get(name).cloned()
    }

    #[wasm_bindgen(js_name = numClauses)]
    pub fn num_clauses(&self) -> usize {
        self.inner.cnf.as_ref().map(|c| c.len()).unwrap_or(0)
    }

    #[wasm_bindgen(js_name = numVarLits)]
    pub fn num_var_lits(&self) -> usize {
        self.inner.var_lits.positive().len()
    }

    #[wasm_bindgen(js_name = numConLits)]
    pub fn num_con_lits(&self) -> usize {
        self.inner.constraints.len()
    }

    #[wasm_bindgen(js_name = hasParam)]
    pub fn has_param(&self, name: &str) -> bool {
        self.inner.eprime.has_param(name)
    }

    #[wasm_bindgen(js_name = paramInt)]
    pub fn param_int(&self, name: &str) -> Result<i64, JsError> {
        self.inner
            .eprime
            .param_i64(name)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    #[wasm_bindgen(js_name = paramBool)]
    pub fn param_bool(&self, name: &str) -> Result<bool, JsError> {
        self.inner
            .eprime
            .param_bool(name)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Possible values for a single variable, looked up by string ("grid[1, 2]").
    /// Returns `null` if the variable isn't in the puzzle.
    #[wasm_bindgen(js_name = variableDomain)]
    pub fn variable_domain(&self, var_str: &str) -> Result<JsValue, JsError> {
        let puzvar = parse_var_string(var_str)
            .ok_or_else(|| JsError::new(&format!("Invalid variable format: {var_str}")))?;
        match self.inner.direct.domainmap.get(&puzvar) {
            Some(domain) => {
                let v: Vec<i64> = domain.iter().copied().collect();
                to_value(&v).map_err(Into::into)
            }
            None => Ok(JsValue::NULL),
        }
    }

    /// All variable domains as an object keyed by variable string.
    #[wasm_bindgen(js_name = allDomains)]
    pub fn all_domains(&self) -> Result<JsValue, JsError> {
        let map: std::collections::BTreeMap<String, Vec<i64>> = self
            .inner
            .direct
            .domainmap
            .iter()
            .map(|(v, d)| (format_puzvar(v), d.iter().copied().collect()))
            .collect();
        to_value(&map).map_err(Into::into)
    }

    /// Variables involved in a named constraint, as an array of strings.
    /// Returns `null` if the constraint isn't in the puzzle.
    #[wasm_bindgen(js_name = constraintVariables)]
    pub fn constraint_variables(&self, con_name: &str) -> Result<JsValue, JsError> {
        let con_name_owned = con_name.to_string();
        let known = self
            .inner
            .constraints
            .descriptions()
            .any(|n| *n == con_name_owned);
        if !known {
            return Ok(JsValue::NULL);
        }
        let con_lit = *self.inner.constraints.lit_for(&con_name_owned);
        let var_lits = self.inner.constraints.var_lits(&con_lit);

        let mut var_names: BTreeSet<String> = BTreeSet::new();
        for lit in var_lits {
            if let Some(puzlits) = self.inner.direct.invlitmap.get(lit) {
                for puzlit in puzlits {
                    var_names.insert(format_puzvar(&puzlit.var()));
                }
            }
        }
        let v: Vec<String> = var_names.into_iter().collect();
        to_value(&v).map_err(Into::into)
    }
}

/// Helpers exposed for parity with `demystify.helpers` in the Lua bindings.
#[derive(Serialize)]
struct ParsedLit {
    name: String,
    indices: Vec<i64>,
    value: i64,
    equal: bool,
}

#[wasm_bindgen(js_name = parseLiteral)]
pub fn parse_literal(s: &str) -> Result<JsValue, JsError> {
    let s = s.trim();
    let (lhs, rhs, equal) = if let Some(idx) = s.find("!=") {
        (&s[..idx], &s[idx + 2..], false)
    } else if let Some(idx) = s.find('=') {
        (&s[..idx], &s[idx + 1..], true)
    } else {
        return Err(JsError::new(&format!(
            "Not a literal (no '=' or '!='): {s}"
        )));
    };

    let var = parse_var_string(lhs)
        .ok_or_else(|| JsError::new(&format!("Invalid variable in literal: {s}")))?;
    let value: i64 = rhs
        .trim()
        .parse()
        .map_err(|_| JsError::new(&format!("Invalid value in literal: {s}")))?;

    let parsed = ParsedLit {
        name: var.name().clone(),
        indices: var.indices().clone(),
        value,
        equal,
    };
    to_value(&parsed).map_err(Into::into)
}
