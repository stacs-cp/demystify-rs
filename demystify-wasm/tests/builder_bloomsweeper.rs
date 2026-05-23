//! Wasm reproducer for the empty-`difficulties()` bug reported by the
//! bloomsweep-web embedder.  Builds the same 4×4 puzzle the browser
//! reproducer (`scripts/difficulties-repro/repro.js`) does, asks for
//! `difficulties()` before any deduction, and asserts the dict isn't
//! empty.
//!
//! The native equivalent
//! (`demystify-builder/tests/bloomsweeper_difficulties.rs`) passes —
//! `all_muses_with_larger` returns ~37 entries — so if this wasm test
//! fails it confirms the bug is wasm32-specific.

use serde_wasm_bindgen::{from_value, to_value};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;

use demystify_wasm::{WasmBuilder, WasmPlanner, WasmPuzzle};

const H: i64 = 4;
const W: i64 = 4;
const M: i64 = 2;

#[rustfmt::skip]
const HERBS: [[i64; 4]; 4] = [
    [1, 2, 1, 1],
    [2, 2, 1, 2],
    [1, 0, 2, 0],
    [2, 1, 1, 0],
];

#[rustfmt::skip]
const KNOWN: [[i64; 4]; 4] = [
    [0, 1, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
];

fn dims(d: &[(i64, i64)]) -> JsValue {
    let v: Vec<Vec<i64>> = d.iter().map(|&(a, b)| vec![a, b]).collect();
    to_value(&v).expect("encode dims")
}

fn build_puzzle() -> WasmPuzzle {
    let b = WasmBuilder::new();
    b.kind("bloomsweeper-tutorial-repro").expect("kind");

    let cell = b
        .var_bool_matrix("cell", dims(&[(0, H - 1), (0, W - 1), (0, M)]))
        .expect("cell var");
    b.show("cell", "main").expect("show");

    let facts = b
        .reveal_bool_matrix("facts", dims(&[(0, H - 1), (0, W - 1), (0, M), (0, 1)]))
        .expect("facts reveal");
    b.reveal("cell", "facts").expect("set_reveal");

    let sumcheck = b
        .con_bool_matrix("sumcheck", dims(&[(0, H - 1), (0, W - 1), (0, M)]))
        .expect("sumcheck $#CON");
    let rowcheck = b
        .con_bool_matrix("rowcheck", dims(&[(0, H - 1), (1, M)]))
        .expect("rowcheck $#CON");
    let colcheck = b
        .con_bool_matrix("colcheck", dims(&[(0, W - 1), (1, M)]))
        .expect("colcheck $#CON");

    for r in 0..H {
        for c in 0..W {
            let mut lits = Vec::new();
            for h in 0..=M {
                lits.push(cell.get(vec![r, c, h]).expect("cell get").pos());
            }
            b.sum_eq_unguarded(lits, 1).expect("one-hot");
        }
    }

    for r in 0..H {
        for c in 0..W {
            if KNOWN[r as usize][c as usize] == 1 {
                let h = HERBS[r as usize][c as usize];
                let lits = vec![cell.get(vec![r, c, h]).expect("known cell").pos()];
                b.sum_eq_unguarded(lits, 1).expect("pin known");
            }
        }
    }

    for r in 0..H {
        for c in 0..W {
            if HERBS[r as usize][c as usize] != 0 {
                continue;
            }
            let mut nbrs = Vec::new();
            for dr in -1..=1_i64 {
                for dc in -1..=1_i64 {
                    if dr == 0 && dc == 0 {
                        continue;
                    }
                    let (nr, nc) = (r + dr, c + dc);
                    if (0..H).contains(&nr) && (0..W).contains(&nc) {
                        nbrs.push((nr, nc));
                    }
                }
            }
            for h in 1..=M {
                let mut count = 0_i64;
                let mut atoms = Vec::new();
                for &(nr, nc) in &nbrs {
                    if HERBS[nr as usize][nc as usize] == h {
                        count += 1;
                    }
                    atoms.push(cell.get(vec![nr, nc, h]).expect("nbr").pos());
                }
                let g = b
                    .guard(
                        &sumcheck.get(vec![r, c, h]).expect("sumcheck atom"),
                        "sumcheck",
                        &format!("sumcheck[{r},{c}] herb={h}"),
                    )
                    .expect("guard")
                    .gated_by(&facts.get(vec![r, c, 0, 1]).expect("facts atom"));
                b.sum_eq(g, atoms, count).expect("sumcheck sum_eq");
            }
        }
    }

    for r in 0..H {
        for h in 1..=M {
            let mut count = 0_i64;
            let mut atoms = Vec::new();
            for c in 0..W {
                if HERBS[r as usize][c as usize] == h {
                    count += 1;
                }
                atoms.push(cell.get(vec![r, c, h]).expect("row cell").pos());
            }
            let g = b
                .guard(
                    &rowcheck.get(vec![r, h]).expect("rowcheck atom"),
                    "rowcheck",
                    &format!("rowcheck[{r}] herb={h}"),
                )
                .expect("rowcheck guard");
            b.sum_eq(g, atoms, count).expect("rowcheck sum_eq");
        }
    }

    for c in 0..W {
        for h in 1..=M {
            let mut count = 0_i64;
            let mut atoms = Vec::new();
            for r in 0..H {
                if HERBS[r as usize][c as usize] == h {
                    count += 1;
                }
                atoms.push(cell.get(vec![r, c, h]).expect("col cell").pos());
            }
            let g = b
                .guard(
                    &colcheck.get(vec![c, h]).expect("colcheck atom"),
                    "colcheck",
                    &format!("colcheck[{c}] herb={h}"),
                )
                .expect("colcheck guard");
            b.sum_eq(g, atoms, count).expect("colcheck sum_eq");
        }
    }

    b.build().expect("build puzzle")
}

#[wasm_bindgen_test]
fn difficulties_populated_on_bloomsweeper_tutorial() {
    let puzzle = build_puzzle();
    let planner = WasmPlanner::new(&puzzle).expect("planner");

    let provable = planner.num_provable();
    assert!(provable > 0, "puzzle should have provable lits");

    let diffs_js = planner.difficulties().expect("difficulties");

    // Regression check for the Map-vs-Object serialisation bug: every
    // JS-facing entry point that promises a "plain object keyed by ..."
    // shape must encode Rust maps as `Object`, not `Map`.  If the
    // serialiser slips back to its default (`Map`), `Object::keys`
    // silently returns an empty array even when the map is populated —
    // which is exactly the bug that made bloomsweep-web's
    // `difficulties()` look empty in the browser.
    let obj: js_sys::Object = diffs_js
        .clone()
        .dyn_into()
        .expect("difficulties() must return a plain JS object, not a Map");
    let keys = js_sys::Object::keys(&obj);

    let diffs: std::collections::BTreeMap<String, usize> =
        from_value(diffs_js).expect("decode difficulties");
    let step_raw = planner.best_step().expect("bestStep");
    let step_present = !step_raw.is_null();

    assert_eq!(
        keys.length() as usize,
        diffs.len(),
        "Object.keys(difficulties()) length must match the decoded map; \
         a mismatch means maps are leaking out as JS Map objects again."
    );

    assert!(
        !diffs.is_empty() || !step_present,
        "difficulties() returned empty but bestStep() found a step \
         (numProvable={provable}). Native control passes — bug is \
         wasm-specific."
    );

    // Contract check on the sentinel: difficulties() must cover every
    // distinct provable lit, even ones the MUS search couldn't size.
    // Un-MUSed lits get a sentinel equal to the puzzle's total
    // constraint count, which is a strict upper bound on any real MUS
    // size, so the sentinel always sorts above genuine difficulties.
    let provable_lits: Vec<String> = from_value(
        planner
            .provable_literals()
            .expect("provable_literals after difficulties"),
    )
    .expect("decode provable_literals");
    let provable_set: std::collections::BTreeSet<String> = provable_lits.into_iter().collect();
    let missing: Vec<&String> = provable_set
        .iter()
        .filter(|s| !diffs.contains_key(*s))
        .collect();
    assert!(
        missing.is_empty(),
        "every provable lit must appear in difficulties(); missing: {missing:?}"
    );
    let con_count = puzzle.num_con_lits();
    assert!(
        diffs.values().all(|&n| n <= con_count),
        "difficulties() should never exceed total constraint count (sentinel = {con_count})"
    );
}
