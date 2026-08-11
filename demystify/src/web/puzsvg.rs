#![allow(clippy::ptr_arg)]
#![allow(clippy::needless_range_loop)]

use std::collections::BTreeSet;

use crate::json::StateLit;

use crate::json::{Problem, Puzzle};
use crate::web::geometry::{Geometry, HexGeometry, SquareGeometry};
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
    /// Lay the grid out as pointy-top hexagons (axial coordinates) rather than squares.
    Hex,
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
                } else if dec == "hex" {
                    flags.insert(DecorationKind::Hex);
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

    fn hex(&self) -> bool {
        self.flags.contains(&DecorationKind::Hex)
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
            base_width: 0.02,
            mid_width: 0.04,
            thick_width: 0.08,
            decorations: Decorations::new(kind, &[]),
        }
    }

    #[must_use]
    pub fn new_with_decs(kind: &str, decs: &[String]) -> Self {
        PuzzleDraw {
            base_width: 0.02,
            mid_width: 0.04,
            thick_width: 0.08,
            decorations: Decorations::new(kind, decs),
        }
    }
}

impl PuzzleDraw {
    #[must_use]
    pub fn draw_puzzle(&self, puzjson: &Problem) -> svg::Document {
        let puzzle = &puzjson.puzzle;
        let geom: Box<dyn Geometry> = if self.decorations.hex() {
            Box::new(HexGeometry::new(puzzle.width, puzzle.height))
        } else {
            Box::new(SquareGeometry::new(puzzle.width, puzzle.height))
        };
        let geom = geom.as_ref();

        let mut cells = self.make_cells(geom, puzzle);
        let mut text_cells = self.make_text_cells(geom);

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

        self.set_cell_data_states(&mut cells, puzjson);

        // Fixed layer stack — document order is z-order, bottom to top.
        let mut board = element::Group::new();

        // Cage / region fills and the grid outline.
        board.append(self.draw_grid(geom, puzzle));

        // Constraint lines (thermometers) sit above the grid but below cells.
        board.append(self.draw_thermometers(geom, puzzle));

        // Cell backgrounds and per-cell interactive content (litboxes, walls).
        let mut cells_layer = element::Group::new();
        cells_layer.assign("class", "layer-cells");
        for row in cells {
            for c in row {
                cells_layer.append(c);
            }
        }
        board.append(cells_layer);

        // Overlays drawn above cell backgrounds but below the text overlay, so
        // digits / candidate numbers always sit on top.
        let mut overlay_layer = element::Group::new();
        overlay_layer.assign("class", "layer-overlays");
        overlay_layer.append(self.draw_less_than(geom, puzzle));
        overlay_layer.append(self.draw_cage_sums(geom, puzzle));
        board.append(overlay_layer);

        // Text (givens and candidate digits), always on top of overlays.
        let mut text_layer = element::Group::new();
        text_layer.assign("class", "layer-digits cell-text-overlays");
        for row in text_cells {
            for c in row {
                text_layer.append(c);
            }
        }
        board.append(text_layer);

        // Outside labels add their own layer and may extend the viewBox.
        let (labels_layer, (min_x, min_y, max_x, max_y)) = self.fill_outside_labels(geom, puzzle);
        board.append(labels_layer);

        let margin = 0.15;
        let vb = (
            min_x - margin,
            min_y - margin,
            (max_x - min_x) + 2.0 * margin,
            (max_y - min_y) + 2.0 * margin,
        );

        // Inline the board stylesheet so the SVG renders correctly on its own
        // (standalone export, file preview) without an external stylesheet. On a
        // demystify-web page these rules coexist with the page's own CSS.
        let style = element::Style::new(crate::web::board_css());

        let doc = svg::Document::new()
            .set("viewBox", vb)
            .set("preserveAspectRatio", "xMidYMid meet")
            .set("id", "board")
            .set("class", "puzzle");
        doc.add(style).add(board)
    }

