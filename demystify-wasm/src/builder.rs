//! Builder bindings exposed to JS via `wasm-bindgen`.
//!
//! Mirrors `demystify-lua/src/builder.rs` so the two interfaces stay in sync.
//! See `demystify-builder` for the constraint vocabulary and design notes.
//!
//! # Quick start (JS / TypeScript)
//!
//! ```js
//! import init, { WasmBuilder } from "./demystify_wasm.js";
//! await init();
//!
//! const b = new WasmBuilder();
//! b.kind("toy");
//! const g = b.varBoolMatrix("g", [[1, 2], [1, 2]]);
//! const rule = b.conBool("rule");
//! const guard = b.guard(rule, "rule", "at least two cells are true");
//! b.sumGe(guard,
//!   [g.get([1, 1]).pos(), g.get([1, 2]).pos(),
//!    g.get([2, 1]).pos(), g.get([2, 2]).pos()],
//!   2);
//! const puzzle = b.build();
//! ```
//!
//! # Marshalling notes
//!
//! `WasmAtom`, `WasmSigned`, `WasmBoolMatrix`, and `WasmGuard` are
//! exposed as opaque handles.  Method calls that take `&self` leave the
//! JS handle valid for reuse; method calls that take `self` consume it.
//!
//! `WasmSigned` lists and `WasmGuard` are *consumed* when passed to a
//! sum_* call.  Build them inline:
//!
//! ```js
//! b.sumGe(b.guard(rule, "rule", "..."),  // fresh guard, consumed
//!   [a.pos(), b.neg()], 1);              // fresh signed handles
//! ```
//!
//! rather than holding the values in JS variables across multiple calls.
//!
//! # Multi-gate constraints (e.g. `$#REVEAL`)
//!
//! To express `gate1 ∧ gate2 ∧ ... → constraint`, fold the gates with
//! [`WasmBuilder::and_atom`] into a fresh atom and use that atom as the
//! `guard()` activation:
//!
//! ```js
//! const gate = b.andAtom([sumcheck.get([i, j]).pos(),
//!                         facts.get([i, j, 0]).pos()]);
//! const g = b.guard(gate, "sumcheck", "...");
//! b.sumEq(g, neighbours, n_mines);
//! ```

use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};

use wasm_bindgen::prelude::*;

use demystify_builder::{Atom, BoolMatrix, Guard, PuzzleBuilder, ShowRole, Signed};

use crate::puzzle::WasmPuzzle;

/// A single boolean atom in the puzzle being built. Opaque handle wrapping
/// a SAT literal — methods take `&self`, so a single atom can be used in
/// multiple `pos()`/`neg()` calls.
#[wasm_bindgen]
#[derive(Copy, Clone)]
pub struct WasmAtom {
    inner: Atom,
}

#[wasm_bindgen]
impl WasmAtom {
    /// The atom must be true.  Returns a fresh `Signed` handle.
    pub fn pos(&self) -> WasmSigned {
        WasmSigned {
            inner: self.inner.pos(),
        }
    }

    /// The atom must be false.  Returns a fresh `Signed` handle.
    pub fn neg(&self) -> WasmSigned {
        WasmSigned {
            inner: self.inner.neg(),
        }
    }
}

/// A signed atom for cardinality constraints. Produced by `atom.pos()` or
/// `atom.neg()`.  Consumed when read from a JS array.
#[wasm_bindgen]
#[derive(Copy, Clone)]
pub struct WasmSigned {
    pub(crate) inner: Signed,
}

#[wasm_bindgen]
impl WasmSigned {
    /// Independent copy of this signed handle.  Use if you want to pass
    /// the same `Signed` to more than one builder call.
    #[wasm_bindgen(js_name = clone)]
    pub fn js_clone(&self) -> WasmSigned {
        *self
    }
}

