# Generate a multi-game solving corpus

These scripts select shipped Bloomsweep puzzles, execute their existing game
encoders against the local WASM builder, and explore solving routes with the
native `demystify-solvetree` executable. The result is a SQLite index plus
self-contained JSON graph archives and offline HTML viewers. No story analysis
or classification is performed during generation.

Once graphs are saved, the [story statistics tool](STORY.md) measures release
chains, choke points, best/worst hard-move counts, breakthrough payoff, routine
tails, local commutation, and metadata proxies over cheapest-move ties:

```sh
python3 -B scripts/solve-corpus/analyse.py --corpus path/to/corpus
```

It writes `statistics/README.md`, per-puzzle JSON and witness reports, and
queryable `story_*` tables in the existing database.

Saved local snapshots: [initial 84 instances](../../etc/example-outputs/solve-graphs/all-games-2026-09-10/README.md)
and [123 additional instances with a full-inventory runtime estimate](../../etc/example-outputs/solve-graphs/more-games-2026-09-13/EXPANSION.md).
Generated corpora are local artifacts, so these links require their saved directories.

Run from the `demystify-rs` workspace. Requirements: Python 3.9+, Node 20+, the
Bloomsweep checkout with its npm dependencies installed (including esbuild),
and the normal Rust/WASM build toolchain.

```sh
make wasm
cargo build --release -p demystify --bin demystify-solvetree

python3 scripts/solve-corpus/prepare.py \
  --apps-root /path/to/bloomsweep-web \
  --out etc/example-outputs/solve-graphs/my-corpus

python3 scripts/solve-corpus/run.py \
  --corpus etc/example-outputs/solve-graphs/my-corpus \
  --workers 2 --seconds 180

python3 scripts/solve-corpus/catalog.py \
  --corpus etc/example-outputs/solve-graphs/my-corpus
```

`prepare.py` refuses to overwrite a manifest. It prefers boards of at most 36
cells, taking low/middle/high available source MUS tiers. When necessary it
admits the smallest board at a missing third tier. Fell includes 4×4 and 4×5;
Gamut includes both variants; Sashes includes both shapes; Bloomsweeper includes
the two difficulty endpoints for all variants. Combination and Poly-pic retain
their pack labels, and Celestial has two shipped examples. Source tiers are
not a calibrated difficulty scale across games. This is a small, selected
research corpus, not a representative sample.

`run.py` exports sequentially, then solves two puzzles concurrently by default.
Each native solve uses two threads and five dynamic MUS repeats. `--seconds`
caps each solve subprocess. The graph is initially saved, then checkpointed
between work units; a timeout loses only work since its latest checkpoint.
Logs and database rows record the exact command, timing, hashes and status.
The underlying solver can automatically increase its initial conflict limit.

Rerun the same command to resume unfinished graphs; completed graphs are
skipped. Use `--ids ID ID ...` to select instances, or `--export-only` to stop
after model export and uniqueness validation. To search for additional MUSes
without replacing the baseline:

```sh
python3 scripts/solve-corpus/run.py \
  --corpus etc/example-outputs/solve-graphs/my-corpus \
  --refine 20 --seconds 600
```

Refined archives/viewers/logs go into `refined-20` subdirectories and a separate
database phase. Rerun `catalog.py` after a batch has stopped. It verifies saved
file hashes, SQLite integrity, and complete-graph status; it copies encoder
source files and WASM artifacts for provenance and writes a linked catalog.
Run it while the encoder checkout and local WASM still match the export.

## Broader samples and a current inventory

`expand.py` inventories every current level file, including Combination's
array-based packs and newly added games. It writes `inventory.json` with all
source entries and full-JSON content hashes. The default sample takes six new
entries per game, or more when needed to cover every variant/board-size
stratum. Within strata it cycles through seeded shuffled packs and levels.
It includes larger boards to help measure their cost; `--max-area 36` restricts
it to smaller ones. An unknown board size stays unknown.

```sh
python3 -B scripts/solve-corpus/expand.py \
  --apps-root /path/to/bloomsweep-web --out path/to/more-corpus \
  --exclude-corpus path/to/previous-corpus --per-game 6 --seed 20260913

python3 -B scripts/solve-corpus/run.py \
  --corpus path/to/more-corpus --workers 2 --seconds 60
python3 -B scripts/solve-corpus/analyse.py --corpus path/to/more-corpus
python3 -B scripts/solve-corpus/catalog.py --corpus path/to/more-corpus

python3 -B scripts/solve-corpus/estimate.py \
  --inventory path/to/more-corpus/inventory.json \
  --corpus path/to/previous-corpus --corpus path/to/more-corpus \
  --budget 60 --workers 2 --out path/to/more-corpus/estimate.json
```

Exclusions compare frozen input contents, not just filenames. Pass multiple
`--exclude-corpus` arguments to exclude multiple previous selections. `--all`
prepares every eligible remaining source entry instead of sampling; preparation
does not itself run the solvers. Existing manifests are never overwritten.
The full census includes source entries even if later validation discovers
multiple solutions or an encoder cannot be loaded.

Runtime projections reweight observations by game and board-size band. They
use initial build attempts, cap unfinished/failed jobs at the selected budget,
and expose unmeasured strata. Short successful resumes do not become misleading
fresh-build timings. These are estimates of a bounded first pass, not a promise
of completing all uncapped graphs. Inputs must still match the current
inventory. The sample is intentionally stratified, with few observations in
some groups, so the projections are approximate rather than confidence bounds.

Finishing every uncapped graph can be much harder than a bounded first pass.
With n independent, equally cheap deductions, a state DAG can still contain
all 2^n subsets: 20 such deductions already give 1,048,576 states. Merging
equivalent orders avoids enumerating n! complete routes, but does not remove
this exponential state growth. MUS difficulty alone is therefore a poor
runtime predictor. Checkpoints preserve expensive cases for selective retries.