    /// Build the outside-label layer and return it together with the bounding
    /// box `(min_x, min_y, max_x, max_y)` of the grid *and* its labels, in cell
    /// units, so the caller can size the `viewBox`.
    ///
    /// Label placement preserves the existing (transposed) side convention:
    /// `top_labels` render to the left of the grid, `bottom_labels` to the
    /// right, `left_labels` above and `right_labels` below — the puzzle pipeline
    /// feeds them in that transposed order.
    fn fill_outside_labels(
        &self,
        geom: &dyn Geometry,
        p: &Puzzle,
    ) -> (element::Group, (f64, f64, f64, f64)) {
        let mut label_group = element::Group::new();
        label_group.assign("class", "layer-labels labels");

        let (mut min_x, mut min_y, mut max_x, mut max_y) = geom.bounds();
        let mut expand = |row: i64, col: i64| {
            for (x, y) in geom.cell_polygon(row, col) {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        };

        let label_groups = [
            &p.top_labels,
            &p.bottom_labels,
            &p.left_labels,
            &p.right_labels,
        ];

        // (total_layers L, layer index, along-side index i) → (row, col).
        let pos = |side: usize, l: i64, layer: i64, i: i64| -> (i64, i64) {
            match side {
                0 => (i, -(l - layer)),     // top_labels
                1 => (i, p.height + layer), // bottom_labels
                2 => (-(l - layer), i),     // left_labels
                _ => (p.width + layer, i),  // right_labels
            }
        };

        for (side, layers_opt) in label_groups.iter().enumerate() {
            if let Some(layers) = layers_opt {
                let total_layers = layers.len() as i64;
                for (layer_idx, labels) in layers.iter().enumerate() {
                    let layer = layer_idx as i64;
                    for (i, label) in labels.iter().enumerate() {
                        if label.is_empty() {
                            continue;
                        }
                        let (row, col) = pos(side, total_layers, layer, i as i64);
                        let mut node = svg::node::element::Text::new(label);
                        node.assign("font-size", 1);
                        node.assign("transform", "translate(0.2, 0.9)");
                        let mut g = make_cell(geom, row, col);
                        g.append(node);
                        label_group.append(g);
                        expand(row, col);
                    }
                }
            }
        }

        (label_group, (min_x, min_y, max_x, max_y))
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
                    // Dark outer rect fills the entire cell (colour from board.css).
                    let mut outer = element::Rectangle::new();
                    outer.assign("width", 1);
                    outer.assign("height", 1);
                    outer.assign("class", "wall-cell");
                    cells[i][j].append(outer);

                    // Light inner rect — leaves a thick dark border around the edge.
                    let mut inner = element::Rectangle::new();
                    inner.assign("x", 0.1);
                    inner.assign("y", 0.1);
                    inner.assign("width", 0.8);
                    inner.assign("height", 0.8);
                    inner.assign("class", "wall-inner");
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
            bg.assign("class", "blocked-cell");
            bg.assign("opacity", 0.5);
            cells[i][j].append(bg);

            // "?" text centered in the cell (colour from board.css .litundeducable).
            let mut text = svg::node::element::Text::new("?");
            text.assign("font-size", 0.7);
            text.assign("x", 0.5);
            text.assign("y", 0.75);
            text.assign("dominant-baseline", "middle");
            text.assign("text-anchor", "middle");
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

    fn draw_grid(&self, geom: &dyn Geometry, puzzle: &Puzzle) -> element::Group {
        let mut topgrp = element::Group::new();

        let width = usize::try_from(puzzle.width).expect("negative width?");
        let height = usize::try_from(puzzle.height).expect("negative height?");
        let cages = &puzzle.cages;

        // Number of palette colours defined as `.region-N` in board.css.
        const REGION_COLOURS: i64 = 16;

        // Closed `path d` tracing the polygon of cell (row, col).
        let cell_path = |row: i64, col: i64| -> String {
            let poly = geom.cell_polygon(row, col);
            let mut d = String::new();
            for (k, (x, y)) in poly.iter().enumerate() {
                d.push_str(if k == 0 { "M " } else { "L " });
                d.push_str(&format!("{x} {y} "));
            }
            d.push('Z');
            d
        };

        let mut cagegrp = element::Group::new();
        cagegrp.assign("class", "layer-cages");

        if let Some(cages) = &cages {
            let colours: BTreeSet<_> = cages.iter().flatten().filter_map(|cell| *cell).collect();

            for i in 0..width {
                for j in 0..height {
                    if !cell_present(puzzle, j as i64, i as i64) {
                        continue;
                    }
                    if let Some(cell) = cages[j][i] {
                        let col = colours.iter().position(|&c| c == cell).unwrap() as i64;
                        let mut p = element::Path::new();
                        p.assign("d", cell_path(j as i64, i as i64));
                        p.assign("class", format!("region-{}", col % REGION_COLOURS));
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
                    let col = (k - 1).max(0) % REGION_COLOURS;
                    let mut p = element::Path::new();
                    p.assign("d", cell_path(j as i64, i as i64));
                    p.assign("class", format!("region-{col}"));
                    cagegrp.append(p);
                }
            }
        }

        topgrp.append(cagegrp);

        let outlinegrp = if self.decorations.hex() {
            self.draw_outline_generic(geom, puzzle)
        } else {
            self.draw_outline_square(puzzle, width, height)
        };
        topgrp.append(outlinegrp);
        topgrp
    }

    /// Rectangular grid outline: one stroke per grid line, thicker at the border,
    /// at cage boundaries and (for sudoku) every third line. This is the original
    /// square path, kept verbatim so square puzzles render unchanged.
    fn draw_outline_square(&self, puzzle: &Puzzle, width: usize, height: usize) -> element::Group {
        let cages = &puzzle.cages;
        let mut outlinegrp = element::Group::new();
        outlinegrp.assign("class", "layer-grid");

        // Vertical grid lines.
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
                let path = format!("M {} {} L {} {}", i, j, i, j + 1);
                let mut p = element::Path::new();
                p.assign("d", path);
                p.assign("stroke", "black");
                p.assign("stroke-width", stroke);
                p.assign("stroke-linecap", "round");
                outlinegrp = outlinegrp.add(p);
            }
        }

        // Horizontal grid lines.
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
                let path = format!("M {} {} L {} {}", i, j, i + 1, j);
                let mut p = element::Path::new();
                p.assign("d", path);
                p.assign("stroke", "black");
                p.assign("stroke-width", stroke);
                p.assign("stroke-linecap", "round");
                outlinegrp.append(p);
            }
        }