/// A matrix of boolean atoms declared via `var_bool_matrix` / `con_bool_matrix`
/// / `aux_bool_matrix`. Cheap to keep around; methods take `&self`.
#[wasm_bindgen]
pub struct WasmBoolMatrix {
    inner: BoolMatrix,
}

#[wasm_bindgen]
impl WasmBoolMatrix {
    /// The matrix's declared name.
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }

    /// The atom at the given (1-indexed) indices.
    pub fn get(&self, indices: Vec<i64>) -> Result<WasmAtom, JsError> {
        self.inner
            .try_get(&indices)
            .map(|a| WasmAtom { inner: a })
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Flat list of every atom in the matrix, row-major.  Returns an
    /// array of `WasmAtom` handles.
    pub fn atoms(&self) -> Vec<WasmAtom> {
        self.inner
            .atoms()
            .iter()
            .map(|a| WasmAtom { inner: *a })
            .collect()
    }
}

/// Single-use guard handle.  Produced by `WasmBuilder.guard(...)` and
/// consumed by the first `sumGe`/`sumLe`/`sumEq` it is passed to (the
/// JS-side handle is invalidated by wasm-bindgen on by-value transfer,
/// matching the "each $#CON description must be unique" invariant).
#[wasm_bindgen]
pub struct WasmGuard {
    pub(crate) inner: Guard,
}

/// JS-facing builder.  Wraps `Option<PuzzleBuilder>` so `build()` can
/// consume the inner builder without consuming the JS handle.
#[wasm_bindgen]
pub struct WasmBuilder {
    inner: Arc<Mutex<Option<PuzzleBuilder>>>,
}

impl Default for WasmBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmBuilder {
    fn with_mut<R, F: FnOnce(&mut PuzzleBuilder) -> Result<R, JsError>>(
        &self,
        f: F,
    ) -> Result<R, JsError> {
        let mut guard = self.inner.lock().unwrap();
        let b = guard
            .as_mut()
            .ok_or_else(|| JsError::new("Builder has already been consumed by build()"))?;
        f(b)
    }
}

