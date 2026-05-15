#![allow(clippy::ptr_arg)]
#![allow(clippy::needless_range_loop)]

use std::collections::BTreeSet;

use crate::json::StateLit;

use crate::json::{ConstraintShape, ConstraintShapeKind, Problem, Puzzle};
use itertools::Itertools;
use svg::Node;

use svg::node::element;

/// Individual SVG decoration flags that can be composed freely.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DecorationKind {
    /// Draw thicker grid lines at 3-cell intervals (Sudoku-style box boundaries).
    SudokuGrid,
    /// Treat this value in `start_grid` as "no clue" and skip rendering it.
    BlankInputVal(i64),
    /// Render `start_grid` clue values as small corner labels rather than large centered text,
    /// and allow domain candidates to be displayed alongside the clue in the same cell.
    /// Use for puzzles where the clue is metadata and the cell still has a deducable domain
    /// (e.g. Mosaic: clue = neighbour count, domain = mine/safe 0/1).
    ClueInCorner,
    /// Cells where `start_grid[r][c] < N` are rendered as walled/black cells: a thick inward
    /// dark border with a lighter interior. Clue numbers (value >= 0) are shown centered inside.
    /// Negative values (e.g. -1 = black unnumbered) get the wall visual but no text.
    WallBelow(i64),
}

/// A composable set of SVG decoration flags for a puzzle.
///
/// Built from:
/// 1. `$#DEC` annotations in the `.eprime` file (via `puzzle.decorations`)
/// 2. Kind-string defaults for backward compatibility
struct Decorations {
    flags: BTreeSet<DecorationKind>,
}

impl Decorations {
    /// Build from explicit decoration strings (from `$#DEC` directives) plus kind fallback.
    pub fn new(kind: &str, decs: &[String]) -> Decorations {
        let mut flags: BTreeSet<DecorationKind> = BTreeSet::new();

        // Explicit $#DEC annotations take full control when present.
        if !decs.is_empty() {
            for dec in decs {
                if dec == "sudoku_grid" {
                    flags.insert(DecorationKind::SudokuGrid);
                } else if let Some(val_str) = dec.strip_prefix("blank_input_val=") {
                    if let Ok(val) = val_str.parse::<i64>() {
                        flags.insert(DecorationKind::BlankInputVal(val));
                    }
                } else if dec == "clue_in_corner" {
                    flags.insert(DecorationKind::ClueInCorner);
                } else if let Some(val_str) = dec.strip_prefix("wall_below=")
                    && let Ok(val) = val_str.parse::<i64>()
                {
                    flags.insert(DecorationKind::WallBelow(val));
                }
            }
            return Decorations { flags };
        }

        // Fall back to kind-string defaults for puzzles without $#DEC annotations.
        let kind = kind.to_lowercase();
        if kind == "sudoku" || kind == "killer sudoku" || kind == "miracle" || kind == "x-sums" {
            flags.insert(DecorationKind::SudokuGrid);
            flags.insert(DecorationKind::BlankInputVal(0));
        } else if kind == "binairo" {
            flags.insert(DecorationKind::BlankInputVal(2));
        } else if kind == "mosaic" {
            flags.insert(DecorationKind::BlankInputVal(-1));
            flags.insert(DecorationKind::ClueInCorner);
        }

        Decorations { flags }
    }

    fn sudoku_grid(&self) -> bool {
        self.flags.contains(&DecorationKind::SudokuGrid)
    }

    fn blank_input_val(&self) -> Option<i64> {
        self.flags.iter().find_map(|f| {
            if let DecorationKind::BlankInputVal(v) = f {
                Some(*v)
            } else {
                None
            }
        })
    }

    fn clue_in_corner(&self) -> bool {
        self.flags.contains(&DecorationKind::ClueInCorner)
    }

    fn wall_below(&self) -> Option<i64> {
        self.flags.iter().find_map(|f| {
            if let DecorationKind::WallBelow(v) = f {
                Some(*v)
            } else {
                None
            }
        })
    }
}

pub struct PuzzleDraw {
    base_width: f64,
    mid_width: f64,
    thick_width: f64,
    decorations: Decorations,
}

impl Default for PuzzleDraw {
    fn default() -> Self {
        Self::new("")
    }
}

impl PuzzleDraw {
    #[must_use]
    pub fn new(kind: &str) -> Self {
        PuzzleDraw {
            base_width: 0.005,
            mid_width: 0.01,
            thick_width: 0.02,
            decorations: Decorations::new(kind, &[]),
        }
    }

    #[must_use]
    pub fn new_with_decs(kind: &str, decs: &[String]) -> Self {
        PuzzleDraw {
            base_width: 0.005,
            mid_width: 0.01,
            thick_width: 0.02,
            decorations: Decorations::new(kind, decs),
        }
    }
}