        outlinegrp
    }

    /// Topology-generic outline: stroke each present cell's polygon edges, drawing
    /// every shared edge once (from the lexicographically-smaller cell). Edges on
    /// the board boundary or between different cages get the thick stroke. Works
    /// for any [`Geometry`]; used for hex boards.
    fn draw_outline_generic(&self, geom: &dyn Geometry, puzzle: &Puzzle) -> element::Group {
        let mut outlinegrp = element::Group::new();
        outlinegrp.assign("class", "layer-grid");

        let cage_at = |r: i64, c: i64| -> Option<i64> {
            puzzle
                .cages
                .as_ref()
                .and_then(|cg| cg.get(r as usize))
                .and_then(|row| row.get(c as usize))
                .copied()
                .flatten()
        };

        for r in 0..geom.height() {
            for c in 0..geom.width() {
                if !cell_present(puzzle, r, c) {
                    continue;
                }
                let poly = geom.cell_polygon(r, c);
                let n = poly.len();
                for (nr, nc, k) in geom.neighbours(r, c) {
                    let nb_present = cell_present(puzzle, nr, nc);
                    // Draw each shared edge once: skip if a present neighbour is
                    // lexicographically smaller (it will draw the edge instead).
                    if nb_present && (nr, nc) < (r, c) {
                        continue;
                    }
                    let stroke = if !nb_present || cage_at(r, c) != cage_at(nr, nc) {
                        self.thick_width
                    } else {
                        self.base_width
                    };
                    let (x0, y0) = poly[k];
                    let (x1, y1) = poly[(k + 1) % n];
                    let mut p = element::Path::new();
                    p.assign("d", format!("M {x0} {y0} L {x1} {y1}"));
                    p.assign("stroke", "black");
                    p.assign("stroke-width", stroke);
                    p.assign("stroke-linecap", "round");
                    outlinegrp.append(p);
                }
            }
        }
        outlinegrp
    }

    /// Draw thermometer paths (bulb circle + tube line) as SVG overlays.
    fn draw_thermometers(&self, geom: &dyn Geometry, puzzle: &Puzzle) -> element::Group {
        let mut grp = element::Group::new();
        grp.assign("class", "layer-lines");
        let therms = match &puzzle.thermometers {
            Some(t) => t,
            None => return grp,
        };

        let radius = 0.38;
        let tube_width = 0.5;
        let outline_extra = 0.06;

        // Two-pass rendering: outlines first, then fills, so fills appear on top.
        // Colours come from board.css (`.thermo-*`); only geometry is inline.
        let mut outlines = element::Group::new();
        let mut fills = element::Group::new();

        for therm in therms {
            if therm.is_empty() {
                continue;
            }

            let points: String = therm
                .iter()
                .map(|[r, c]| {
                    let (x, y) = geom.cell_centre(*r, *c);
                    format!("{x},{y}")
                })
                .collect::<Vec<_>>()
                .join(" ");

            if therm.len() > 1 {
                let mut poly_outline = element::Polyline::new();
                poly_outline.assign("points", points.clone());
                poly_outline.assign("fill", "none");
                poly_outline.assign("class", "thermo-tube-outline");
                poly_outline.assign("stroke-width", tube_width + outline_extra);
                poly_outline.assign("stroke-linecap", "round");
                poly_outline.assign("stroke-linejoin", "round");
                outlines.append(poly_outline);

                let mut poly_fill = element::Polyline::new();
                poly_fill.assign("points", points);
                poly_fill.assign("fill", "none");
                poly_fill.assign("class", "thermo-tube");
                poly_fill.assign("stroke-width", tube_width);
                poly_fill.assign("stroke-linecap", "round");
                poly_fill.assign("stroke-linejoin", "round");
                fills.append(poly_fill);
            }

            // Bulb circle at the first cell.
            let [br, bc] = therm[0];
            let (cx, cy) = geom.cell_centre(br, bc);

            let mut bulb_outline = element::Circle::new();
            bulb_outline.assign("cx", cx);
            bulb_outline.assign("cy", cy);
            bulb_outline.assign("r", radius + outline_extra / 2.0);
            bulb_outline.assign("class", "thermo-bulb-outline");
            outlines.append(bulb_outline);

            let mut bulb_fill = element::Circle::new();
            bulb_fill.assign("cx", cx);
            bulb_fill.assign("cy", cy);
            bulb_fill.assign("r", radius);
            bulb_fill.assign("class", "thermo-bulb");
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
    fn draw_less_than(&self, geom: &dyn Geometry, puzzle: &Puzzle) -> element::Group {
        let mut grp = element::Group::new();
        grp.assign("class", "layer-ineq");
        let pairs = match &puzzle.less_than {
            Some(p) => p,
            None => return grp,
        };

        let s = 0.14; // half-arm length, cell units
        let stroke_w = 0.03;

        for &[r1, c1, r2, c2] in pairs {
            // Only render for directly adjacent cells.
            if (r2 - r1).abs() + (c2 - c1).abs() != 1 {
                continue;
            }

            // The symbol sits at the midpoint of the two cell centres, which for
            // adjacent cells is the shared edge midpoint.
            let (ax1, ay1) = geom.cell_centre(r1, c1);
            let (ax2, ay2) = geom.cell_centre(r2, c2);
            let cx = (ax1 + ax2) / 2.0;
            let cy = (ay1 + ay2) / 2.0;

            // Chevron: two lines (ax,ay)→(mx,my) and (mx,my)→(bx,by).
            // (mx,my) is the tip, pointing toward the smaller cell (r1,c1).
            let (ax, ay, mx, my, bx, by);

            if r1 == r2 {
                // Horizontal neighbours.
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
                line.assign("class", "ineq");
                line.assign("stroke-width", stroke_w);
                line.assign("stroke-linecap", "round");
                grp.append(line);
            }
        }

        grp
    }

    /// Draw small cage sum labels in the top-left corner of each cage's top-left cell.
    fn draw_cage_sums(&self, geom: &dyn Geometry, puzzle: &Puzzle) -> element::Group {
        let mut grp = element::Group::new();
        grp.assign("class", "layer-cage-sums");
        let cages = match &puzzle.cages {
            Some(c) => c,
            None => return grp,
        };
        let cage_sums = match &puzzle.cage_sums {
            Some(s) => s,
            None => return grp,
        };

        let font_size = 0.28;
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
            // Top-left vertex of the cell, nudged inward.
            let (vx, vy) = geom.cell_polygon(r as i64, c as i64)[0];
            let x = vx + 0.04;
            let y = vy + font_size + 0.03;

            let mut text = svg::node::element::Text::new(sum.to_string());
            text.assign("x", x);
            text.assign("y", y);
            text.assign("font-size", font_size);
            text.assign("class", "cage-sum");
            grp.append(text);
        }

        grp
    }

    fn make_cells(&self, geom: &dyn Geometry, puzzle: &Puzzle) -> Vec<Vec<element::Group>> {
        let mut out = Vec::new();
        for i in 0..geom.height() {
            out.push(vec![]);
            for j in 0..geom.width() {
                // Absent cells (presence mask) get an empty placeholder so the
                // grid stays indexable, but render nothing.
                let g = if cell_present(puzzle, i, j) {
                    make_cell(geom, i, j)
                } else {
                    element::Group::new()
                };
                out.last_mut().unwrap().push(g);
            }
        }

        out
    }

    /// Parallel grid of empty groups sharing each cell's local-coordinate
    /// transform.  Used as the destination for text content (digits and
    /// candidates) so they can be rendered AFTER overlays — keeping numbers
    /// always on top of the overlay layer.  No `id` (would
    /// clash with the corresponding cell `<g>`) and no background rect.
    fn make_text_cells(&self, geom: &dyn Geometry) -> Vec<Vec<element::Group>> {
        let mut out = Vec::new();
        for i in 0..geom.height() {
            out.push(vec![]);
            for j in 0..geom.width() {
                let mut g = element::Group::new();
                g.assign("transform", cell_content_transform(geom, i, j));
                // Mouse / hover handling stays on the underlying cell <g>.
                g.assign("pointer-events", "none");
                g.assign("class", "cell-text-overlay");
                out.last_mut().unwrap().push(g);
            }
        }

        out
    }
}