#[wasm_bindgen]
impl WasmBuilder {
    /// Construct an empty builder.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmBuilder {
        WasmBuilder {
            inner: Arc::new(Mutex::new(Some(PuzzleBuilder::new()))),
        }
    }

    /// Set the `$#KIND` tag.
    pub fn kind(&self, kind: &str) -> Result<(), JsError> {
        self.with_mut(|b| {
            b.kind(kind.to_string());
            Ok(())
        })
    }

    /// Push a `$#INFO` string.
    pub fn info(&self, info: &str) -> Result<(), JsError> {
        self.with_mut(|b| {
            b.info(info.to_string());
            Ok(())
        })
    }

    /// Declare a `$#VAR` boolean matrix.
    #[wasm_bindgen(js_name = varBoolMatrix)]
    pub fn var_bool_matrix(&self, name: &str, dims: JsValue) -> Result<WasmBoolMatrix, JsError> {
        let dims = parse_dims(dims)?;
        self.with_mut(|b| {
            Ok(WasmBoolMatrix {
                inner: b.var_bool_matrix(name, &dims),
            })
        })
    }

    /// Declare a `$#CON` boolean matrix.
    #[wasm_bindgen(js_name = conBoolMatrix)]
    pub fn con_bool_matrix(&self, name: &str, dims: JsValue) -> Result<WasmBoolMatrix, JsError> {
        let dims = parse_dims(dims)?;
        self.with_mut(|b| {
            Ok(WasmBoolMatrix {
                inner: b.con_bool_matrix(name, &dims),
            })
        })
    }

    /// Declare a `$#AUX` boolean matrix.
    #[wasm_bindgen(js_name = auxBoolMatrix)]
    pub fn aux_bool_matrix(&self, name: &str, dims: JsValue) -> Result<WasmBoolMatrix, JsError> {
        let dims = parse_dims(dims)?;
        self.with_mut(|b| {
            Ok(WasmBoolMatrix {
                inner: b.aux_bool_matrix(name, &dims),
            })
        })
    }

    /// Declare a 0-d `$#CON` activation atom.
    #[wasm_bindgen(js_name = conBool)]
    pub fn con_bool(&self, name: &str) -> Result<WasmAtom, JsError> {
        self.with_mut(|b| {
            Ok(WasmAtom {
                inner: b.con_bool(name),
            })
        })
    }

    /// Declare a 0-d `$#AUX` atom.
    #[wasm_bindgen(js_name = auxBool)]
    pub fn aux_bool(&self, name: &str) -> Result<WasmAtom, JsError> {
        self.with_mut(|b| {
            Ok(WasmAtom {
                inner: b.aux_bool(name),
            })
        })
    }

    /// Declare a `$#REVEAL`-target boolean matrix.  Wire it up with
    /// [`Self::reveal`] to make it the reveal target for a `$#VAR`.
    #[wasm_bindgen(js_name = revealBoolMatrix)]
    pub fn reveal_bool_matrix(&self, name: &str, dims: JsValue) -> Result<WasmBoolMatrix, JsError> {
        let dims = parse_dims(dims)?;
        self.with_mut(|b| {
            Ok(WasmBoolMatrix {
                inner: b.reveal_bool_matrix(name, &dims),
            })
        })
    }

    /// Register `target` as the `$#REVEAL` target for `src`.  Both must
    /// be previously declared via [`Self::var_bool_matrix`] and
    /// [`Self::reveal_bool_matrix`] respectively.
    pub fn reveal(&self, src: &str, target: &str) -> Result<(), JsError> {
        self.with_mut(|b| {
            b.set_reveal(src, target)
                .map_err(|e| JsError::new(&e.to_string()))
        })
    }

    /// Register a `$#SHOW` directive.  `role` is a lowercase string
    /// (e.g. `"main"`, `"givens"`).
    pub fn show(&self, var: &str, role: &str) -> Result<(), JsError> {
        let role = parse_show_role(role)?;
        self.with_mut(|b| {
            b.show(var, role);
            Ok(())
        })
    }

    /// Create a fresh anonymous atom `c` such that `c ↔ AND(inputs)`.
    /// Pass the result to [`Self::guard`] (or use it in further sums) as
    /// needed.  Throws if `inputs` is empty.
    #[wasm_bindgen(js_name = andAtom)]
    pub fn and_atom(&self, inputs: Vec<WasmSigned>) -> Result<WasmAtom, JsError> {
        if inputs.is_empty() {
            return Err(JsError::new("andAtom requires at least one input"));
        }
        let inputs: Vec<Signed> = inputs.into_iter().map(|s| s.inner).collect();
        self.with_mut(|b| {
            Ok(WasmAtom {
                inner: b.and_atom(&inputs),
            })
        })
    }

    /// Pair an atom with the `$#CON` family it belongs to and a
    /// description.  Atoms returned by [`Self::con_bool`] /
    /// [`Self::con_bool_matrix`] are already labelled with their
    /// family; atoms returned by [`Self::and_atom`] (or other future
    /// fresh-atom primitives) are registered into the named family by
    /// this call.  Rejects atoms already in a *different* family.
    pub fn guard(
        &self,
        atom: &WasmAtom,
        family: &str,
        description: &str,
    ) -> Result<WasmGuard, JsError> {
        self.with_mut(|b| {
            b.guard(atom.inner, family, description.to_string())
                .map(|g| WasmGuard { inner: g })
                .map_err(|e| JsError::new(&e.to_string()))
        })
    }

    /// Post `guard.atom -> (sum(signed) >= k)` and register a `$#CON`
    /// entry using the family + description carried by `guard`.
    /// Consumes `guard`.
    #[wasm_bindgen(js_name = sumGe)]
    pub fn sum_ge(&self, guard: WasmGuard, signed: Vec<WasmSigned>, k: i64) -> Result<(), JsError> {
        let signed: Vec<Signed> = signed.into_iter().map(|s| s.inner).collect();
        self.with_mut(|b| {
            b.sum_ge(guard.inner, &signed, k)
                .map(|_| ())
                .map_err(|e| JsError::new(&e.to_string()))
        })
    }

    /// Post `guard.atom -> (sum(signed) <= k)`.  See [`Self::sum_ge`].
    #[wasm_bindgen(js_name = sumLe)]
    pub fn sum_le(&self, guard: WasmGuard, signed: Vec<WasmSigned>, k: i64) -> Result<(), JsError> {
        let signed: Vec<Signed> = signed.into_iter().map(|s| s.inner).collect();
        self.with_mut(|b| {
            b.sum_le(guard.inner, &signed, k)
                .map(|_| ())
                .map_err(|e| JsError::new(&e.to_string()))
        })
    }

    /// Post `guard.atom -> (sum(signed) == k)`.  See [`Self::sum_ge`].
    #[wasm_bindgen(js_name = sumEq)]
    pub fn sum_eq(&self, guard: WasmGuard, signed: Vec<WasmSigned>, k: i64) -> Result<(), JsError> {
        let signed: Vec<Signed> = signed.into_iter().map(|s| s.inner).collect();
        self.with_mut(|b| {
            b.sum_eq(guard.inner, &signed, k)
                .map(|_| ())
                .map_err(|e| JsError::new(&e.to_string()))
        })
    }

    /// Post `sum(signed) == k` unconditionally; no `$#CON` entry produced.
    #[wasm_bindgen(js_name = sumEqUnguarded)]
    pub fn sum_eq_unguarded(&self, signed: Vec<WasmSigned>, k: i64) -> Result<(), JsError> {
        let signed: Vec<Signed> = signed.into_iter().map(|s| s.inner).collect();
        self.with_mut(|b| {
            b.sum_eq_unguarded(&signed, k)
                .map_err(|e| JsError::new(&e.to_string()))
        })
    }

    /// Finalise into a `Puzzle`.  Consumes the builder; subsequent calls
    /// throw.
    pub fn build(&self) -> Result<WasmPuzzle, JsError> {
        let mut guard = self.inner.lock().unwrap();
        let b = guard
            .take()
            .ok_or_else(|| JsError::new("Builder already consumed"))?;
        let puzzle = b.build().map_err(|e| JsError::new(&e.to_string()))?;
        Ok(WasmPuzzle::from_parse(puzzle))
    }
}