impl PuzzleDraw {
    #[must_use]
    pub fn draw_puzzle(&self, puzjson: &Problem) -> svg::Document {
        let puzzle = &puzjson.puzzle;

        let mut out = self.draw_grid(puzzle);

        let mut cells = self.make_cells(puzzle);
        let mut text_cells = self.make_text_cells(puzzle);

        if let Some(start_grid) = &puzzle.start_grid {
            self.fill_fixed_state(&mut cells, &mut text_cells, start_grid);
        }

        if let Some(state) = &puzjson.state
            && let Some(knowledge_grid) = &state.knowledge_grid
        {
            self.fill_knowledge(
                &mut cells,
                &mut text_cells,
                &puzzle.start_grid,
                knowledge_grid,
            );
        }

        if let Some(state) = &puzjson.state
            && let Some(blocked) = &state.blocked_cells
        {
            self.fill_blocked_cells(&mut cells, puzzle, blocked);
        }

        /*
            if ("solution_grid" in puzzle) {
              const solutionCpy = structuredClone(puzzle["solution_grid"]);
              if ("start_grid" in puzzle) {
                for (let i = 0; i < puzzle["start_grid"].length; i++) {
                  for (let j = 0; j < puzzle["start_grid"][i].length; j++) {
                    const cell = puzzle["start_grid"][i][j];
                    if (cell) {
                      solutionCpy[i][j] = null;
                    }
                  }
                }
              }
              this.fillFixedState(out, solutionCpy, { color: "grey" });
            }
        */

        self.set_cell_data_states(&mut cells, puzjson);

        // Thermometer overlays drawn before cells so grid lines appear on top.
        out.append(self.draw_thermometers(puzzle));

        let mut cellgrp = element::Group::new();

        for row in cells {
            for c in row {
                cellgrp.append(c);
            }
        }

        out.append(cellgrp);

        // Overlays drawn after cell backgrounds but BEFORE the text overlay,
        // so digits / candidate numbers always sit on top.
        out.append(self.draw_less_than(puzzle));
        out.append(self.draw_cage_sums(puzzle));
        if let Some(state) = &puzjson.state
            && let Some(shapes) = &state.constraint_shapes
        {
            out.append(self.draw_constraint_shapes(puzzle, shapes));
        }

        let mut textgrp = element::Group::new();
        textgrp.assign("class", "cell-text-overlays");
        for row in text_cells {
            for c in row {
                textgrp.append(c);
            }
        }
        out.append(textgrp);

        let out = self.fill_outside_labels(out, puzzle);

        let mut final_grp = element::Group::new();
        final_grp.assign("transform", "translate(50,50) scale(400)");
        final_grp.append(out);

        let doc = svg::Document::new()
            .set("viewBox", (0, 0, 500, 500))
            .set("width", 500)
            .set("height", 500)
            .set("id", "board")
            .set("class", "puzzle");
        doc.add(final_grp)
    }

    fn fill_outside_labels(&self, mut grid: element::Group, p: &Puzzle) -> element::Group {
        let mut label_group = element::Group::new();
        label_group.assign("class", "labels");

        let step = 1.0 / std::cmp::min(p.width, p.height) as f64;

        let mut puz_bounds = (0.0, step * (p.width as f64), 0.0, step * (p.height as f64));

        // Each side's labels are a Vec of layers. Closures map (total_layers L, layer index,
        // along-side index i) → (row, col) for make_cell, plus a per-layer bounds bump.
        // For top/left, layer 0 is furthest from the grid; for bottom/right, layer 0 is nearest.
        let label_groups = [
            &p.top_labels,
            &p.bottom_labels,
            &p.left_labels,
            &p.right_labels,
        ];

        #[allow(clippy::type_complexity)]
        let label_positions: Vec<(
            Box<dyn Fn(i64, i64, usize) -> i64>,
            Box<dyn Fn(i64, i64, usize) -> i64>,
            Box<dyn Fn(&mut (f64, f64, f64, f64))>,
        )> = vec![
            (
                Box::new(|_l, _layer, i| i as i64),
                Box::new(|l, layer, _i| -(l - layer)),
                Box::new(|bounds| bounds.0 -= step),
            ),
            (
                Box::new(|_l, _layer, i| i as i64),
                Box::new(|_l, layer, _i| p.height + layer),
                Box::new(|bounds| bounds.1 += step),
            ),
            (
                Box::new(|l, layer, _i| -(l - layer)),
                Box::new(|_l, _layer, i| i as i64),
                Box::new(|bounds| bounds.2 -= step),
            ),
            (
                Box::new(|_l, layer, _i| p.width + layer),
                Box::new(|_l, _layer, i| i as i64),
                Box::new(|bounds| bounds.3 += step),
            ),
        ];

        for (layers_opt, position) in label_groups.iter().zip(label_positions.iter()) {
            if let Some(layers) = layers_opt {
                let total_layers = layers.len() as i64;
                for (layer_idx, labels) in layers.iter().enumerate() {
                    position.2(&mut puz_bounds);
                    let layer = layer_idx as i64;
                    for (i, label) in labels.iter().enumerate() {
                        if label.is_empty() {
                            continue;
                        }
                        let mut node = svg::node::element::Text::new(label);
                        node.assign("font-size", 1);
                        node.assign("transform", "translate(0.2, 0.9)");
                        let mut g = make_cell(
                            position.0(total_layers, layer, i),
                            position.1(total_layers, layer, i),
                            step,
                        );
                        g.append(node);
                        label_group.append(g);
                    }
                }
            }
        }

        grid.append(label_group);

        let max_scale = f64::min(
            1.0 / (-puz_bounds.0 + puz_bounds.1),
            1.0 / (-puz_bounds.2 + puz_bounds.3),
        );

        let mut resized_grid = element::Group::new();
        resized_grid.assign(
            "transform",
            format!(
                "translate({},{}) scale({},{})",
                -puz_bounds.0, -puz_bounds.2, max_scale, max_scale
            ),
        );
        resized_grid.append(grid);

        resized_grid
    }

