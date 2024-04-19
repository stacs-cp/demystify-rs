#![allow(clippy::ptr_arg)]
#![allow(clippy::needless_range_loop)]

use crate::json::StateLit;

use crate::json::{Problem, Puzzle};
use svg::Node;

use svg::node::element;

pub struct PuzzleDraw {
    base_width: f64,
    mid_width: f64,
    thick_width: f64,
}

impl Default for PuzzleDraw {
    fn default() -> Self {
        Self::new()
    }
}

impl PuzzleDraw {
    pub fn new() -> Self {
        PuzzleDraw {
            base_width: 0.005,
            mid_width: 0.01,
            thick_width: 0.02,
        }
    }

    fn new_with_options(base_width: f64, mid_width: f64, thick_width: f64) -> Self {
        PuzzleDraw {
            base_width,
            mid_width,
            thick_width,
        }
    }
}

impl PuzzleDraw {
    pub fn draw_puzzle(&self, puzjson: &Problem) -> svg::Document {
        let puzzle = &puzjson.puzzle;

        let mut out = self.draw_grid(puzzle, &puzzle.kind);

        let mut cells = self.make_cells(puzzle);

        if let Some(start_grid) = &puzzle.start_grid {
            println!("start_grid");
            self.fill_fixed_state(&mut cells, start_grid);
        }

        if let Some(state) = &puzjson.state {
            if let Some(knowledge_grid) = &state.knowledge_grid {
                println!("knowledge_grid");
                self.fill_knowledge(&mut cells, knowledge_grid);
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

        let doc = svg::Document::new();
        doc.add(out)
    }

    fn fill_fixed_state(
        &self,
        cells: &mut Vec<Vec<element::Group>>,
        contents: &Vec<Vec<Option<i64>>>,
    ) {
        for i in 0..contents.len() {
            for j in 0..contents[i].len() {
                if let Some(cell) = contents[i][j] {
                    let s = cell.to_string();

                    let mut node = svg::node::element::Text::new(s);
                    node.assign("font-size", 1);
                    node.assign("transform", "translate(0.2, 0.9)");

                    cells[i][j].append(node);

                    //.text(s)
                    //.font_size(1)
                    //.translate(0.2, 0.9);
                }
            }
        }
    }

    fn fill_knowledge(
        &self,
        cells: &mut Vec<Vec<element::Group>>,
        contents: &Vec<Vec<Option<Vec<StateLit>>>>,
    ) {
        for i in 0..contents.len() {
            for j in 0..contents[i].len() {
                if let Some(cell) = &contents[i][j] {
                    println!("{} {}", i, j);
                    // Find the right size of grid to fit our values in
                    let sqrt_length = (cell.len() as f64).sqrt().ceil() as usize;
                    let little_step = 0.9 / sqrt_length as f64;
                    for a in 0..sqrt_length {
                        for b in 0..sqrt_length {
                            if a * sqrt_length + b < cell.len() {
                                let state = &cell[a * sqrt_length + b];
                                let s = state.val.to_string();
                                let mut node = svg::node::element::Text::new(s);
                                node.assign("font-size", little_step);
                                node.assign(
                                    "transform",
                                    format!(
                                        "translate({}, {})",
                                        0.1 + b as f64 * little_step,
                                        (a as f64 + 1.2) * little_step
                                    ),
                                );
                                node.assign(
                                    "id",
                                    format!("D_{}_{}_{}", j, i, cell[a * sqrt_length + b].val),
                                );
                                if let Some(classes) = &state.classes {
                                    node.assign("classes", classes.join(" "));
                                }
                                cells[i][j].append(node);
                            }
                        }
                    }
                }
            }
        }
    }

    fn draw_grid(&self, puzzle: &Puzzle, kind: &String) -> element::Group {
        let mut topgrp = element::Group::new();
        topgrp.assign("transform", "translate(25,25) scale(450)");

        let mut grp = element::Group::new();

        let width = puzzle.width as usize;
        let height = puzzle.height as usize;
        let cages = &puzzle.cages;

        let sudoku_decorations = kind == "sudoku";

        let wstep = 1.0 / (width as f64);
        let hstep = 1.0 / (height as f64);

        let colours_list = ["#FFB3B3", "#B3B3FF", "#FFFFB3", "#B3FFB3", "#E6B3FF"];

        let mut cagegrp = element::Group::new();

        if let Some(cages) = &cages {
            let colours: Vec<_> = cages.iter().flatten().filter_map(|cell| *cell).collect();

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
                    if sudoku_decorations && i % 3 == 0 {
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
                    if sudoku_decorations && j % 3 == 0 {
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
                g.assign("id", format!("C_${j}_${i}"));
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

        let puz_draw = PuzzleDraw::new();

        let svg = puz_draw.draw_puzzle(&problem);

        let _ = svg.to_string();

        Ok(())
    }
}