The runner can resume a checkpoint after story statistics have been indexed;
updating a run preserves analysis foreign keys. Its new hash/status makes older
statistics stale until `analyse.py` is rerun. The exporter can resolve shared
workspace modules for new apps before their node_modules links are installed.

## Full passes and overall distributions

`sweep.py` runs a durable first pass over a prepared corpus, interleaving seeded
shuffled queues from each game. It exports, solves and analyses chunks, then
refreshes `overall/README.md`, `overall/overall.json` and `overall/overall.sqlite`
across the selected corpora. It records live counters and its PID in
`progress.json`. The finished pass also writes a verified generation catalog.

```sh
python3 -B scripts/solve-corpus/sweep.py \
  --corpus path/to/full-corpus \
  --include-corpus path/to/previous-corpus \
  --include-corpus path/to/more-corpus \
  --workers 4 --seconds 60 --chunk-size 40
```

The 13 September full pass is saved in
[full-pass-2026-09-13](../../etc/example-outputs/solve-graphs/full-pass-2026-09-13/SWEEP.md).
Its launcher runs separately from the terminal and keeps idle sleep disabled
only while the pass is running. The command above itself runs in the foreground;
use a persistent terminal or a detached launcher for unattended work.

Restarting `sweep.py` continues unattempted/interrupted jobs and catches up on
missing analyses. It does **not** grant another budget to every saved checkpoint.
Use `run.py --ids ...` later for deliberate retries, after the sweep has stopped.
A file lock prevents duplicate sweeps of the same corpus; do not concurrently
run a different writer against that corpus. Native and WASM builds are copied
and fixed for the pass. Encoder input JSON is frozen, while encoder sources are
still read from the configured app checkout. The pass pauses before a chunk if
free disk space falls below `--min-free-gib` (default 10), and caps each analysis
chunk at 10 minutes; failures remain visible in progress and coverage.

To regenerate combined statistics separately:

```sh
python3 -B scripts/solve-corpus/overall.py \
  --inventory path/to/full-corpus/inventory.json \
  --corpus path/to/previous-corpus --corpus path/to/more-corpus \
  --corpus path/to/full-corpus --out path/to/full-corpus/overall
```

The derived database has one `puzzles` row per current source entry, scalar
metrics, source-corpus/report references, and a `distributions` table with
sample counts, quartiles, p10/p90 and extrema. Distributions split by game,
game/board-size band, game/source pack and game/source tier. Current contents
must match the frozen input; duplicate observations do not double-count a
source entry. Only current completed-graph analyses contribute story values.
Missing or unfinished results remain null, with separate coverage counts.

Every puzzle has equal weight in the per-game distributions. Within a puzzle,
choke/independence fractions use expected route encounters under uniform
cheapest-successor choices, avoiding a bias towards states on large branches.
They are ratios of expectations, not extrema or the expected fraction of an
individual route. Cross-game MUS scales and metadata profiles are not calibrated,
and completion bias must be considered before drawing population conclusions.

## Encoders and givens

Most games use `apps/GAME/src/game/demystify-encode.ts`. Bloomsweeper uses its
existing solver builder, with initially known cells passed as pins. Its reveal
rules survive the export. Combination uses the shared `combination-core`
builder with a small adapter for the app's board/piece formats. Celestial has
no app MUS encoder; its adapter follows the board validator's row/column/region
star counts and moon adjacency rules. Its two shipped examples have multiple
solutions and are explicitly marked `underdetermined`.

The exported `WasmPuzzle.toJson()` contains the full native model and CNF,
including constraint metadata and reveal rules. Native graph generation needs
neither Node nor the app checkout after export. Uniqueness is checked against
the exported game encoding; this is not an independent validation of all game
rules or a comparison with stored reference solutions.

## Files and database

- `manifest.json`: selection, source paths/indices/hashes, source difficulty.
- `inputs/`: frozen source level JSON and any given-cell pins.
- `parsed/`: native puzzle models exported from the app encoders.
- `validation/`: uniqueness results, fixed/unfixed variables, and model counts.
- `graphs/`: resumable JSON archives with models, proofs and all retained states.
- `viewers/`: self-contained offline HTML viewers.
- `logs/`: commands, progress and errors.
- `sources/`: encoder bundles, source snapshots, WASM and build provenance.
- `corpus.sqlite`: `games`, `puzzles`, `runs`, `attempts`, and the
  `completed_graphs` view.

Keep the directory together: the database stores relative paths to the graph
files. `runs.summary_json` holds native summary fields and `settings_json`
records search settings. `difficulty_max` is the peak MUS on currently active
edges; `source_tier` is the original pack's label. `board_area` is a size proxy
and may be a bounding rectangle for hex boards.

`runs` holds the latest result per puzzle/pass. `attempts` retains timings and
summary metadata for previous attempts, so resuming does not erase time already
spent. An attempt's archive hash describes that checkpoint at that time; the
file at its path advances on resume. Only the latest `runs` hash is expected to
match the current file. Sum `attempts.seconds` for total search/packaging cost.

Statuses distinguish complete fixed points, unfinished checkpoints,
underdetermined puzzles, incomplete MUS searches, and failures. A completed
graph explores the current easiest-move policy to a fixed point of discovered
proofs; it does not certify globally minimum MUSes. Use the `completed_graphs`
view for future comparisons of complete routes, and compare each game's own
tiers before making claims across different constraint encodings.

All generated data belongs under the gitignored `etc/example-outputs/` tree.
Back up the entire corpus directory if it should outlive this checkout.
