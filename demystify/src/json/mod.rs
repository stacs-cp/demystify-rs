/// This module contains the definitions and implementations related to JSON serialization and deserialization for the demystify library.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::Context;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::problem::{
    PuzLit, VarValPair,
    parse::{EdgeSide, LtAxis, PuzzleParse, ShowRole},
    solver::PuzzleSolver,
};

/// Convert a 2D clue matrix (outer = side entry, inner = padded-with-trailing-zeros clue slots)
/// into layered border labels. Non-zero values in each row are right-aligned across layers so
/// the last clue ends up on the layer nearest the grid. Empty leading layers are trimmed.
/// Returns an empty Vec if every row is all zeros.
fn clue_matrix_to_layers(clues: &[Vec<i64>]) -> Vec<Vec<String>> {
    let max_clues = clues
        .iter()
        .map(|row| row.iter().filter(|&&v| v > 0).count())
        .max()
        .unwrap_or(0);
    if max_clues == 0 {
        return Vec::new();
    }
    let mut layers: Vec<Vec<String>> = vec![vec![String::new(); clues.len()]; max_clues];
    for (r, row) in clues.iter().enumerate() {
        let non_zero: Vec<i64> = row.iter().copied().filter(|&v| v > 0).collect();
        let offset = max_clues - non_zero.len();
        for (k, v) in non_zero.into_iter().enumerate() {
            layers[offset + k][r] = v.to_string();
        }
    }
    layers
}

/// Read a `$#SHOW` name as a 2-D `Option<i64>` matrix, falling back to
/// known PuzLits when the name is a find/aux variable rather than a given
/// parameter.  Mystify uses this path: every `puz_*` matrix is a `find`
/// var whose values arrive at render-time via injected known lits, not via
/// a `letting` block.
fn read_2d_option_i64(
    pp: &PuzzleParse,
    known: &BTreeSet<PuzLit>,
    name: &str,
) -> anyhow::Result<Vec<Vec<Option<i64>>>> {
    if pp.eprime.has_param(name) {
        return pp.eprime.param_vec_vec_option_i64(name);
    }
    if !pp.eprime.vars.contains(name) && !pp.eprime.auxvars.contains(name) {
        anyhow::bail!("$#SHOW name '{name}' is neither a parameter nor a find/aux variable");
    }
    let dims = pp
        .get_matrix_indices(name)
        .with_context(|| format!("variable '{name}' has no shape information for rendering"))?;
    if dims.len() != 2 {
        anyhow::bail!(
            "$#SHOW '{name}': expected 2-D matrix, got {} dimensions",
            dims.len()
        );
    }
    let h = dims[0].max(0) as usize;
    let w = dims[1].max(0) as usize;
    let mut grid = vec![vec![None; w]; h];
    for lit in known {
        if !lit.sign() || lit.var().name() != name {
            continue;
        }
        let lit_var = lit.var();
        let idx = lit_var.indices();
        if idx.len() != 2 {
            continue;
        }
        let r = (idx[0] - 1) as usize;
        let c = (idx[1] - 1) as usize;
        if r < h && c < w {
            grid[r][c] = Some(lit.val());
        }
    }
    Ok(grid)
}

/// Read a `$#SHOW` name as a 2-D `i64` matrix.  Same dispatch as
/// [`read_2d_option_i64`] but cells without a known lit default to 0.
fn read_2d_i64(
    pp: &PuzzleParse,
    known: &BTreeSet<PuzLit>,
    name: &str,
) -> anyhow::Result<Vec<Vec<i64>>> {
    if pp.eprime.has_param(name) {
        return pp.eprime.param_vec_vec_i64(name);
    }
    let m = read_2d_option_i64(pp, known, name)?;
    Ok(m.into_iter()
        .map(|row| row.into_iter().map(|c| c.unwrap_or(0)).collect())
        .collect())
}

/// Read a `$#SHOW` name as a 1-D `i64` vector.
fn read_1d_i64(pp: &PuzzleParse, known: &BTreeSet<PuzLit>, name: &str) -> anyhow::Result<Vec<i64>> {
    if pp.eprime.has_param(name) {
        return pp.eprime.param_vec_i64(name);
    }
    if !pp.eprime.vars.contains(name) && !pp.eprime.auxvars.contains(name) {
        anyhow::bail!("$#SHOW name '{name}' is neither a parameter nor a find/aux variable");
    }
    let dims = pp
        .get_matrix_indices(name)
        .with_context(|| format!("variable '{name}' has no shape information for rendering"))?;
    if dims.len() != 1 {
        anyhow::bail!(
            "$#SHOW '{name}': expected 1-D vector, got {} dimensions",
            dims.len()
        );
    }
    let n = dims[0].max(0) as usize;
    let mut out = vec![0; n];
    for lit in known {
        if !lit.sign() || lit.var().name() != name {
            continue;
        }
        let lit_var = lit.var();
        let idx = lit_var.indices();
        if idx.len() != 1 {
            continue;
        }
        let i = (idx[0] - 1) as usize;
        if i < n {
            out[i] = lit.val();
        }
    }
    Ok(out)
}