fn parse_dims(dims: JsValue) -> Result<Vec<RangeInclusive<i64>>, JsError> {
    let outer: Vec<Vec<i64>> = serde_wasm_bindgen::from_value(dims).map_err(|e| {
        JsError::new(&format!(
            "dims must be an array of [first, last] pairs: {e}"
        ))
    })?;
    let mut out = Vec::with_capacity(outer.len());
    for pair in outer {
        if pair.len() != 2 {
            return Err(JsError::new(&format!(
                "each dim must be [first, last]; got {} values",
                pair.len()
            )));
        }
        let lo = pair[0];
        let hi = pair[1];
        if lo > hi {
            return Err(JsError::new(&format!(
                "matrix dimension {lo}..{hi}: first must be <= last"
            )));
        }
        out.push(lo..=hi);
    }
    Ok(out)
}

fn parse_show_role(s: &str) -> Result<ShowRole, JsError> {
    match s {
        "main" => Ok(ShowRole::Main),
        "cages" => Ok(ShowRole::Cages),
        "region_tint" => Ok(ShowRole::RegionTint),
        "givens" => Ok(ShowRole::Givens),
        "cage_sums" => Ok(ShowRole::CageSums),
        "less_than" => Ok(ShowRole::LessThan),
        "side_labels" => Ok(ShowRole::SideLabels),
        other => Err(JsError::new(&format!(
            "unknown show role '{other}' (supported: main, cages, region_tint, givens, cage_sums, less_than, side_labels)"
        ))),
    }
}
