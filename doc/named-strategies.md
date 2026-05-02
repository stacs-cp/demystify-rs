# Named-strategy recognition

Demystify produces a sequence of MUSes (Minimal Unsatisfiable Subsets) that
justify each deduction. Without further context they appear as anonymous
lists of constraints. The named-strategy system labels each MUS with a
human-recognised technique name (e.g. "Row hidden single", "Column naked
pair") when the MUS structure matches a known pattern.

## How it works

1. **Fingerprint.** Each MUS is reduced to a canonical-form string that
   captures the *shape* of the constraint graph: which constraint families
   are involved, and how their variable scopes overlap. Two MUSes share a
   fingerprint iff they are isomorphic (with edge weights quantised to
   {1, 2, ≥3}).
2. **Lookup.** A per-puzzle-kind TOML database maps fingerprints to
   technique names. The DB lives at `demystify/named-strategies/<KIND>.toml`
   where `<KIND>` matches the `$#KIND` directive in the .eprime model.
3. **Display name.** When a strategy entry sets `orientation_group`, the
   display logic finds MUS constraints whose family belongs to that group
   and prefixes the technique name with the group-relative member label —
   so a single DB entry covers Row/Column/Box variants.

## Annotating an `.eprime` model

Two directives are involved.

### `$#FAMILY` — declare a family-group with member labels

```
$#FAMILY <group-id> ["group label"] <member-id1> ["label1"] <member-id2> ["label2"] ...
```

Quoted strings are *labels*; bare tokens are *identifiers*. Labels are
optional and default to the identifier.

A constraint family may belong to multiple groups (e.g. `row_alldiff` is in
both `unit_alldiff` and `line_alldiff`). Labels are *group-relative*: the
same member id can have a different label depending on which group it is
viewed through (`row_alldiff` is "Row" within `unit_alldiff`, but could be
"AllDiff" within a hypothetical `row_things` group).

Example from `eprime/sudoku.eprime`:

```
$#FAMILY unit_atmost   "Unit at-most"  row_atmost   "Row" con_atmost   "Column" box_atmost   "Box"
$#FAMILY line_atmost   "Line at-most"  row_atmost   "Row" con_atmost   "Column"
$#FAMILY unit_contains "Unit contains" row_contains "Row" con_contains "Column" box_contains "Box"
$#FAMILY line_contains "Line contains" row_contains "Row" con_contains "Column"
```

## The strategy database

One TOML file per `$#KIND` at `demystify/named-strategies/<KIND>.toml`.

```toml
[[strategy]]
name = "hidden single"
fingerprint = "row_contains;"
orientation_group = "unit_contains"
```

- **`name`** — the human-readable technique name. The orientation prefix is
  prepended at display time, so write the bare name ("hidden single", not
  "Row hidden single").
- **`fingerprint`** — the canonical-form string emitted by the
  fingerprinter. Copy verbatim from the JSON / HTML output of a planner
  run.
- **`orientation_group`** *(optional)* — names the `$#FAMILY` group whose
  member labels prefix the display name. The display logic scans the MUS
  for constraints whose family is in that group, takes the unique label,
  and prepends it. If multiple constraints disagree on label (shouldn't
  happen for a well-tagged technique), the prefix is dropped defensively.

## The fingerprint format

`<families>;<edges>` — a canonical serialisation of the constraint-graph
shape. For example:

| fingerprint | meaning |
|---|---|
| `row_contains;` | one `row_contains` atom (no edges → single-node graph) |
| `row_atmost,row_atmost;0-1:3` | two `row_atmost` atoms with a ≥3-cell overlap |
| `con_contains,con_contains,con_contains;0-1:3,0-2:3,1-2:3` | three column-contains atoms, fully connected, all overlaps ≥3 — i.e. a hidden triple in a column |

Edge weights are quantised to {1, 2, 3} where 3 means "≥3 shared
variables". This makes fingerprints stable across puzzle sizes (the same
technique on a 4×4 sudoku and a 9×9 sudoku produces the same fingerprint).

The number of literals deduced is *not* part of the fingerprint —
the same logical technique may eliminate 1, 4, or 14 candidates depending
on context, and folding that into the fingerprint forces unnecessary DB
duplication.

## Curating the database

The end-to-end loop:

1. Run the planner on a representative puzzle:
   ```sh
   cargo run --release --bin demystify -- \
       --model eprime/sudoku.eprime --param eprime/sudoku/<your-puzzle>.param \
       --json | jq '.[] | select(.name == null) | .fingerprint'
   ```
   This lists fingerprints of MUSes that didn't match any DB entry.

2. Inspect the constraint listing of a fingerprint you'd like to name. The
   HTML output (`--html`) shows each fingerprint as a small `fp: ...` line
   below the matched technique name (or in place of it for unnamed MUSes).

3. Add an entry to the appropriate `<KIND>.toml`:
   ```toml
   [[strategy]]
   name = "your technique name"
   fingerprint = "<paste fingerprint here>"
   orientation_group = "unit_atmost"   # if applicable
   ```

4. Re-run; the new name should appear in the output for matching MUSes.

## Limitations

- **Rich constraints are out of scope.** The fingerprint captures
  *between-constraint* structure. When a single constraint is itself rich
  (Killer cages, nonogram lines, Sandwich), the named technique often lives
  *inside* the constraint and won't be recognisable from the inter-atom
  graph. A separate per-constraint analyser would be needed.
- **Brute-force canonicalisation caps at 8 atoms.** Above that, the
  fingerprinter falls back to a degree-sequence hash that may alias some
  graphs together. No incorrect names are produced — at worst, a
  larger-than-8-atom MUS fails to find its DB entry.
- **Edge weights are state-dependent.** When earlier deductions have fixed
  some variables, those variables drop out of constraints' live scopes, and
  the overlap between two constraints may shrink. The same logical
  technique can therefore produce slightly different fingerprints depending
  on puzzle state. Mitigation: add multiple DB entries covering the
  observed variations.