/// Read a `$#SHOW` name as a scalar `i64`.
fn read_scalar_i64(pp: &PuzzleParse, known: &BTreeSet<PuzLit>, name: &str) -> anyhow::Result<i64> {
    if pp.eprime.has_param(name) {
        return pp.eprime.param_i64(name);
    }
    for lit in known {
        if lit.sign() && lit.var().name() == name && lit.var().indices().is_empty() {
            return Ok(lit.val());
        }
    }
    anyhow::bail!(
        "$#SHOW name '{name}': scalar has no value (not a parameter, and no \
         known equality literal found)"
    )
}

/// Read a 2-D `String` matrix.  Used for `side_labels`-shaped params.
fn read_2d_string(
    pp: &PuzzleParse,
    known: &BTreeSet<PuzLit>,
    name: &str,
) -> anyhow::Result<Vec<Vec<String>>> {
    if pp.eprime.has_param(name) {
        return pp.eprime.param_vec_vec_string(name);
    }
    let m = read_2d_i64(pp, known, name)?;
    Ok(m.into_iter()
        .map(|row| row.into_iter().map(|c| c.to_string()).collect())
        .collect())
}

/// Read an edge-labels parameter, returning the layered-labels representation
/// the renderer expects.  Accepts two shapes:
///
/// - a 1-D vector: produces a single layer of stringified entries.
/// - a 2-D matrix (e.g. nonogram clue runs with trailing-zero padding):
///   produces multiple layers via [`clue_matrix_to_layers`].
///
/// Returns `None` if the param is the zero-only matrix shape (all rows empty),
/// matching the legacy "no labels to render" behaviour.
fn read_edge_labels(
    pp: &PuzzleParse,
    known: &BTreeSet<PuzLit>,
    name: &str,
) -> anyhow::Result<Option<Vec<Vec<String>>>> {
    // Try matrix shape first (numeric clue grids).
    let dims = if pp.eprime.has_param(name) {
        // Probe the param shape: vec_vec_i64 errors out for 1-D params.
        pp.eprime.param_vec_vec_i64(name).ok().map(|_| 2)
    } else {
        pp.get_matrix_indices(name).map(|d| d.len())
    };
    if let Some(2) = dims {
        let m = read_2d_i64(pp, known, name)?;
        let layers = clue_matrix_to_layers(&m);
        return Ok(if layers.is_empty() {
            None
        } else {
            Some(layers)
        });
    }
    // 1-D fallback: vec_string for params (preserves any string entries),
    // stringified i64 for find vars.
    if pp.eprime.has_param(name) {
        return Ok(Some(vec![pp.eprime.param_vec_string(name)?]));
    }
    let v = read_1d_i64(pp, known, name)?;
    Ok(Some(vec![v.iter().map(|x| x.to_string()).collect()]))
}

/// One instantiated constraint from a `$#CON` class, with the grid cells it covers.
#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ConstraintInstance {
    /// Rendered human-readable description of this constraint instance.
    pub description: String,
    /// Grid cells (0-indexed [row, col]) that this constraint's scope covers.
    pub cells: Vec<[i64; 2]>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Puzzle {
    pub kind: String,
    pub width: i64,
    pub height: i64,
    pub start_grid: Option<Vec<Vec<Option<i64>>>>,
    pub solution_grid: Option<Vec<Vec<Option<i64>>>>,
    pub cages: Option<Vec<Vec<Option<i64>>>>,
    /// Border labels on each side. Outer vec is layer depth (0 = furthest from grid for
    /// top/left, 0 = nearest grid for bottom/right). Inner vec runs along the side
    /// (row index for top/bottom_labels, col index for left/right_labels). Depth-1 layer
    /// vectors cover the single-number case (Kakurasu, Skyscrapers). Multi-layer is used
    /// for Nonogram clue lists.
    pub top_labels: Option<Vec<Vec<String>>>,
    pub bottom_labels: Option<Vec<Vec<String>>>,
    pub left_labels: Option<Vec<Vec<String>>>,
    pub right_labels: Option<Vec<Vec<String>>>,
    /// Thermometer paths: each thermometer is an ordered list of [row, col] (0-indexed), bulb first.
    pub thermometers: Option<Vec<Vec<[i64; 2]>>>,
    /// Less-than constraints: each entry is [r1, c1, r2, c2] (0-indexed), meaning cell (r1,c1) < cell (r2,c2).
    pub less_than: Option<Vec<[i64; 4]>>,
    /// Cage sums indexed by cage ID (1-indexed): cage_sums[cage_id - 1] = target sum.
    pub cage_sums: Option<Vec<i64>>,
    /// Lines from `$#INFO` directives in the .eprime file.
    pub info: Option<Vec<String>>,
    /// All instantiated constraints grouped by their `$#CON` class name.
    /// Built once at puzzle-load time; templates rendered in parse.rs, not here.
    pub constraint_classes: Option<BTreeMap<String, Vec<ConstraintInstance>>>,
    /// SVG decoration flags from `$#DEC` directives (e.g. "sudoku_grid", "blank_input_val=2").
    #[serde(default)]
    pub decorations: Vec<String>,
}

