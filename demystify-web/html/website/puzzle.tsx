import { SVG } from "@svgdotjs/svg.js";

// G, Path, Line, Text
const _puzzle_decorations = {
  sudoku: { sudoku: true },
};

class PuzzleDraw {
  base_width: number;
  thick_width: number;
  colours_list: string[];

  constructor({
    base_width = 0.005,
    thick_width = 0.02,
    colours_list = ["red", "blue", "yellow", "green", "purple"],
  }: {
    base_width?: number;
    thick_width?: number;
    colours_list?: string[];
  }) {
    this.base_width = base_width;
    this.thick_width = thick_width;
    this.colours_list = colours_list;
  }

  fillMissing(puzdef: any) {
    puzdef = structuredClone(puzdef);
    let grid = null;
    const puz = puzdef["puzzle"];
    if ("startGrid" in puz) {
      grid = puz["startGrid"];
    }
    if ("solution" in puzdef) {
      grid = puzdef["solutionGrid"];
    }

    if (!("height" in puzdef)) {
      puzdef["height"] = grid.length;
    }
    if (!("width" in puzdef)) {
      puzdef["width"] = grid[0].length;
    }

    if (!("decorations" in puzdef)) {
      puzdef["decorations"] = {};
    }

    if (puzdef["type"] && puzdef["type"] in _puzzle_decorations) {
      for (const [k, v] of Object.entries(_puzzle_decorations)) {
        if (!(k in puzdef["decorations"])) {
          puzdef["decorations"][k] = v;
        }
      }
    }

    return puzdef;
  }

  drawPuzzle(puzjson: any) {
    puzjson = this.fillMissing(puzjson);

    const puzzle = puzjson["puzzle"];

    const out = this.drawGrid(puzzle);

    if ("startGrid" in puzzle) {
      console.log("startGrid");
      this.fillCells(out, puzzle["startGrid"]);
    }

    if ("solutionGrid" in puzzle) {
      const solutionCpy = structuredClone(puzzle["solutionGrid"]);
      if ("startGrid" in puzzle) {
        for (let i = 0; i < puzzle["startGrid"].length; i++) {
          for (let j = 0; j < puzzle["startGrid"][i].length; j++) {
            const cell = puzzle["startGrid"][i][j];
            if (cell) {
              solutionCpy[i][j] = null;
            }
          }
        }
      }
      this.fillCells(out, solutionCpy, { color: "grey" });
    }

    return out;
  }

  drawGrid(puzzle: any) {
    const svg = SVG();
    const topgrp = svg.group();
    topgrp.transform({ scale: 300 });

    const grp = topgrp.group();

    const width = puzzle["width"];
    const height = puzzle["height"];

    let cages = null;
    if ("cages" in puzzle) {
      cages = puzzle["cages"];
    }

    let sudoku_decorations = false;
    if ("decorations" in puzzle) {
      if ("sudoku" in puzzle["decorations"]) {
        sudoku_decorations = true;
      }
    }

    const wstep = 1.0 / width;
    const hstep = 1.0 / height;

    const colours_list = ["red", "blue", "yellow", "green", "purple"];
    if (cages) {
      const colours = Array.from(
        new Set(
          cages.flatMap((row: any) => row.filter((cell: any) => cell !== null))
        )
      );

      for (let i = 0; i < width; i++) {
        for (let j = 0; j < height; j++) {
          if (cages[j][i] !== null) {
            const col = colours.indexOf(cages[j][i]);
            const path = `M ${wstep * i} ${hstep * j} H ${wstep * (i + 1)} V ${
              wstep * (j + 1)
            } H ${wstep * i} Z`;
            const p = SVG().path(path).fill(this.colours_list[col]);

            grp.add(p);
          }
        }
      }
    }

    for (let i = 0; i < width + 1; i++) {
      for (let j = 0; j < height; j++) {
        let stroke = this.base_width;
        if (i === 0 || i === width) {
          stroke = this.thick_width;
        } else {
          if (sudoku_decorations && i % 3 === 0) {
            stroke = this.thick_width;
          }
          if (cages && cages[j][i] !== cages[j][i - 1]) {
            stroke = this.thick_width;
          }
        }
        grp.add(
          SVG()
            .line(wstep * i, hstep * j, wstep * i, hstep * (j + 1))
            .stroke({
              color: "black",
              width: stroke,
            })
        );
      }
    }

    for (let i = 0; i < width; i++) {
      for (let j = 0; j < height + 1; j++) {
        let stroke = this.base_width;
        if (j === 0 || j === height) {
          stroke = this.thick_width;
        } else {
          if (sudoku_decorations && j % 3 === 0) {
            stroke = this.thick_width;
          }
          if (cages && cages[j][i] !== cages[j - 1][i]) {
            stroke = this.thick_width;
          }
        }
        grp.add(
          SVG()
            .line(wstep * (i + 1), hstep * j, wstep * i, hstep * j)
            .stroke({
              color: "black",
              width: stroke,
            })
        );
      }
    }

    const cellgrp = topgrp.group();

    const cells = Array.from({ length: width }, () =>
      Array.from({ length: height }, () => SVG().group())
    );

    for (const row of cells) {
      for (const c of row) {
        cellgrp.add(c);
      }
    }

    for (let i = 0; i < width; ++i) {
      for (let j = 0; j < height; ++j) {
        let itrans = wstep * (i + 0.05);
        let jtrans = hstep * (j + 0.05);
        console.log(itrans, jtrans, wstep);
        cells[j][i].id(`C_${j}_${i}`);
        cells[j][i].transform({ translateX: itrans, translateY: jtrans });
        cells[j][i] = cells[j][i].group();
        cells[j][i] = cells[j][i].transform({ scale: wstep * 0.9 });
      }
    }

    return { svg: svg, cells };
  }