    fn fixed_cell_is_used(&self, cell: Option<i64>) -> bool {
        cell.is_some_and(|c| Some(c) != self.decorations.blank_input_val())
    }

    fn fill_fixed_state(
        &self,
        cells: &mut Vec<Vec<element::Group>>,
        text_cells: &mut Vec<Vec<element::Group>>,
        contents: &Vec<Vec<Option<i64>>>,
    ) {
        let wall_below = self.decorations.wall_below();

        for i in 0..contents.len() {
            for j in 0..contents[i].len() {
                let val = contents[i][j];

                // Wall cell: start_grid value < wall_below threshold → draw thick inward border.
                if let Some(wb) = wall_below
                    && val.is_some_and(|v| v < wb)
                {
                    // Dark outer rect fills the entire cell.
                    let mut outer = element::Rectangle::new();
                    outer.assign("width", 1);
                    outer.assign("height", 1);
                    outer.assign("fill", "#666666");
                    outer.assign("class", "wall-cell");
                    cells[i][j].append(outer);

                    // White inner rect — leaves a thick dark border around the edge.
                    let mut inner = element::Rectangle::new();
                    inner.assign("x", 0.1);
                    inner.assign("y", 0.1);
                    inner.assign("width", 0.8);
                    inner.assign("height", 0.8);
                    inner.assign("fill", "white");
                    cells[i][j].append(inner);

                    // Show clue number only for numbered cells (value >= 0).
                    // Negative values (e.g. -1) mean "black, no number".
                    if let Some(v) = val
                        && v >= 0
                    {
                        let mut node = svg::node::element::Text::new(v.to_string());
                        node.assign("font-size", 0.5);
                        node.assign("x", 0.5);
                        node.assign("y", 0.65);
                        node.assign("dominant-baseline", "middle");
                        node.assign("text-anchor", "middle");
                        text_cells[i][j].append(node);
                    }
                    continue;
                }

                // Normal fixed cell rendering.
                if self.fixed_cell_is_used(val) {
                    let cell = val.unwrap();
                    let s = cell.to_string();

                    let mut node = svg::node::element::Text::new(s);
                    if self.decorations.clue_in_corner() {
                        // Small corner label — leaves most of the cell free for domain candidates.
                        node.assign("font-size", 0.35);
                        node.assign("x", 0.05);
                        node.assign("y", 0.38);
                    } else {
                        node.assign("font-size", 1);
                        node.assign("transform", "translate(0.2, 0.9)");
                    }

                    text_cells[i][j].append(node);
                }
            }
        }
    }