impl Puzzle {
    /// Build a [`Puzzle`] from a parsed model with no known literals.  Use
    /// this when you don't have a [`PuzzleSolver`] handy (mostly tests and
    /// unit tests).  All `$#SHOW` directives whose name resolves to a
    /// `find` variable will produce empty matrices, since there's no known
    /// state to read values from.
    pub fn new_from_puzzle(problem: &PuzzleParse) -> anyhow::Result<Puzzle> {
        Self::new_from_puzzle_and_known(problem, &BTreeSet::new())
    }

    /// Build a [`Puzzle`] from a parsed model plus a set of known PuzLits.
    /// `known` is consulted whenever a `$#SHOW` directive names a `find`
    /// or `aux` variable rather than a `given` parameter — this is the
    /// path mystify uses to render `puz_*` clue values that are injected
    /// at runtime via [`PuzzleSolver::add_not_provable_known_lit`].
    pub fn new_from_puzzle_and_known(
        problem: &PuzzleParse,
        known: &BTreeSet<PuzLit>,
    ) -> anyhow::Result<Puzzle> {
        let kind = problem.eprime.kind.clone().unwrap_or("Unknown".to_string());

        let mut width = None;
        let mut height = None;

        for label in ["width", "x", "x_dim"] {
            if problem.eprime.has_param(label) {
                width = Some(problem.eprime.param_i64(label)?);
            }
        }

        for label in ["height", "y", "y_dim"] {
            if problem.eprime.has_param(label) {
                height = Some(problem.eprime.param_i64(label)?);
            }
        }

        if problem.eprime.has_param("grid_size") {
            height = Some(problem.eprime.param_i64("grid_size")?);
            width = Some(problem.eprime.param_i64("grid_size")?);
        }

        if problem.eprime.has_param("size") {
            height = Some(problem.eprime.param_i64("size")?);
            width = Some(problem.eprime.param_i64("size")?);
        }

        // If there is only one 'VAR', then it might tell us what to draw
        if height.is_none() && width.is_none() && problem.eprime.vars.len() == 1 {
            let var = problem.eprime.vars.iter().next().unwrap();

            let indices = problem.get_matrix_indices(var);
            if let Some(v) = indices
                && v.len() == 2
            {
                width = Some(v[1]);
                height = Some(v[0]);
            }
        }

        let mut start_grid = None;
        let mut cages = None;

        let mut top_labels = None;
        let mut bottom_labels = None;
        let mut left_labels = None;
        let mut right_labels = None;

        // Renderer roles are driven entirely by `$#SHOW` directives now;
        // there are no longer any magic-name fallbacks (`fixed`, `cages`,
        // `start_grid`, etc.).  See `ShowRole` in parse.rs.
        let show = &problem.eprime.show;
        let find_role = |pred: &dyn Fn(&ShowRole) -> bool| -> Option<&str> {
            show.iter().find(|d| pred(&d.role)).map(|d| d.var.as_str())
        };

        // Edge labels: vector of strings (or a 2-D clue matrix) per side.
        let edge_param = |side: EdgeSide| -> Option<String> {
            find_role(&|r| matches!(r, ShowRole::Edge { side: s } if *s == side))
                .map(str::to_string)
        };
        if let Some(p) = edge_param(EdgeSide::Top) {
            top_labels = read_edge_labels(problem, known, &p)?;
        }
        if let Some(p) = edge_param(EdgeSide::Left) {
            left_labels = read_edge_labels(problem, known, &p)?;
        }
        if let Some(p) = edge_param(EdgeSide::Bottom) {
            bottom_labels = read_edge_labels(problem, known, &p)?;
        }
        if let Some(p) = edge_param(EdgeSide::Right) {
            right_labels = read_edge_labels(problem, known, &p)?;
        }

        // Combined four-side labels: a [side, slot] matrix in left/top/right/
        // bottom order.  Overrides individual edge directives if present.
        if let Some(p) = find_role(&|r| matches!(r, ShowRole::SideLabels)) {
            let side_labels = read_2d_string(problem, known, p)?;
            left_labels = Some(vec![side_labels[0].clone()]);
            top_labels = Some(vec![side_labels[1].clone()]);
            right_labels = Some(vec![side_labels[2].clone()]);
            bottom_labels = Some(vec![side_labels[3].clone()]);
        }

        // Givens (pre-filled values shown as immutable in cells).
        if let Some(p) = find_role(&|r| matches!(r, ShowRole::Givens)) {
            start_grid = Some(read_2d_option_i64(problem, known, p)?);
        }

        // Cages (matrix of cage ids per cell).
        if let Some(p) = find_role(&|r| matches!(r, ShowRole::Cages)) {
            cages = Some(read_2d_option_i64(problem, known, p)?);
        }

        let mut thermometers = None;
        let mut less_than = None;
        let mut cage_sums = None;

        // Thermometer paths.  Decoder: therms_raw[col][row] = therm_id * step
        // + position.  The directive carries both the therms matrix name
        // and the name of the scalar `step` integer.
        if let Some(d) = show
            .iter()
            .find(|d| matches!(d.role, ShowRole::Thermometers { .. }))
        {
            let ShowRole::Thermometers { step: step_name } = &d.role else {
                unreachable!()
            };
            let step = read_scalar_i64(problem, known, step_name)?;
            let therms_raw = read_2d_i64(problem, known, &d.var)?;
            // therms_raw[col_0][row_0] (0-indexed), outer index = col, inner index = row.
            let mut therm_paths: BTreeMap<i64, BTreeMap<i64, [i64; 2]>> = BTreeMap::new();
            for (col_0, col_data) in therms_raw.iter().enumerate() {
                for (row_0, &val) in col_data.iter().enumerate() {
                    if val == 0 {
                        continue;
                    }
                    let therm_id = val / step;
                    let pos = val % step;
                    therm_paths
                        .entry(therm_id)
                        .or_default()
                        .insert(pos, [row_0 as i64, col_0 as i64]);
                }
            }
            let paths: Vec<Vec<[i64; 2]>> = therm_paths
                .into_values()
                .map(|path| path.into_values().collect())
                .collect();
            if !paths.is_empty() {
                thermometers = Some(paths);
            }
        }

        // Less-than relations: vector of [r1, c1, r2, c2] 1-indexed cell pairs.
        // Collect from both the tuple-list role (`less_than`) and the per-axis
        // sign-matrix role (`less_than_grid horizontal|vertical`).  Cells are
        // emitted as 0-indexed [r1, c1, r2, c2] where (r1,c1) is the smaller.
        let mut pairs: Vec<[i64; 4]> = Vec::new();
        if let Some(p) = find_role(&|r| matches!(r, ShowRole::LessThan)) {
            let lt_raw = read_2d_i64(problem, known, p)?;
            pairs.extend(
                lt_raw
                    .iter()
                    .map(|v| [v[0] - 1, v[1] - 1, v[2] - 1, v[3] - 1]),
            );
        }
        for d in show.iter() {
            let axis = match d.role {
                ShowRole::LessThanGrid { axis } => axis,
                _ => continue,
            };
            let m = read_2d_i64(problem, known, &d.var)?;
            for (r, row) in m.iter().enumerate() {
                for (c, &v) in row.iter().enumerate() {
                    let (r, c) = (r as i64, c as i64);
                    let pair = match (axis, v) {
                        (LtAxis::Horizontal, 1) => [r, c, r, c + 1],
                        (LtAxis::Horizontal, 2) => [r, c + 1, r, c],
                        (LtAxis::Vertical, 1) => [r, c, r + 1, c],
                        (LtAxis::Vertical, 2) => [r + 1, c, r, c],
                        (_, 0) => continue,
                        (_, other) => anyhow::bail!(
                            "$#SHOW {} less_than_grid {axis:?}: cell [{r},{c}] \
                             has unexpected value {other} (expected 0, 1, or 2)",
                            d.var
                        ),
                    };
                    pairs.push(pair);
                }
            }
        }
        if !pairs.is_empty() {
            less_than = Some(pairs);
        }

        // Cage sums: vector of integers indexed by cage id.
        if let Some(p) = find_role(&|r| matches!(r, ShowRole::CageSums)) {
            cage_sums = Some(read_1d_i64(problem, known, p)?);
        }

        if width.is_none() || height.is_none() {
            if let Some(sg) = &start_grid {
                width = Some(sg[0].len() as i64);
                height = Some(sg.len() as i64);
            } else if let Some(cg) = &cages {
                width = Some(cg[0].len() as i64);
                height = Some(cg.len() as i64);
            }
        }

        // $#INFO lines
        let info = if problem.eprime.info.is_empty() {
            None
        } else {
            Some(problem.eprime.info.clone())
        };

        // Build constraint_classes: group all instantiated constraint descriptions by $#CON class.
        // Templates were already rendered in parse.rs (conset); we just reorganise here.
        // Only keep instances whose scope includes at least one 2D grid cell (vacuous instances
        // — where Essence' guard failed — have no SAT connections and are not useful to show).
        // Cap each class at MAX_INSTANCES_PER_CLASS to keep the HTML payload manageable.
        const MAX_INSTANCES_PER_CLASS: usize = 50;
        let constraint_classes = if problem.constraints.is_empty() {
            None
        } else {
            let mut classes: BTreeMap<String, Vec<ConstraintInstance>> = BTreeMap::new();
            for (lit, description) in problem.constraints.iter() {
                if let Some(puzlits) = problem.direct.invlitmap.get(lit)
                    && let Some(puzlit) = puzlits.iter().find(|p| p.val() == 1)
                {
                    let class = puzlit.var().name().clone();
                    // Skip if this class is already at the cap.
                    let entries = classes.entry(class.clone()).or_default();
                    if entries.len() >= MAX_INSTANCES_PER_CLASS {
                        continue;
                    }
                    // Collect the unique grid cells this constraint covers (0-indexed).
                    let scope = problem.constraint_scope(description);
                    let cells: Vec<[i64; 2]> = scope
                        .iter()
                        .filter(|vvp| vvp.var().indices().len() == 2)
                        .map(|vvp| {
                            let idx = vvp.var().indices();
                            [idx[0] - 1, idx[1] - 1]
                        })
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    // Skip vacuous instances (guard failed → no SAT connections → no cells).
                    if cells.is_empty() {
                        continue;
                    }
                    entries.push(ConstraintInstance {
                        description: description.clone(),
                        cells,
                    });
                }
            }
            if classes.is_empty() {
                None
            } else {
                Some(classes)
            }
        };

        Ok(Puzzle {
            kind,
            width: width.context("'width' not given as a param, and unable to deduce")?,
            height: height.context("'height' not given as a param, and unable to deduce")?,
            start_grid,
            solution_grid: None,
            cages,
            top_labels,
            bottom_labels,
            left_labels,
            right_labels,
            thermometers,
            less_than,
            cage_sums,
            info,
            constraint_classes,
            decorations: problem.eprime.decs.clone(),
        })
    }
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct StateLit {
    pub val: i64,
    pub classes: Option<BTreeSet<String>>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct State {
    pub knowledge_grid: Option<Vec<Vec<Option<Vec<StateLit>>>>>,
    pub statements: Option<Vec<Statement>>,
    pub description: Option<String>,
    /// Cells (0-indexed [row, col]) that have no deducable literals at this step.
    /// Populated only in difficulty view to show non-deducable cells visually.
    #[serde(default)]
    pub blocked_cells: Option<Vec<[i64; 2]>>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Statement {
    pub content: String,
    pub classes: Vec<String>,
}

#[derive(Clone, PartialOrd, Ord, Hash, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Problem {
    pub puzzle: Puzzle,
    pub state: Option<State>,
}

pub struct DescriptionStatement {
    pub result: String,
    pub constraints: Vec<String>,
    /// Matched named-strategy display name, if any (e.g. "Row hidden single").
    pub name: Option<String>,
    /// Canonical-form `MusFingerprint` string. Always populated for MUS-derived
    /// statements; empty for non-MUS statements (e.g. initial state).
    pub fingerprint: Option<String>,
}

impl DescriptionStatement {
    pub fn new(result: String, constraints: Vec<String>) -> Self {
        Self {
            result,
            constraints,
            name: None,
            fingerprint: None,
        }
    }
}

impl Problem {
    pub fn new_from_puzzle(problem: &PuzzleParse) -> anyhow::Result<Problem> {
        let puzzle = Puzzle::new_from_puzzle(problem)?;
        Ok(Problem {
            puzzle,
            state: None,
        })
    }

    pub fn new_from_puzzle_and_state(
        solver: &PuzzleSolver,
        tosolve: &BTreeSet<VarValPair>,
        known: &BTreeSet<PuzLit>,
        deduced_lits: &BTreeSet<PuzLit>,
        comments: &str,
    ) -> anyhow::Result<Problem> {
        Self::new_from_puzzle_and_mus(solver, tosolve, known, deduced_lits, &[], comments)
    }

    pub fn new_from_puzzle_and_mus(
        solver: &PuzzleSolver,
        tosolve: &BTreeSet<VarValPair>,
        known: &BTreeSet<PuzLit>,
        deduced_lits: &BTreeSet<PuzLit>,
        deduction_list: &[DescriptionStatement],
        comments: &str,
    ) -> anyhow::Result<Problem> {
        let puzzle = Puzzle::new_from_puzzle_and_known(solver.puzzleparse(), known)?;

        let main_show = solver
            .puzzleparse()
            .eprime
            .show
            .iter()
            .find(|d| d.role == ShowRole::Main)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "model has no `$#SHOW <var> main` directive — required for rendering"
                )
            })?;
        let allowed_names: HashSet<String> = std::iter::once(main_show.var.clone()).collect();

