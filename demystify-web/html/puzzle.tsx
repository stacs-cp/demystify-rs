import { SVG } from "@svgdotjs/svg.js";
import { assert } from "console";

// G, Path, Line, Text
const _puzzle_decorations = {
  sudoku: { sudoku: true },
};

class PuzzleDraw {
  base_width: number;
  mid_width: number;
  thick_width: number;
  

  constructor({
    base_width = 0.005,
    mid_width = 0.01,
    thick_width = 0.02
  }: {
    base_width?: number;
    mid_width?: number;
    thick_width?: number;
  }) {
    this.base_width = base_width;
    this.mid_width = mid_width;
    this.thick_width = thick_width;
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
      for (const [k, v] of Object.entries(_puzzle_decorations) as [string, any][]) {
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

    const out = this.drawGrid(puzzle, puzjson["decorations"]);

    if ("startGrid" in puzzle) {
      console.log("startGrid");
      this.fillFixedState(out, puzzle["startGrid"]);
    }

    if ("knowledgeGrid" in puzzle) {
      console.log("knowledgeGrid");
      this.fillKnowledge(out, puzzle["knowledgeGrid"]);
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
      this.fillFixedState(out, solutionCpy, { color: "grey" });
    }

    return out;
  }

  drawGrid(puzzle: any, decorations: any) {
    const svg = SVG();
    const topgrp = svg.group();
    topgrp.transform({ translateX: 25, translateY: 25, scale: 450 });

    const grp = topgrp.group();

    const width = puzzle["width"];
    const height = puzzle["height"];

    let cages = null;
    if ("cages" in puzzle) {
      cages = puzzle["cages"];
    }

    let sudoku_decorations = false;
    if ("sudoku" in decorations) {
      sudoku_decorations = true;
    }

    const wstep = 1.0 / width;
    const hstep = 1.0 / height;

    const colours_list = ["#FFB3B3", "#B3B3FF", "#FFFFB3", "#B3FFB3", "#E6B3FF"];
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
            const p = SVG().path(path).fill(colours_list[col]);

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
            stroke = this.mid_width;
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
              linecap: "round",
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
            stroke = this.mid_width;
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
        cells[j][i].id(`C_${j}_${i}`);
        cells[j][i].transform({ translateX: itrans, translateY: jtrans });
        cells[j][i] = cells[j][i].group();
        cells[j][i] = cells[j][i].transform({ scale: wstep * 0.9 });
      }
    }

    return { svg: svg, cells };
  }

  fillFixedState(gridobj: any, contents: any, { color = "black" } = {}) {
    const cells = gridobj["cells"];
    console.log(contents);
    for (let i = 0; i < contents.length; i++) {
      for (let j = 0; j < contents[i].length; j++) {
        const cell = contents[i][j];
        if (cell) {
          const s = String(cell);
          cells[i][j]
            .text(s)
            .font({ size: 1 })
            .transform({ translateX: 0.2, translateY: 0.9 });
        }
      }
    }
  }

  fillKnowledge(gridobj: any, contents: any, { color = "black" } = {}) {
    const cells = gridobj["cells"];
    console.log("fillKnowledge:", contents);
    for (let i = 0; i < contents.length; i++) {
      for (let j = 0; j < contents[i].length; j++) {
        const cell = contents[i][j];
        if (cell) {
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


//window.PuzzleDraw = PuzzleDraw;
//window.SVG = SVG;

async function setupPuzzle() {
  let puzdraw = new PuzzleDraw({});

  let request = await fetch("sudoku.json");
  console.log(request);
  let j = await request.json();
  console.log(j);

  let sudoku = puzdraw.drawPuzzle(j);
  let puzzle = document.getElementById("puzzle");
  if (puzzle) {
    sudoku.svg.addTo(puzzle).size(500, 500);
  }
}


async function checkServer() {
  console.log(" --- Testing server");
  let request = await fetch("/greet");
  console.log(request);
  let j = await request.text();
  console.log(j);
  console.log(" --- Server test")

}

console.log("-- Step 1");
setupPuzzle();
console.log("-- Step 2");
checkServer();
console.log("-- Step 3");