    fn fill_knowledge(
        &self,
        cells: &mut Vec<Vec<element::Group>>,
        text_cells: &mut Vec<Vec<element::Group>>,
        fixed_contents: &Option<Vec<Vec<Option<i64>>>>,
        contents: &Vec<Vec<Option<Vec<StateLit>>>>,
    ) {
        for i in 0..contents.len() {
            for j in 0..contents[i].len() {
                // Skip cells that already have a fixed (given) value — unless clue_in_corner
                // is active, in which case the clue is shown small in the corner and the
                // domain candidates are still rendered in the main cell area.
                if !self.decorations.clue_in_corner()
                    && fixed_contents
                        .as_ref()
                        .is_some_and(|c| self.fixed_cell_is_used(c[i][j]))
                {
                    continue;
                }

                if let Some(cell) = &contents[i][j] {
                    // Find the right size of grid to fit our values in
                    let sqrt_length = (cell.len() as f64).sqrt().ceil() as usize;
                    let little_step = 0.9 / sqrt_length as f64;
                    for a in 0..sqrt_length {
                        for b in 0..sqrt_length {
                            if a * sqrt_length + b < cell.len() {
                                let state = &cell[a * sqrt_length + b];
                                let s = state.val.to_string();
                                let transform = format!(
                                    "translate({}, {})",
                                    0.05 + (b as f64 * little_step),
                                    0.05 + (a as f64 + 1.0) * little_step
                                );

                                let id = format!(
                                    "D_{}_{}_{}",
                                    i + 1,
                                    j + 1,
                                    cell[a * sqrt_length + b].val
                                );

                                let mut classes = vec!["literal".to_owned()];
                                if let Some(extra_classes) = &state.classes {
                                    classes.extend(extra_classes.iter().cloned());
                                }
                                let class_str = classes.iter().join(" ");

                                // Background group — interactive: holds the litbox rect that
                                // turns red on hover, plus the id/classes the JS hover layer
                                // uses to find peers.
                                let mut bg_group = svg::node::element::Group::new();
                                bg_group.assign("transform", transform.clone());

                                let mut rect = svg::node::element::Rectangle::new();
                                rect.assign("width", little_step);
                                rect.assign("height", little_step);
                                rect.assign("y", -little_step);
                                rect.assign("class", "litbox");
                                bg_group.append(rect);

                                bg_group.assign("id", id.clone());
                                bg_group.assign("name", id);
                                bg_group.assign("data-cand", state.val.to_string());
                                bg_group.assign("class", class_str.clone());
                                cells[i][j].append(bg_group);

                                // Text group — non-interactive (pointer-events inherits the
                                // text_cells default of `none`), but keeps the same visual
                                // classes so styles like `.litneg text { line-through }` work.
                                let mut text_group = svg::node::element::Group::new();
                                text_group.assign("transform", transform);
                                let mut node = svg::node::element::Text::new(s);
                                node.assign("font-size", little_step);
                                node.assign("x", little_step / 2.0);
                                node.assign("y", -little_step / 3.0);
                                node.assign("dominant-baseline", "middle");
                                node.assign("text-anchor", "middle");
                                text_group.append(node);
                                text_group.assign("class", class_str);
                                text_cells[i][j].append(text_group);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Mark cells that have no deducable literals with a grey background and a "?" indicator.
    /// Mark cells that have no deducable literals with a grey background and a "?" indicator.
    /// The cell coordinate space is 0..1 (established by `make_cell`'s transform).
    fn fill_blocked_cells(
        &self,
        cells: &mut Vec<Vec<element::Group>>,
        _puzzle: &Puzzle,
        blocked: &[[i64; 2]],
    ) {
        for &[r, c] in blocked {
            let i = r as usize;
            let j = c as usize;
            if i >= cells.len() || j >= cells[i].len() {
                continue;
            }

            // Grey background covering the cell (0..1 coordinate space).
            let mut bg = element::Rectangle::new();
            bg.assign("width", 1);
            bg.assign("height", 1);
            bg.assign("fill", "#cccccc");
            bg.assign("opacity", 0.5);
            cells[i][j].append(bg);

            // "?" text centered in the cell.
            let mut text = svg::node::element::Text::new("?");
            text.assign("font-size", 0.7);
            text.assign("x", 0.5);
            text.assign("y", 0.75);
            text.assign("dominant-baseline", "middle");
            text.assign("text-anchor", "middle");
            text.assign("fill", "#888888");
            text.assign("class", "litundeducable");
            cells[i][j].append(text);
        }
    }

    fn set_cell_data_states(&self, cells: &mut [Vec<element::Group>], problem: &Problem) {
        let puzzle = &problem.puzzle;
        for i in 0..cells.len() {
            for j in 0..cells[i].len() {
                let has_fixed = puzzle.start_grid.as_ref().is_some_and(|sg| {
                    i < sg.len() && j < sg[i].len() && self.fixed_cell_is_used(sg[i][j])
                });

                let knowledge = problem
                    .state
                    .as_ref()
                    .and_then(|s| s.knowledge_grid.as_ref())
                    .and_then(|kg| kg.get(i))
                    .and_then(|row| row.get(j))
                    .and_then(|cell| cell.as_ref());

                if let Some(cell_vals) = knowledge {
                    let is_single_known = cell_vals.len() == 1
                        && cell_vals[0]
                            .classes
                            .as_ref()
                            .is_some_and(|c| c.contains("litknown") || c.contains("litpos"));
                    if is_single_known {
                        cells[i][j].assign("data-state", "solved");
                    } else {
                        cells[i][j].assign("data-state", "candidates");
                    }
                } else if has_fixed && !self.decorations.clue_in_corner() {
                    cells[i][j].assign("data-state", "given");
                }
            }
        }
    }

    fn draw_grid(&self, puzzle: &Puzzle) -> element::Group {
        let mut topgrp = element::Group::new();

        let mut grp = element::Group::new();

        let width = usize::try_from(puzzle.width).expect("negative width?");
        let height = usize::try_from(puzzle.height).expect("negative height?");
        let cages = &puzzle.cages;

        let step = 1.0 / std::cmp::min(width, height) as f64;

        let colours_list = [
            "#85586f", "#d6efed", "#957dad", "#ac7d88", "#b7d3df", "#e0bbe4", "#deb6ab", "#c9bbcf",
            "#fec8d8", "#f8ecd1", "#898aa6", "#ffdfd3", "#c4dfaa", "#f5f0bb", "#e6e1cd", "#d6b1dd",
        ];

        let mut cagegrp = element::Group::new();

        if let Some(cages) = &cages {
            let colours: BTreeSet<_> = cages.iter().flatten().filter_map(|cell| *cell).collect();

            for i in 0..width {
                for j in 0..height {
                    if let Some(cell) = cages[j][i] {
                        let col = colours.iter().position(|&c| c == cell).unwrap();
                        let i_f = i as f64;
                        let j_f = j as f64;
                        let path = format!(
                            "M {} {} H {} V {} H {} Z",
                            step * i_f,
                            step * j_f,
                            step * (i_f + 1.0),
                            step * (j_f + 1.0),
                            step * i_f
                        );

                        let mut p = element::Path::new();
                        p.assign("d", path);
                        p.assign("fill", colours_list[col]);
                        cagegrp.append(p);
                    }
                }
            }
        }

        // Colour-region tints (the `region_tint` $#SHOW role): fill each
        // coloured cell with a palette colour keyed on the colour id.  No
        // borders — the regions need not be contiguous, so cell tint is the
        // only cue.  Uncoloured cells (id 0) were already mapped to `None`.
        if let Some(tints) = &puzzle.region_tint {
            for i in 0..width {
                for j in 0..height {
                    let Some(k) = tints[j][i] else { continue };
                    let col = ((k - 1).max(0) as usize) % colours_list.len();
                    let i_f = i as f64;
                    let j_f = j as f64;
                    let path = format!(
                        "M {} {} H {} V {} H {} Z",
                        step * i_f,
                        step * j_f,
                        step * (i_f + 1.0),
                        step * (j_f + 1.0),
                        step * i_f
                    );
                    let mut p = element::Path::new();
                    p.assign("d", path);
                    p.assign("fill", colours_list[col]);
                    cagegrp.append(p);
                }
            }
        }

        grp.append(cagegrp);

        let mut outlinegrp = element::Group::new();

        for i in 0..=width {
            for j in 0..height {
                let mut stroke = self.base_width;
                if i == 0 || i == width {
                    stroke = self.thick_width;
                } else {
                    if self.decorations.sudoku_grid() && i % 3 == 0 {
                        stroke = self.mid_width;
                    }
                    if let Some(cages) = cages
                        && cages[j][i] != cages[j][i - 1]
                    {
                        stroke = self.thick_width;
                    }
                }
                let i_f = i as f64;
                let j_f = j as f64;

                let path = format!(
                    "M {} {} L {} {}",
                    step * i_f,
                    step * j_f,
                    step * i_f,
                    step * (j_f + 1.0)
                );
                let mut p = element::Path::new();
                p.assign("d", path);
                p.assign("stroke", "black");
                p.assign("stroke-width", stroke);
                p.assign("stroke-linecap", "round");
                outlinegrp = outlinegrp.add(p);
            }
        }

        for i in 0..width {
            for j in 0..=height {
                let mut stroke = self.base_width;
                if j == 0 || j == height {
                    stroke = self.thick_width;
                } else {
                    if self.decorations.sudoku_grid() && j % 3 == 0 {
                        stroke = self.mid_width;
                    }
                    if let Some(cages) = cages
                        && cages[j][i] != cages[j - 1][i]
                    {
                        stroke = self.thick_width;
                    }
                }
                let i_f = i as f64;
                let j_f = j as f64;

                let path = format!(
                    "M {} {} L {} {}",
                    step * i_f,
                    step * j_f,
                    step * (i_f + 1.0),
                    step * j_f
                );
                let mut p = element::Path::new();
                p.assign("d", path);
                p.assign("stroke", "black");
                p.assign("stroke-width", stroke);
                p.assign("stroke-linecap", "round");
                outlinegrp.append(p);
            }
        }

        grp.append(outlinegrp);

        topgrp.append(grp);
        topgrp
    }

    /// Draw thermometer paths (bulb circle + tube line) as SVG overlays.
    fn draw_thermometers(&self, puzzle: &Puzzle) -> element::Group {
        let mut grp = element::Group::new();
        let therms = match &puzzle.thermometers {
            Some(t) => t,
            None => return grp,
        };

        let step = 1.0 / std::cmp::min(puzzle.width, puzzle.height) as f64;
        let radius = step * 0.38;
        let tube_width = step * 0.5;
        let outline_extra = step * 0.06;
        let fill_color = "#d8d8d8";
        let outline_color = "#888888";

        // Two-pass rendering: outlines first, then fills, so fills appear on top.
        let mut outlines = element::Group::new();
        let mut fills = element::Group::new();

        for therm in therms {
            if therm.is_empty() {
                continue;
            }

            let points: String = therm
                .iter()
                .map(|[r, c]| format!("{},{}", step * (*c as f64 + 0.5), step * (*r as f64 + 0.5)))
                .collect::<Vec<_>>()
                .join(" ");

            if therm.len() > 1 {
                let mut poly_outline = element::Polyline::new();
                poly_outline.assign("points", points.clone());
                poly_outline.assign("fill", "none");
                poly_outline.assign("stroke", outline_color);
                poly_outline.assign("stroke-width", tube_width + outline_extra);
                poly_outline.assign("stroke-linecap", "round");
                poly_outline.assign("stroke-linejoin", "round");
                outlines.append(poly_outline);

                let mut poly_fill = element::Polyline::new();
                poly_fill.assign("points", points);
                poly_fill.assign("fill", "none");
                poly_fill.assign("stroke", fill_color);
                poly_fill.assign("stroke-width", tube_width);
                poly_fill.assign("stroke-linecap", "round");
                poly_fill.assign("stroke-linejoin", "round");
                fills.append(poly_fill);
            }

            // Bulb circle at the first cell.
            let [br, bc] = therm[0];
            let cx = step * (bc as f64 + 0.5);
            let cy = step * (br as f64 + 0.5);

            let mut bulb_outline = element::Circle::new();
            bulb_outline.assign("cx", cx);
            bulb_outline.assign("cy", cy);
            bulb_outline.assign("r", radius + outline_extra / 2.0);
            bulb_outline.assign("fill", outline_color);
            outlines.append(bulb_outline);

            let mut bulb_fill = element::Circle::new();
            bulb_fill.assign("cx", cx);
            bulb_fill.assign("cy", cy);
            bulb_fill.assign("r", radius);
            bulb_fill.assign("fill", fill_color);
            fills.append(bulb_fill);
        }

        grp.append(outlines);
        grp.append(fills);
        grp
    }

    /// Draw less-than chevrons between adjacent cells with inequality constraints.
    ///
    /// Each chevron is two SVG line segments forming a ‹/›/∧/∨ shape.
    /// The tip points toward the smaller cell (r1,c1).
    fn draw_less_than(&self, puzzle: &Puzzle) -> element::Group {
        let mut grp = element::Group::new();
        let pairs = match &puzzle.less_than {
            Some(p) => p,
            None => return grp,
        };

        let step = 1.0 / std::cmp::min(puzzle.width, puzzle.height) as f64;
        let s = step * 0.14; // half-arm length
        let stroke_w = step * 0.03;

        for &[r1, c1, r2, c2] in pairs {
            // Only render for directly adjacent cells.
            if (r2 - r1).abs() + (c2 - c1).abs() != 1 {
                continue;
            }

            // Centre of the symbol sits on the shared cell boundary.
            // Chevron: two lines (ax,ay)→(mx,my) and (mx,my)→(bx,by).
            // (mx,my) is the tip, pointing toward the smaller cell (r1,c1).
            let (ax, ay, mx, my, bx, by);

            if r1 == r2 {
                // Horizontal neighbours.
                let cx = step * (c1.min(c2) as f64 + 1.0);
                let cy = step * (r1 as f64 + 0.5);
                if c1 < c2 {
                    // smaller on left → tip points left  (<)
                    ax = cx + s;
                    ay = cy - s;
                    mx = cx - s;
                    my = cy;
                    bx = cx + s;
                    by = cy + s;
                } else {
                    // smaller on right → tip points right  (>)
                    ax = cx - s;
                    ay = cy - s;
                    mx = cx + s;
                    my = cy;
                    bx = cx - s;
                    by = cy + s;
                }
            } else {
                // Vertical neighbours.
                let cx = step * (c1 as f64 + 0.5);
                let cy = step * (r1.min(r2) as f64 + 1.0);
                if r1 < r2 {
                    // smaller on top → tip points up  (^)
                    ax = cx - s;
                    ay = cy + s;
                    mx = cx;
                    my = cy - s;
                    bx = cx + s;
                    by = cy + s;
                } else {
                    // smaller on bottom → tip points down  (v)
                    ax = cx - s;
                    ay = cy - s;
                    mx = cx;
                    my = cy + s;
                    bx = cx + s;
                    by = cy - s;
                }
            }

            for (x1, y1, x2, y2) in [(ax, ay, mx, my), (mx, my, bx, by)] {
                let mut line = element::Line::new();
                line.assign("x1", x1);
                line.assign("y1", y1);
                line.assign("x2", x2);
                line.assign("y2", y2);
                line.assign("stroke", "#444444");
                line.assign("stroke-width", stroke_w);
                line.assign("stroke-linecap", "round");
                grp.append(line);
            }
        }

        grp
    }

    /// Draw a per-MUS-constraint visual indicator on the grid.
    /// All colour, dash pattern, and stroke width are CSS-controlled — the
    /// renderer only emits structural classes (`constraint-shape`,
    /// the kind subclass, and the `highlight_conN` class that already drives
    /// the cell tint).
    fn draw_constraint_shapes(
        &self,
        puzzle: &Puzzle,
        shapes: &[ConstraintShape],
    ) -> element::Group {
        let mut grp = element::Group::new();
        grp.assign("class", "constraint-shapes");
        let step = 1.0 / std::cmp::min(puzzle.width, puzzle.height) as f64;
        let stagger_unit = step * 0.04;

        for shape in shapes {
            let class = format!(
                "constraint-shape {} highlight_con{}",
                match shape.kind {
                    ConstraintShapeKind::Row => "row",
                    ConstraintShapeKind::Col => "col",
                    ConstraintShapeKind::Pair => "pair",
                    ConstraintShapeKind::Region => "region",
                },
                shape.idx
            );
            match shape.kind {
                ConstraintShapeKind::Row => {
                    if shape.cells.len() < 2 {
                        continue;
                    }
                    let row = shape.cells[0][0] as f64;
                    let cols: Vec<i64> = shape.cells.iter().map(|c| c[1]).collect();
                    let c0 = (*cols.iter().min().unwrap()) as f64;
                    let c1 = (*cols.iter().max().unwrap()) as f64;
                    let y = step * (row + 0.5) + stagger_unit * shape.stagger as f64;
                    let mut line = element::Line::new();
                    line.assign("x1", step * (c0 + 0.5));
                    line.assign("y1", y);
                    line.assign("x2", step * (c1 + 0.5));
                    line.assign("y2", y);
                    line.assign("class", class);
                    grp.append(line);
                }
                ConstraintShapeKind::Col => {
                    if shape.cells.len() < 2 {
                        continue;
                    }
                    let col = shape.cells[0][1] as f64;
                    let rows: Vec<i64> = shape.cells.iter().map(|c| c[0]).collect();
                    let r0 = (*rows.iter().min().unwrap()) as f64;
                    let r1 = (*rows.iter().max().unwrap()) as f64;
                    let x = step * (col + 0.5) + stagger_unit * shape.stagger as f64;
                    let mut line = element::Line::new();
                    line.assign("x1", x);
                    line.assign("y1", step * (r0 + 0.5));
                    line.assign("x2", x);
                    line.assign("y2", step * (r1 + 0.5));
                    line.assign("class", class);
                    grp.append(line);
                }
                ConstraintShapeKind::Pair => {
                    let [a, b] = [shape.cells[0], shape.cells[1]];
                    let mut line = element::Line::new();
                    line.assign("x1", step * (a[1] as f64 + 0.5));
                    line.assign("y1", step * (a[0] as f64 + 0.5));
                    line.assign("x2", step * (b[1] as f64 + 0.5));
                    line.assign("y2", step * (b[0] as f64 + 0.5));
                    line.assign("class", class);
                    grp.append(line);
                }
                ConstraintShapeKind::Region => {
                    let path_d = region_perimeter_path(&shape.cells, step);
                    if path_d.is_empty() {
                        continue;
                    }
                    let mut p = element::Path::new();
                    p.assign("d", path_d);
                    p.assign("class", class);
                    grp.append(p);
                }
            }
        }
        grp
    }

    /// Draw small cage sum labels in the top-left corner of each cage's top-left cell.
    fn draw_cage_sums(&self, puzzle: &Puzzle) -> element::Group {
        let mut grp = element::Group::new();
        let cages = match &puzzle.cages {
            Some(c) => c,
            None => return grp,
        };
        let cage_sums = match &puzzle.cage_sums {
            Some(s) => s,
            None => return grp,
        };

        let step = 1.0 / std::cmp::min(puzzle.width, puzzle.height) as f64;
        let font_size = step * 0.28;
        let height = usize::try_from(puzzle.height).expect("negative height");
        let width = usize::try_from(puzzle.width).expect("negative width");

        // For each cage ID, find the top-left cell (min row, then min col).
        let mut cage_topleft: std::collections::BTreeMap<i64, (usize, usize)> =
            std::collections::BTreeMap::new();
        for r in 0..height {
            for c in 0..width {
                if let Some(Some(cage_id)) = cages.get(r).and_then(|row| row.get(c)) {
                    cage_topleft.entry(*cage_id).or_insert((r, c));
                }
            }
        }

        for (cage_id, (r, c)) in cage_topleft {
            let idx = (cage_id - 1) as usize;
            if idx >= cage_sums.len() {
                continue;
            }
            let sum = cage_sums[idx];
            let x = step * c as f64 + step * 0.04;
            let y = step * r as f64 + font_size + step * 0.03;

            let mut text = svg::node::element::Text::new(sum.to_string());
            text.assign("x", x);
            text.assign("y", y);
            text.assign("font-size", font_size);
            text.assign("fill", "#111111");
            grp.append(text);
        }

        grp
    }

    fn make_cells(&self, puzzle: &Puzzle) -> Vec<Vec<element::Group>> {
        let step = 1.0 / std::cmp::min(puzzle.width, puzzle.height) as f64;

        let mut out = Vec::new();
        for i in 0..puzzle.height {
            out.push(vec![]);
            for j in 0..puzzle.width {
                let g = make_cell(i, j, step);

                out.last_mut().unwrap().push(g);
            }
        }

        out
    }

    /// Parallel grid of empty groups sharing each cell's local-coordinate
    /// transform.  Used as the destination for text content (digits and
    /// candidates) so they can be rendered AFTER overlays — keeping numbers
    /// always on top of constraint-shape lines/regions.  No `id` (would
    /// clash with the corresponding cell `<g>`) and no background rect.
    fn make_text_cells(&self, puzzle: &Puzzle) -> Vec<Vec<element::Group>> {
        let step = 1.0 / std::cmp::min(puzzle.width, puzzle.height) as f64;

        let mut out = Vec::new();
        for i in 0..puzzle.height {
            out.push(vec![]);
            for j in 0..puzzle.width {
                let i_f = i as f64;
                let j_f = j as f64;
                let mut g = element::Group::new();
                g.assign(
                    "transform",
                    format!(
                        "translate({} {}) scale({})",
                        step * (j_f + 0.05),
                        step * (i_f + 0.05),
                        step * 0.9
                    ),
                );
                // Mouse / hover handling stays on the underlying cell <g>.
                g.assign("pointer-events", "none");
                g.assign("class", "cell-text-overlay");
                out.last_mut().unwrap().push(g);
            }
        }

        out
    }
}

/// Build an SVG `path d` attribute tracing the outline of the union of `cells`,
/// skipping any cell-edge that's shared with another cell in the same set.
/// Edges sit exactly on cell boundaries (no inset).  Output is a series of
/// disconnected `M x y L x y` segments — fine for stroking, the path is not
/// expected to be closed or filled.
fn region_perimeter_path(cells: &[[i64; 2]], step: f64) -> String {
    use std::collections::BTreeSet;
    let cellset: BTreeSet<[i64; 2]> = cells.iter().copied().collect();
    let mut out = String::new();
    for &[r, c] in cells {
        let x0 = step * c as f64;
        let x1 = step * (c + 1) as f64;
        let y0 = step * r as f64;
        let y1 = step * (r + 1) as f64;
        // Top edge — skip if cell (r-1, c) is in scope.
        if !cellset.contains(&[r - 1, c]) {
            out.push_str(&format!("M {x0} {y0} L {x1} {y0} "));
        }
        // Bottom edge — skip if cell (r+1, c) is in scope.
        if !cellset.contains(&[r + 1, c]) {
            out.push_str(&format!("M {x0} {y1} L {x1} {y1} "));
        }
        // Left edge — skip if cell (r, c-1) is in scope.
        if !cellset.contains(&[r, c - 1]) {
            out.push_str(&format!("M {x0} {y0} L {x0} {y1} "));
        }
        // Right edge — skip if cell (r, c+1) is in scope.
        if !cellset.contains(&[r, c + 1]) {
            out.push_str(&format!("M {x1} {y0} L {x1} {y1} "));
        }
    }
    out
}

fn make_cell(i: i64, j: i64, step: f64) -> element::Group {
    let i_f = i as f64;
    let j_f = j as f64;

    let mut g = element::Group::new();
    g.assign("id", format!("C_{}_{}", i + 1, j + 1));
    g.assign("data-cell", format!("{},{}", i + 1, j + 1));
    g.assign(
        "transform",
        format!(
            "translate({} {}) scale({})",
            step * (j_f + 0.05),
            step * (i_f + 0.05),
            step * 0.9
        ),
    );

    // Transparent background rect — always present so CSS can target it for highlighting
    // (e.g. g.con-preview .cell-bg { fill: ... }).  Sized to fill the 0..1 cell coordinate space.
    let mut bg = element::Rectangle::new();
    bg.assign("class", "cell-bg");
    bg.assign("width", 1);
    bg.assign("height", 1);
    g.append(bg);

    g
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use test_log::test;

    use crate::{json::Problem, web::puzsvg::PuzzleDraw};

    #[test]
    fn test_svg_sudoku() -> anyhow::Result<()> {
        let file = File::open("./tst/sudoku.json")?;
        let problem: Problem = serde_json::from_reader(file)?;
        let puz_draw = PuzzleDraw::new(&problem.puzzle.kind);
        let svg = puz_draw.draw_puzzle(&problem);
        let svg_str = svg.to_string();

        // Should produce non-empty SVG with grid rectangles.
        assert!(!svg_str.is_empty(), "SVG output must not be empty");
        assert!(svg_str.contains("<svg"), "SVG output must contain <svg tag");
        assert!(
            svg_str.contains("rect"),
            "SVG output must contain rect elements"
        );

        Ok(())
    }

    #[test]
    fn test_svg_sudoku_has_sudoku_grid_lines() -> anyhow::Result<()> {
        // The sudoku kind triggers SudokuGrid decoration — thick lines every 3 cells.
        let file = File::open("./tst/sudoku.json")?;
        let problem: Problem = serde_json::from_reader(file)?;
        let puz_draw = PuzzleDraw::new(&problem.puzzle.kind);
        let svg = puz_draw.draw_puzzle(&problem);
        let svg_str = svg.to_string();

        // Sudoku grids use stroke-width > 0.1 for box boundaries.
        assert!(
            svg_str.contains("stroke-width"),
            "Sudoku SVG must include stroke-width attributes for grid lines"
        );

        Ok(())
    }

    #[test]
    fn test_svg_multilayer_labels_nonogram_5x5() -> anyhow::Result<()> {
        // 5x5 nonogram layered labels: row clues on top_labels, col clues on left_labels.
        // Each has depth 3. Verify the renderer emits every non-empty label string.
        let puzzle = crate::json::Puzzle {
            kind: "Nonogram".to_string(),
            width: 5,
            height: 5,
            start_grid: None,
            solution_grid: None,
            cages: None,
            region_tint: None,
            top_labels: Some(vec![
                vec!["".into(), "".into(), "".into(), "".into(), "1".into()],
                vec!["".into(), "1".into(), "3".into(), "2".into(), "1".into()],
                vec!["5".into(), "3".into(), "1".into(), "2".into(), "1".into()],
            ]),
            bottom_labels: None,
            left_labels: Some(vec![
                vec!["".into(), "".into(), "".into(), "".into(), "".into()],
                vec!["".into(), "".into(), "".into(), "".into(), "".into()],
                vec!["5".into(), "1".into(), "3".into(), "2".into(), "5".into()],
            ]),
            right_labels: None,
            thermometers: None,
            less_than: None,
            cage_sums: None,
            info: None,
            constraint_classes: None,
            decorations: vec![],
        };
        let problem = Problem {
            puzzle,
            state: None,
        };
        let svg = PuzzleDraw::new("Nonogram").draw_puzzle(&problem);
        let svg_str = svg.to_string();

        // The labels group should carry one <text> element per non-empty clue.
        // top_labels: 1 + 4 + 5 = 10 non-empty; left_labels: 0 + 0 + 5 = 5 → 15 total.
        let text_count = svg_str.matches("<text").count();
        assert_eq!(
            text_count, 15,
            "expected 15 label <text> elements for 15 non-empty clues, got {text_count}"
        );
        // Sanity: the clue values actually appear in the output.
        for expected in ["5", "1", "3", "2"] {
            assert!(
                svg_str.contains(expected),
                "expected clue {expected} in SVG output"
            );
        }
        Ok(())
    }

    #[test]
    fn test_svg_minesweeper() -> anyhow::Result<()> {
        // Build a minesweeper puzzle without a pre-parsed JSON file.
        let puz = crate::problem::util::test_utils::build_puzzleparse(
            "./tst/minesweeper.eprime",
            "./tst/minesweeperPrinted.param",
        );
        let puzzle = crate::json::Puzzle::new_from_puzzle(&puz)?;
        let problem = Problem {
            puzzle,
            state: None,
        };
        let puz_draw = PuzzleDraw::new(&problem.puzzle.kind);
        let svg = puz_draw.draw_puzzle(&problem);
        let svg_str = svg.to_string();

        assert!(!svg_str.is_empty());
        assert!(svg_str.contains("<svg"));

        Ok(())
    }
}