        let mut knowledgegrid: Vec<Vec<Option<Vec<StateLit>>>> =
            vec![
                vec![None; usize::try_from(puzzle.width).context("width is negative")?];
                usize::try_from(puzzle.height).context("height is negative")?
            ];

        // Start by getting a list of all constraints, and assigning a number to each of them.
        let mut constraint_num: HashMap<String, usize> = HashMap::new();
        // Make a list of the tags we need to attach to each varvalpair in the scope of each constraint
        let mut constraint_tags: HashMap<VarValPair, BTreeSet<String>> = HashMap::new();

        for deduction in deduction_list {
            for constraint in &deduction.constraints {
                // constraint_num makes sure we only tag each constraint once
                if !constraint_num.contains_key(constraint) {
                    let len = constraint_num.len();
                    constraint_num.insert(constraint.clone(), len);
                    let scope = solver.puzzleparse().constraint_scope(constraint);
                    for p in scope {
                        let tags = constraint_tags.entry(p).or_default();
                        tags.insert(format!("highlight_con{len}"));
                        tags.insert("js_highlighter".to_string());
                    }
                }
            }
        }

        let all_lits = solver.puzzleparse().all_var_varvals();

        for l in all_lits {
            if !(tosolve.contains(&l) || known.contains(&PuzLit::new_eq(l.clone()))) {
                continue;
            }

            if !allowed_names.contains(l.var().name()) {
                continue;
            }

            // TODO: Handle more than one variable matrix?
            let index = l.var().indices().clone();
            assert_eq!(index.len(), 2);
            let i = usize::try_from(index[0]).context("negative index 0?")?;
            let j = usize::try_from(index[1]).context("negative index 1?")?;

            assert!(i > 0, "Variables should be 1-indexed");
            assert!(j > 0, "Variables should be 1-indexed");

            let i = i - 1;
            let j = j - 1;

            let mut tags = BTreeSet::new();

            if let Some(val) = constraint_tags.get(&l) {
                tags.extend(val.clone());
                tags.insert("litinmus".to_string());
            }

            if deduced_lits.contains(&PuzLit::new_eq(l.clone())) {
                tags.insert("litpos".to_string());
                tags.insert("highlight_".to_string() + &l.to_css_string());
                tags.insert("js_highlighter".to_string());
            }

            if deduced_lits.contains(&PuzLit::new_neq(l.clone())) {
                tags.insert("litneg".to_string());
                tags.insert("highlight_".to_string() + &l.to_css_string());
                tags.insert("js_highlighter".to_string());
            }

            if known.contains(&PuzLit::new_eq(l.clone())) {
                tags.insert("litknown".to_string());
            }

            if knowledgegrid[i][j].is_none() {
                knowledgegrid[i][j] = Some(vec![]);
            }

            knowledgegrid[i][j].as_mut().unwrap().push(StateLit {
                val: l.val(),
                classes: Some(tags),
            });
        }

