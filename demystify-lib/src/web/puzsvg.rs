#![allow(clippy::ptr_arg)]
#![allow(clippy::needless_range_loop)]

use std::collections::BTreeSet;

use crate::json::StateLit;

use crate::json::{Problem, Puzzle};
use itertools::Itertools;
use svg::Node;

use svg::node::element;

struct Decorations {
    sudoku_grid: bool,
    blank_input_val: Option<i64>,
}

impl Decorations {
    pub fn new(kind: &str) -> Decorations {
        let kind = kind.to_lowercase();
        if kind == "sudoku" {
            Decorations {
                sudoku_grid: true,
                blank_input_val: Some(0),
            }
        } else if kind == "binairo" {
            Decorations {
                sudoku_grid: false,
                blank_input_val: Some(2),
            }
        } else {
            //println!("Unknown puzzle type: {kind}");
            Decorations {
                sudoku_grid: false,
                blank_input_val: None,
            }
        }
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
            decorations: Decorations::new(kind),
        }
    }
}

impl PuzzleDraw {
    #[must_use]
    pub fn draw_puzzle(&self, puzjson: &Problem) -> svg::Document {
        let puzzle = &puzjson.puzzle;

        let mut out = self.draw_grid(puzzle);

        let mut cells = self.make_cells(puzzle);

        if let Some(start_grid) = &puzzle.start_grid {
            self.fill_fixed_state(&mut cells, start_grid);
        }

        if let Some(state) = &puzjson.state {
            if let Some(knowledge_grid) = &state.knowledge_grid {
                self.fill_knowledge(&mut cells, &puzzle.start_grid, knowledge_grid);
            }
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

        let mut cellgrp = element::Group::new();

        for row in cells {
            for c in row {
                cellgrp.append(c);
            }
        }

        out.append(cellgrp);

        let out = self.fill_outside_labels(out, &puzzle);

        let mut final_grp = element::Group::new();
        final_grp.assign("transform", "translate(50,50) scale(400)");
        final_grp.append(out);

        let doc = svg::Document::new()
            .set("viewBox", (0, 0, 500, 500))
            .set("width", 500)
            .set("height", 500)
            .set("class", "puzzle");
        doc.add(final_grp)
    }

    fn fill_outside_labels(&self, mut grid: element::Group, p: &Puzzle) -> element::Group {
        let mut label_group = element::Group::new();
        label_group.assign("class", "labels");

        let step = 1.0 / std::cmp::min(p.width, p.height) as f64;

        let mut puz_bounds = (0.0, step * (p.width as f64), 0.0, step * (p.height as f64));

        // Add top labels
        let label_groups = [
            &p.top_labels,
            &p.bottom_labels,
            &p.left_labels,
            &p.right_labels,
        ];

        let label_positions: Vec<(
            Box<dyn Fn(usize) -> i64>,
            Box<dyn Fn(usize) -> i64>,
            Box<dyn Fn(&mut (f64, f64, f64, f64))>,
        )> = vec![
            (
                Box::new(|i| i as i64),
                Box::new(|_| -1),
                Box::new(|bounds| bounds.0 -= step),
            ),
            (
                Box::new(|i| i as i64),
                Box::new(|_| p.height),
                Box::new(|bounds| bounds.1 += step),
            ),
            (
                Box::new(|_| -1),
                Box::new(|i| i as i64),
                Box::new(|bounds| bounds.2 -= step),
            ),
            (
                Box::new(|_| p.width),
                Box::new(|i| i as i64),
                Box::new(|bounds| bounds.3 += step),
            ),
        ];

        for (labels, position) in label_groups.iter().zip(label_positions.iter()) {
            if let Some(labels) = labels {
                // Update grid bounds
                position.2(&mut puz_bounds);
                for (i, label) in labels.iter().enumerate() {
                    let mut node = svg::node::element::Text::new(label);
                    node.assign("font-size", 1);
                    node.assign("transform", "translate(0.2, 0.9)");
                    let mut g = make_cell(position.0(i), position.1(i), step);
                    g.append(node);
                    label_group.append(g);
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
        cell.is_some_and(|c| Some(c) != self.decorations.blank_input_val)
    }

    fn fill_fixed_state(
        &self,
        cells: &mut Vec<Vec<element::Group>>,
        contents: &Vec<Vec<Option<i64>>>,
    ) {
        for i in 0..contents.len() {
            for j in 0..contents[i].len() {
                if self.fixed_cell_is_used(contents[i][j]) {
                    let cell = contents[i][j].unwrap();
                    let s = cell.to_string();

                    let mut node = svg::node::element::Text::new(s);
                    node.assign("font-size", 1);
                    node.assign("transform", "translate(0.2, 0.9)");

                    cells[i][j].append(node);
                }
            }
        }
    }

    fn fill_knowledge(
        &self,
        cells: &mut Vec<Vec<element::Group>>,
        fixed_contents: &Option<Vec<Vec<Option<i64>>>>,
        contents: &Vec<Vec<Option<Vec<StateLit>>>>,
    ) {
        for i in 0..contents.len() {
            for j in 0..contents[i].len() {
                // The only reason we have 'fixed_contents' is because we do not want to
                // put knowledge in these cells
                if fixed_contents
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

                                let mut group = svg::node::element::Group::new();
                                group.assign(
                                    "transform",
                                    format!(
                                        "translate({}, {})",
                                        0.05 + (b as f64 * little_step),
                                        0.05 + (a as f64 + 1.0) * little_step
                                    ),
                                );

                                let mut rect = svg::node::element::Rectangle::new();
                                rect.assign("width", little_step);
                                rect.assign("height", little_step);
                                rect.assign("y", -little_step);
                                rect.assign("class", "litbox");
                                group.append(rect);

                                let mut node = svg::node::element::Text::new(s);
                                node.assign("font-size", little_step);
                                node.assign("x", little_step / 2.0);
                                node.assign("y", -little_step / 3.0);
                                node.assign("dominant-baseline", "middle");
                                node.assign("text-anchor", "middle");

                                group.append(node);

                                let id = format!(
                                    "D_{}_{}_{}",
                                    i + 1,
                                    j + 1,
                                    cell[a * sqrt_length + b].val
                                );
                                group.assign("id", id.clone());
                                group.assign("name", id);
                                group.assign("hx-post", "/clickLiteral");
                                group.assign("hx-target", "#mainSpace");
                                group.assign("class", "literal");
                                let mut classes = vec!["literal".to_owned()];

                                if let Some(extra_classes) = &state.classes {
                                    classes.extend(extra_classes.iter().cloned());
                                }
                                group.assign("class", classes.iter().join(" "));

                                cells[i][j].append(group);
                            }
                        }
                    }
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

        grp.append(cagegrp);

        let mut outlinegrp = element::Group::new();

        for i in 0..=width {
            for j in 0..height {
                let mut stroke = self.base_width;
                if i == 0 || i == width {
                    stroke = self.thick_width;
                } else {
                    if self.decorations.sudoku_grid && i % 3 == 0 {
                        stroke = self.mid_width;
                    }
                    if let Some(cages) = cages {
                        if cages[j][i] != cages[j][i - 1] {
                            stroke = self.thick_width;
                        }
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
                    if self.decorations.sudoku_grid && j % 3 == 0 {
                        stroke = self.mid_width;
                    }
                    if let Some(cages) = cages {
                        if cages[j][i] != cages[j - 1][i] {
                            stroke = self.thick_width;
                        }
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
}

fn make_cell(i: i64, j: i64, step: f64) -> element::Group {
    let i_f = i as f64;
    let j_f = j as f64;

    let mut g = element::Group::new();
    g.assign("id", format!("C_{}_{}", i + 1, j + 1));
    g.assign(
        "transform",
        format!(
            "translate({} {}) scale({})",
            step * (j_f + 0.05),
            step * (i_f + 0.05),
            step * 0.9
        ),
    );
    g
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use test_log::test;

    use crate::{json::Problem, web::puzsvg::PuzzleDraw};

    #[test]
    fn test_svg_sudoku() -> anyhow::Result<()> {
        let svg_path = "./tst/sudoku.json";

        let file = File::open(svg_path)?;
        let problem: Problem = serde_json::from_reader(file)?;

        let puz_draw = PuzzleDraw::new(&problem.puzzle.kind);

        let svg = puz_draw.draw_puzzle(&problem);

        let _ = svg.to_string();

        Ok(())
    }
}
