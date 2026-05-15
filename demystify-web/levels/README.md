# Game-mode levels

Each subdirectory contains one playable level: a model file (`puzzle.eprime` or
`puzzle.essence`) and a parameter file (`puzzle.param`). The order, display
names and difficulty markers are defined in code in `demystify-web/src/game.rs`
(constant `LEVELS`).

The files here are bundled into the binary at compile time via `include_str!`,
so adding a new level requires both adding the files and adding an entry to the
`LEVELS` table.