  fillCells(gridobj: any, contents: any, { color = "black" } = {}) {
    const cells = gridobj["cells"];
    console.log(contents);
    for (let i = 0; i < contents.length; i++) {
      for (let j = 0; j < contents[i].length; j++) {
        const cell = contents[i][j];
        console.log(cell);
        if (cell) {
          if (Number.isFinite(cell)) {
            const s = String(cell);
            const p = cells[i][j].line(0, 0, 1, 1).stroke({
              color: "blue",
              width: 0.01,
            });
            cells[i][j]
              .text(s)
              .font({ size: 1 })
              .transform({ translateX: 0.2, translateY: 0.9 });
          } else {
            // Find the right size of grid to fit our values in
            let sqrtLength = Math.floor(Math.sqrt(cell.length));
            if (sqrtLength * sqrtLength < cell.length) {
              sqrtLength += 1;
            }
            const littleStep = 0.9 / sqrtLength;
            for (let a = 0; a < sqrtLength; ++a) {
              for (let b = 0; b < sqrtLength; ++b) {
                if (a * sqrtLength + b < cell.length) {
                  cells[i][j]
                    .text(String(cell[a * sqrtLength + b]))
                    .font({ size: littleStep })
                    .transform({
                      translateX: 0.1 + b * littleStep,
                      translateY: (a + 1.2) * littleStep,
                    })
                    .id(`D_${j}_${i}_${cell[a * sqrtLength + b]}`);
                }
              }
            }
          }
        }
      }
    }
  }
}

window.PuzzleDraw = PuzzleDraw;
window.SVG = SVG;
let puzdraw = new PuzzleDraw({});

let j = JSON.parse(`
          {
  "$schema": "puzschema.json",
  "type": "sudoku",
  "puzzle": {
    "width": 9,
    "height": 9,
    "startGrid": [
      [null, null, 3, null, [1,2,3,4], null, [1,2,3,4,5,6,7], null, null],
      [9, null, null, 3, null, 5, null, null, 1],
      [null, null, 1, 8, null, 6, 4, null, null],
      [null, null, 8, 1, null, 2, 9, null, null],
      [7, null, null, null, null, null, null, null, 8],
      [null, null, 6, 7, null, 8, 2, null, null],
      [null, null, 2, 6, null, 9, 5, null, null],
      [8, null, null, 2, null, 3, null, null, 9],
      [null, null, 5, null, 1, null, 3, null, null]
    ],
    "cages": [
        [0,0,1,1,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0],
        [0,0,0,0,0,0,0,0,0]
    ]
  }
}`);

let sudoku = puzdraw.drawPuzzle(j);
let puzzle = document.getElementById("puzzle");
sudoku.svg.addTo(puzzle).size(300, 300);