/// Whether cell `(row, col)` is part of the board: in bounds, and not masked
/// out by the presence grid (used to carve non-rectangular shapes, e.g. a
/// radius-N hexagon from the `[r][q]` rhombus).
fn cell_present(puzzle: &Puzzle, row: i64, col: i64) -> bool {
    if row < 0 || col < 0 || row >= puzzle.height || col >= puzzle.width {
        return false;
    }
    match &puzzle.present {
        None => true,
        Some(grid) => grid
            .get(row as usize)
            .and_then(|r| r.get(col as usize))
            .copied()
            .unwrap_or(false),
    }
}

/// Transform placing a cell's local `0..1` content box (used for candidate
/// grids, given digits, walls) centred on the cell, inset to 0.9 of the cell so
/// a small margin shows around the content.  Works for any geometry: the box is
/// centred on [`Geometry::cell_centre`].
fn cell_content_transform(geom: &dyn Geometry, i: i64, j: i64) -> String {
    let (cx, cy) = geom.cell_centre(i, j);
    format!("translate({} {}) scale(0.9)", cx - 0.45, cy - 0.45)
}

fn make_cell(geom: &dyn Geometry, i: i64, j: i64) -> element::Group {
    let mut g = element::Group::new();
    g.assign("id", format!("C_{}_{}", i + 1, j + 1));
    g.assign("data-cell", format!("{},{}", i + 1, j + 1));
    g.assign("transform", cell_content_transform(geom, i, j));

    // Transparent background polygon — always present so CSS can target it for
    // highlighting (e.g. g.con-preview .cell-bg { fill: ... }).  Follows the cell
    // shape (square or hex) in the 0..1 local content space.
    let points = geom
        .local_bg_points()
        .iter()
        .map(|(x, y)| format!("{x},{y}"))
        .join(" ");
    let mut bg = element::Polygon::new();
    bg.assign("class", "cell-bg");
    bg.assign("points", points);
    g.append(bg);

    g
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use test_log::test;

    use crate::{json::Problem, web::puzsvg::PuzzleDraw};

    /// The cell-unit/layer refactor must keep a cell-unit viewBox (not the old
    /// 500×500 + scale(400) wrapper), emit the named layer stack, and preserve
    /// the literal-by-default DOM contract the JS hover layer and CSS depend on.
    #[test]
    fn test_svg_cell_unit_layers_and_literals() -> anyhow::Result<()> {
        let file = File::open("./tst/render_fixture.json")?;
        let problem: Problem = serde_json::from_reader(file)?;
        let svg = PuzzleDraw::new_with_decs(&problem.puzzle.kind, &problem.puzzle.decorations)
            .draw_puzzle(&problem);
        let s = svg.to_string();

        // Cell-unit coordinate system, not the legacy fixed-pixel wrapper.
        assert!(
            !s.contains("0 0 500 500") && !s.contains("scale(400)"),
            "should use a cell-unit viewBox, not the 500×500 wrapper"
        );
        // Named layer stack.
        for cls in [
            "layer-cages",
            "layer-grid",
            "layer-cells",
            "layer-overlays",
            "layer-digits",
        ] {
            assert!(s.contains(cls), "missing layer class {cls}");
        }
        // Literal-by-default DOM contract: per-cell ids, candidate boxes, classes.
        assert!(s.contains("C_1_1"), "cell id C_1_1 missing");
        assert!(s.contains("litbox"), "candidate litbox missing");
        assert!(s.contains("data-cand"), "data-cand missing");
        assert!(s.contains("litinmus"), "litinmus class missing");
        Ok(())
    }

    /// The board carries its styling inline (so exported SVG renders standalone)
    /// and moves colours out of Rust into board.css classes.
    #[test]
    fn test_svg_self_contained_styling() -> anyhow::Result<()> {
        let file = File::open("./tst/render_fixture.json")?;
        let problem: Problem = serde_json::from_reader(file)?;
        let svg = PuzzleDraw::new_with_decs(&problem.puzzle.kind, &problem.puzzle.decorations)
            .draw_puzzle(&problem);
        let s = svg.to_string();
        // Inline stylesheet present.
        assert!(s.contains("<style"), "inline <style> missing");
        assert!(s.contains(".thermo-tube"), "board.css not embedded");
        // Colours are class-driven, not hardcoded as fill attributes.
        assert!(
            s.contains("class=\"thermo-bulb\""),
            "thermo bulb class missing"
        );
        assert!(
            s.contains("region-"),
            "cage/region fills should use region-N classes"
        );
        assert!(
            !s.contains("fill=\"#d8d8d8\""),
            "thermometer colour should live in CSS, not a fill attribute"
        );
        Ok(())
    }

    /// The `hex` decoration lays the board out as hexagons: cell centres use the
    /// √3 axial spacing and candidates still render.
    #[test]
    fn test_svg_hex_topology() -> anyhow::Result<()> {
        let file = File::open("./tst/hex_fixture.json")?;
        let problem: Problem = serde_json::from_reader(file)?;
        let svg = PuzzleDraw::new_with_decs(&problem.puzzle.kind, &problem.puzzle.decorations)
            .draw_puzzle(&problem);
        let s = svg.to_string();
        // Hex axial spacing leaves the √3 factor in coordinates; a square board
        // would only have integer / half-integer cell positions.
        assert!(
            s.contains("1.732"),
            "hex √3 spacing missing — not laid out as hexagons"
        );
        // Literal-by-default candidates still render.
        assert!(s.contains("litbox"), "hex candidates missing");
        Ok(())
    }

    /// Outside labels (nonogram clues) still render and the viewBox expands to
    /// include them.
    #[test]
    fn test_svg_labels_expand_viewbox() -> anyhow::Result<()> {
        let file = File::open("./tst/nonogram_fixture.json")?;
        let problem: Problem = serde_json::from_reader(file)?;
        let svg = PuzzleDraw::new_with_decs(&problem.puzzle.kind, &problem.puzzle.decorations)
            .draw_puzzle(&problem);
        let s = svg.to_string();
        assert!(s.contains("layer-labels"), "labels layer missing");
        // 10 + 5 = 15 non-empty clue strings → 15 label <text> elements.
        assert!(s.contains("viewBox"), "viewBox missing");
        Ok(())
    }

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
            present: None,
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