        let mut statements = Vec::new();

        for deduction in deduction_list {
            // Bundle the technique name (if matched) into the same Statement
            // as the deduction text, so they form one visual block instead of
            // looking like sibling deductions in the flat statements list.
            // Fingerprint rendering is suppressed for now — the format is not
            // yet stable and the raw string confuses readers.
            let mut header = String::new();
            if let Some(name) = &deduction.name {
                header.push_str(&format!(
                    "<div class=\"technique-name\">{}</div>",
                    tera::escape_html(name)
                ));
            }
            statements.push(Statement {
                content: format!("{header}{}", deduction.result),
                classes: vec!["deduction".to_string()],
            });
            for constraint in &deduction.constraints {
                let num = constraint_num.get(constraint).unwrap();
                statements.push(Statement {
                    content: tera::escape_html(constraint),
                    classes: vec![
                        format!("highlight_con{}", num),
                        "js_highlighter".to_string(),
                    ],
                });
            }
        }

        let state = State {
            knowledge_grid: Some(knowledgegrid),
            statements: Some(statements),
            description: Some(comments.to_owned()),
            blocked_cells: None,
        };

        Ok(Problem {
            puzzle,
            state: Some(state),
        })
    }

    pub fn new_from_puzzle_and_difficulty(
        solver: &PuzzleSolver,
        tosolve: &BTreeSet<VarValPair>,
        known: &BTreeSet<PuzLit>,
        complexity: &BTreeMap<VarValPair, usize>,
        description: &str,
    ) -> anyhow::Result<Problem> {
        let puzzle = Puzzle::new_from_puzzle_and_known(solver.puzzleparse(), known)?;

        let main_show = solver
            .puzzleparse()
            .eprime
            .show
            .iter()
            .find(|d| d.role == ShowRole::Main)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "model has no `$#SHOW <var> main` directive — required for rendering"
                )
            })?;
        let allowed_names: HashSet<String> = std::iter::once(main_show.var.clone()).collect();

        let mut knowledgegrid: Vec<Vec<Option<Vec<StateLit>>>> =
            vec![
                vec![None; usize::try_from(puzzle.width).context("width is negative")?];
                usize::try_from(puzzle.height).context("height is negative")?
            ];

        let all_lits = solver.puzzleparse().all_var_varvals();

        let complexity_vals: BTreeSet<_> = complexity.values().collect();

        for l in all_lits {
            if !(tosolve.contains(&l) || known.contains(&PuzLit::new_eq(l.clone()))) {
                continue;
            }

            if !allowed_names.contains(l.var().name()) {
                continue;
            }

            // TODO: Handle more than one variable matrix?
            let index = l.var().indices().clone();
            assert_eq!(index.len(), 2);
            let i = usize::try_from(index[0]).context("negative index 0?")?;
            let j = usize::try_from(index[1]).context("negative index 1?")?;

            assert!(i > 0, "Variables should be 1-indexed");
            assert!(j > 0, "Variables should be 1-indexed");

            let i = i - 1;
            let j = j - 1;

            let mut tags = BTreeSet::new();

            if let Some(val) = complexity.get(&l) {
                let i = complexity_vals.iter().position(|&v| v == val).unwrap_or(0);
                tags.insert(format!("highlight_con{i}"));
                tags.insert("js_highlighter".to_string());
            }

            if known.contains(&PuzLit::new_eq(l.clone())) {
                tags.insert("litknown".to_string());
            }

            if knowledgegrid[i][j].is_none() {
                knowledgegrid[i][j] = Some(vec![]);
            }

            knowledgegrid[i][j].as_mut().unwrap().push(StateLit {
                val: l.val(),
                classes: Some(tags),
            });
        }

        let statements = complexity_vals
            .iter()
            .enumerate()
            .map(|(i, consize)| Statement {
                content: format!("MUS size {consize}"),
                classes: vec![format!("highlight_con{}", i), "js_highlighter".to_string()],
            })
            .collect_vec();

        // Collect cells that have no deducable or known literals — shown as blocked in SVG.
        let height = usize::try_from(puzzle.height).unwrap_or(0);
        let width = usize::try_from(puzzle.width).unwrap_or(0);
        let blocked: Vec<[i64; 2]> = (0..height)
            .flat_map(|r| (0..width).map(move |c| (r, c)))
            .filter(|&(r, c)| {
                knowledgegrid[r][c].is_none()
                    && puzzle
                        .start_grid
                        .as_ref()
                        .is_none_or(|sg| sg[r][c].is_none())
            })
            .map(|(r, c)| [r as i64, c as i64])
            .collect();
        let blocked_cells = if blocked.is_empty() {
            None
        } else {
            Some(blocked)
        };

        let state = State {
            knowledge_grid: Some(knowledgegrid),
            statements: Some(statements),
            description: Some(description.to_owned()),
            blocked_cells,
        };

        Ok(Problem {
            puzzle,
            state: Some(state),
        })
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;

    use crate::json::Puzzle;
    use crate::problem::util::test_utils::build_puzzleparse;

    #[test]
    fn givens_role_reads_from_known_puzlits_for_find_var() -> anyhow::Result<()> {
        use crate::problem::PuzLit;
        use crate::problem::PuzVar;
        use crate::problem::VarValPair;
        use crate::problem::parse::{ShowDirective, ShowRole};
        use std::collections::BTreeSet;

        // Mystify-style scenario: `grid` is a `find` matrix declared in the
        // model; values arrive at render-time via known PuzLits rather than
        // a `letting` block.  Re-target the existing $#SHOW directives so
        // that `grid` plays the `givens` role, then check the renderer
        // builds `start_grid` from the supplied known lits.
        let mut puz = build_puzzleparse("./tst/binairo.eprime", "./tst/binairo-1.param");
        puz.eprime.show = vec![
            ShowDirective {
                var: "grid".to_string(),
                role: ShowRole::Main,
            },
            ShowDirective {
                var: "grid".to_string(),
                role: ShowRole::Givens,
            },
        ];

        let mut known: BTreeSet<PuzLit> = BTreeSet::new();
        // Pin a single cell so we can verify the matrix is filled from `known`.
        known.insert(PuzLit::new_eq(VarValPair::new(
            &PuzVar::new("grid", vec![1, 1]),
            1,
        )));

        let p = Puzzle::new_from_puzzle_and_known(&puz, &known)?;
        let sg = p
            .start_grid
            .expect("givens role should populate start_grid");
        assert_eq!(sg[0][0], Some(1), "known grid[1,1]=1 should appear");
        // Cells without a known lit should remain None.
        assert_eq!(sg[0][1], None);
        Ok(())
    }

    #[test]
    fn test_parse_essence_binairo() -> anyhow::Result<()> {
        let puz = build_puzzleparse("./tst/binairo.eprime", "./tst/binairo-1.param");
        let p = Puzzle::new_from_puzzle(&puz)?;
        assert_eq!(p.kind, "Binairo");
        assert_eq!(p.width, 6);
        assert_eq!(p.height, 6);
        Ok(())
    }

    #[test]
    fn test_puzzle_dimensions_match_param() -> anyhow::Result<()> {
        // binairo with n=6 should produce a 6×6 puzzle.
        let puz = build_puzzleparse("./tst/binairo.eprime", "./tst/binairo-1.param");
        let p = Puzzle::new_from_puzzle(&puz)?;
        assert_eq!(p.width, 6, "binairo n=6 should give width=6");
        assert_eq!(p.height, 6, "binairo n=6 should give height=6");
        Ok(())
    }

    #[test]
    fn test_puzzle_start_grid_present() -> anyhow::Result<()> {
        // Binairo has a start_grid with some given values.
        let puz = build_puzzleparse("./tst/binairo.eprime", "./tst/binairo-1.param");
        let p = Puzzle::new_from_puzzle(&puz)?;
        assert!(p.start_grid.is_some());
        let sg = p.start_grid.unwrap();
        assert_eq!(sg.len() as i64, p.height);
        assert_eq!(sg[0].len() as i64, p.width);
        Ok(())
    }

    #[test]
    fn test_clue_matrix_to_layers_nonogram_5x5() {
        use crate::json::clue_matrix_to_layers;
        let row_clues = vec![
            vec![5, 0, 0],
            vec![1, 3, 0],
            vec![3, 1, 0],
            vec![2, 2, 0],
            vec![1, 1, 1],
        ];
        let layers = clue_matrix_to_layers(&row_clues);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec!["", "", "", "", "1"]);
        assert_eq!(layers[1], vec!["", "1", "3", "2", "1"]);
        assert_eq!(layers[2], vec!["5", "3", "1", "2", "1"]);
    }

    #[test]
    fn test_clue_matrix_to_layers_trims_empty_depth() {
        use crate::json::clue_matrix_to_layers;
        // maxruns=4 but no row uses more than 2 clues: depth should be 2, not 4.
        let clues = vec![vec![3, 0, 0, 0], vec![1, 2, 0, 0]];
        let layers = clue_matrix_to_layers(&clues);
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0], vec!["", "1"]);
        assert_eq!(layers[1], vec!["3", "2"]);
    }

    #[test]
    fn test_clue_matrix_to_layers_all_zeros() {
        use crate::json::clue_matrix_to_layers;
        let clues = vec![vec![0, 0], vec![0, 0]];
        assert!(clue_matrix_to_layers(&clues).is_empty());
    }

    #[test]
    fn test_puzzle_minesweeper_has_no_start_grid() -> anyhow::Result<()> {
        // Minesweeper has no pre-filled grid.
        let puz = build_puzzleparse("./tst/minesweeper.eprime", "./tst/minesweeperPrinted.param");
        let p = Puzzle::new_from_puzzle(&puz)?;
        // Minesweeper start_grid is None or all-None cells
        let all_empty = p.start_grid.as_ref().map_or(true, |sg| {
            sg.iter().all(|row| row.iter().all(|c| c.is_none()))
        });
        assert!(all_empty, "minesweeper should have no fixed start cells");
        Ok(())
    }
}
