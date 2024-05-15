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
            println!("Unknown puzzle type: {kind}");
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

        let doc = svg::Document::new()
            .set("viewBox", (0, 0, 500, 500))
            .set("width", 500)
            .set("height", 500);
        doc.add(out)
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
                                        0.1 + b as f64 * little_step,
                                        (a as f64 + 1.2) * little_step
                                    ),
                                );

                                let mut rect = svg::node::element::Rectangle::new();
                                rect.assign("width", little_step);
                                rect.assign("height", little_step);
                                rect.assign("y", -little_step);
                                rect.assign("fill", "none");
                                rect.assign("stroke-width", "0.05");
                                rect.assign("stroke", "blue");
                                group.append(rect);

                                let mut node = svg::node::element::Text::new(s);
                                node.assign("font-size", little_step);

                                group.append(node);

                                group.assign(
                                    "id",
                                    format!("D_{}_{}_{}", j, i, cell[a * sqrt_length + b].val),
                                );
                                if let Some(classes) = &state.classes {
                                    group.assign("class", classes.iter().join(" "));
                                }

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
        topgrp.assign("transform", "translate(25,25) scale(450)");

        let mut grp = element::Group::new();

        let width = usize::try_from(puzzle.width).expect("negative width?");
        let height = usize::try_from(puzzle.height).expect("negative height?");
        let cages = &puzzle.cages;

        let wstep = 1.0 / (width as f64);
        let hstep = 1.0 / (height as f64);

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
                            wstep * i_f,
                            hstep * j_f,
                            wstep * (i_f + 1.0),
                            wstep * (j_f + 1.0),
                            wstep * i_f
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
                    wstep * i_f,
                    hstep * j_f,
                    wstep * i_f,
                    hstep * (j_f + 1.0)
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
                    wstep * i_f,
                    hstep * j_f,
                    wstep * (i_f + 1.0),
                    hstep * j_f
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
        let wstep = 1.0 / (puzzle.width as f64);
        let hstep = 1.0 / (puzzle.height as f64);

        let mut out = Vec::new();
        for i in 0..puzzle.width {
            out.push(vec![]);
            for j in 0..puzzle.height {
                let i_f = i as f64;
                let j_f = j as f64;

                let mut g = element::Group::new();
                g.assign("id", format!("C_{j}_{i}"));
                g.assign(
                    "transform",
                    format!(
                        "translate({} {}) scale({})",
                        wstep * (i_f + 0.05),
                        hstep * (j_f + 0.05),
                        wstep * 0.9
                    ),
                );

                out.last_mut().unwrap().push(g);
            }
        }

        out
    }
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
