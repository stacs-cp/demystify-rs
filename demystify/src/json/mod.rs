/// This module contains the definitions and implementations related to JSON serialization and deserialization for the demystify library.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::Context;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::problem::{PuzLit, VarValPair, parse::PuzzleParse, solver::PuzzleSolver};

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
    pub fn new_from_puzzle(problem: &PuzzleParse) -> anyhow::Result<Puzzle> {
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

        for label in ["row_labels", "top_labels", "row_sums"] {
            if problem.eprime.has_param(label) {
                top_labels = Some(vec![problem.eprime.param_vec_string(label)?]);
            }
        }

        for label in ["col_labels", "left_labels", "col_sums"] {
            if problem.eprime.has_param(label) {
                left_labels = Some(vec![problem.eprime.param_vec_string(label)?]);
            }
        }

        for label in ["bottom_labels"] {
            if problem.eprime.has_param(label) {
                bottom_labels = Some(vec![problem.eprime.param_vec_string(label)?]);
            }
        }

        for label in ["right_labels"] {
            if problem.eprime.has_param(label) {
                right_labels = Some(vec![problem.eprime.param_vec_string(label)?]);
            }
        }

        if problem.eprime.has_param("side_labels") {
            let side_labels = problem.eprime.param_vec_vec_string("side_labels")?;
            left_labels = Some(vec![side_labels[0].clone()]);
            top_labels = Some(vec![side_labels[1].clone()]);
            right_labels = Some(vec![side_labels[2].clone()]);
            bottom_labels = Some(vec![side_labels[3].clone()]);
        }

        // Nonogram clues: matrix indexed by [side_count, maxruns] of ints with trailing-zero
        // padding. Convert to layered labels, right-aligned so the last clue sits nearest
        // the grid and empty trailing layers are trimmed.
        if problem.eprime.has_param("row_clues") {
            let m = problem.eprime.param_vec_vec_i64("row_clues")?;
            let layers = clue_matrix_to_layers(&m);
            if !layers.is_empty() {
                top_labels = Some(layers);
            }
        }
        if problem.eprime.has_param("col_clues") {
            let m = problem.eprime.param_vec_vec_i64("col_clues")?;
            let layers = clue_matrix_to_layers(&m);
            if !layers.is_empty() {
                left_labels = Some(layers);
            }
        }

        if problem.eprime.has_param("start_grid") {
            start_grid = Some(problem.eprime.param_vec_vec_option_i64("start_grid")?);
        }

        if problem.eprime.has_param("fixed") {
            start_grid = Some(problem.eprime.param_vec_vec_option_i64("fixed")?);
        }

        if problem.eprime.has_param("cages") {
            cages = Some(problem.eprime.param_vec_vec_option_i64("cages")?);
        }

        // Killer Sudoku uses "grid" for cage IDs (not the solver variable) and "hints" for sums.
        if problem.eprime.has_param("grid")
            && !problem.eprime.vars.contains("grid")
            && problem.eprime.has_param("hints")
        {
            cages = Some(problem.eprime.param_vec_vec_option_i64("grid")?);
        }

        let mut thermometers = None;
        let mut less_than = None;
        let mut cage_sums = None;

        // Parse thermometer paths from therms[col][row] = therm_id * step + position.
        if problem.eprime.has_param("therms") && problem.eprime.has_param("step") {
            let step = problem.eprime.param_i64("step")?;
            let therms_raw = problem.eprime.param_vec_vec_i64("therms")?;
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

        // Parse less-than constraints: lt[i] = [r1, c1, r2, c2] (1-indexed).
        if problem.eprime.has_param("lt") {
            let lt_raw = problem.eprime.param_vec_vec_i64("lt")?;
            let pairs: Vec<[i64; 4]> = lt_raw
                .iter()
                .map(|v| [v[0] - 1, v[1] - 1, v[2] - 1, v[3] - 1])
                .collect();
            if !pairs.is_empty() {
                less_than = Some(pairs);
            }
        }

        // Parse cage sums from "hints" (used by Killer Sudoku).
        if problem.eprime.has_param("hints") {
            let hints = problem.eprime.param_vec_i64("hints")?;
            cage_sums = Some(hints);
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
        let constraint_classes = if problem.conset.is_empty() {
            None
        } else {
            let mut classes: BTreeMap<String, Vec<ConstraintInstance>> = BTreeMap::new();
            for (lit, description) in &problem.conset {
                // Recover the class name from invlitmap (Lit → PuzLit → PuzVar.name)
                if let Some(puzlits) = problem.invlitmap.get(lit)
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
}

impl DescriptionStatement {
    pub fn new(result: String, constraints: Vec<String>) -> Self {
        Self {
            result,
            constraints,
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
        let puzzle = Puzzle::new_from_puzzle(solver.puzzleparse())?;

        let varnames = tosolve
            .iter()
            .map(|x| x.var().name().clone())
            .chain(known.iter().map(|x| x.var().name().clone()))
            .collect::<HashSet<String>>();

        let allowed_names: HashSet<String> = if varnames.len() == 1 {
            varnames
        } else if varnames.contains("grid") {
            {
                let mut set = HashSet::new();
                set.insert("grid".to_string());
                set
            }
        } else {
            return Err(anyhow::anyhow!(
                "More than one variable matrix, and none called 'grid', so not sure what to print: {:?}",
                varnames
            ));
        };

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
            statements.push(Statement {
                content: deduction.result.clone(),
                classes: vec![],
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
        let puzzle = Puzzle::new_from_puzzle(solver.puzzleparse())?;

        let varnames = tosolve
            .iter()
            .map(|x| x.var().name().clone())
            .chain(known.iter().map(|x| x.var().name().clone()))
            .collect::<HashSet<String>>();

        let allowed_names: HashSet<String> = if varnames.len() == 1 {
            varnames
        } else if varnames.contains("grid") {
            {
                let mut set = HashSet::new();
                set.insert("grid".to_string());
                set
            }
        } else {
            return Err(anyhow::anyhow!(
                "More than one variable matrix, and none called 'grid', so not sure what to print: {:?}",
                varnames
            ));
        };

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
