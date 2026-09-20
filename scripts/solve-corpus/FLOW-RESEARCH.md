# Flow, insight and puzzle structure

Research note, 13 September 2026. This proposes extensions to the
[cheapest-route statistics](STORY.md); it does not change the solver or claim
that the proposed measures are validated predictors of puzzle quality.

Implementation follow-up: the analyser now includes payoff and tail bounds,
local commutation and reconvergence, random tie-breaking expectations, and
constraint-family/display-target proxies. See [implemented definitions and
metadata coverage](STORY.md#payoff-pacing-and-metadata-proxies). Human strategy
recognition and global causal decomposition remain research work.

## What the evidence suggests

The useful distinction is between difficulty, absorption, discovery, and
satisfaction. We should retain separate measurements for these experiences
until player data establish which graph properties predict them.

### The closest empirical precedent matches our solving policy

Pelánek's Sudoku study evaluates a model which repeatedly applies a simplest
available deduction, choosing randomly among ties. Both individual deduction
complexity and the availability of next steps contribute to predicting human
solving time. Dependency structure matters even in puzzles requiring only basic
techniques. This is a particularly close precedent for our cheapest-move DAG,
although its human techniques are not MUS cardinalities and its outcome is
solving time, not enjoyment. [Pelánek, 2014](https://arxiv.org/html/1403.7373)

### Flow depends on the player and on more than difficulty

A meta-analysis of 28 studies found a moderate association between
challenge–skill balance and flow, with clear goals and sense of control also
contributing. It spans multiple activities, so it does not establish an ideal
difficulty curve for logic puzzles. Only the abstract was accessible for this
review. [Fong, Zaleski & Leach, 2015](https://doi.org/10.1080/17439760.2014.967799)

In a Candy Crush experiment with 60 retained participants, easy levels produced
less flow; regular and hard levels produced comparable flow despite greater
frustration in the hard condition. This supports measuring frustration and
satisfaction separately from absorption. Transfer is limited: this was a
different kind of puzzle, and participants whose easy/hard results failed the
experimental manipulation were excluded.
[Larche & Dixon, 2020](https://real.mtak.hu/138655/1/article-p606.pdf)

An alternative computational theory relates flow to the mutual information
between actions and desired outcomes, supported by five experiments. It directs
attention toward the relationship between action and meaningful progress. It
does not establish that a deduction graph's branching factor or information
gain is a flow score. [Melnikoff, Carlson & Stillman, 2022](https://www.nature.com/articles/s41467-022-29742-2)

### Insight can be part of the reward

A large survey of cryptic-crossword solvers found that moments of insight were
an important motivation. Respondents often reported stronger insight after
struggle and benefits from setting a clue aside. These are retrospective reports
from a different puzzle population; they do not show that deliberately adding
frustration improves a puzzle. [Friedlander & Fine, 2018](https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2018.00904/full)

Jonathan Blow describes designing puzzles to communicate ideas clearly and
avoiding irrelevant distractions. This is practitioner advice that suggests
examining what a puzzle teaches and how legibly it presents that idea.
[Blow interview, 2016](https://time.com/4355763/the-witness-jonathan-blow-interview/)

### Parallelism can support good pacing

Ron Gilbert explicitly uses puzzle dependency charts to balance restrictive
linearity against an overwhelming number of open problems. His preferred
adventure-game structure repeatedly opens several branches and brings them
together again. This is design experience, not an experimental result for grid
puzzles; the useful hypothesis is that opening and convergence may matter more
than simply maximizing or minimizing width.
[Gilbert, 2014](https://grumpygamer.com/puzzle_dependency_charts/)

Gilbert also argues for understandable goals, incremental rewards, and puzzles
whose solutions advance the experience. For our purposes, that motivates
looking at what a difficult step accomplishes and whether the remaining work
adds anything interesting. [Gilbert, original essay 1989](https://grumpygamer.com/why_adventure_games_suck/)

### Solver effort still needs human calibration

A recent Nonogram preprint studied 67 participants and six puzzles. Its tested
SAT counters did not significantly correlate with perceived difficulty. The
small number of puzzles limits the conclusion, and the study did not test MUS
size. It is a useful reminder to validate our explanatory measures rather than
substitute solver runtime for human effort.
[He, Ju, Calver & Gao, 2026; preprint](https://arxiv.org/html/2608.23300v1)

## Proposed measurements

These are our operational hypotheses inspired by the sources above. None is
automatically a quality score. All route analysis should initially retain the
existing rule that the player selects a cheapest discovered move.

### 1. Payoff after a difficult deduction

For each hard move, measure three separate things:

- Immediate progress: explicit deductions and automatic closure, counted
  separately in literals and affected cells.
- New opportunities: how much unresolved work has become available below each
  difficulty threshold because its best discovered proof became cheaper.
- Following progress: the min/max length and amount of work in the cheaper
  stretch before the next hard move, across permitted continuations.

Reuse `c(s,L)`, the smallest discovered proof size for literal L at state s.
For an edge from s to t and an easy threshold h, immediate newly available work
is the set of still-unresolved literals in t for which
`c(t,L) <= h < c(s,L)`. Missing evidence is treated as infinity for this
calculation and explicitly labelled as absence of a discovered proof. This
cannot certify that a deduction was previously unavailable to a human.

Keep work already available in s separate from newly released work. A long easy
stretch after a hard move can be clearing an old backlog. Release ancestry can
attribute parts of the stretch to the move under our existing path-specific
definition, but is not a counterfactual proof that the move alone caused it.

This lets us distinguish a hard step followed by a productive cascade from a
hard step followed immediately by another hard barrier. Both could be good;
they tell different stories.

### 2. Routine work at the end, and breathing space between hard moves

For every hard threshold k, calculate the min/max number of moves and newly
settled cells after the last move of size at least k. Report routes containing
no such move separately. Also measure cheap intervals between successive hard
moves and the position of the last hard move as a fraction of route progress.

A long easy ending might feel like a satisfying collapse or like bookkeeping.
Its length alone cannot distinguish those experiences. Pair it with release
ancestry, batch yield, and eventually proof novelty. Use a neutral name such as
`routine_tail`, rather than embedding a judgment such as `boring_tail`.

### 3. New ideas, repetition and combinations of ideas

Three large MUSes could require three different insights, or repeat one idea
three times. Our current hard-move counts cannot distinguish these cases.

Start with inspectable proof-family labels, using named strategies where
available. Later, compare constraint/deduction structures under renaming and
board symmetries. Record first appearances, repeated applications, spacing
between repetitions, and hard moves requiring a previously unseen family.
Canonicalization must preserve meaningful rule and geometric relationships;
MUS size or shared constraint IDs alone cannot identify a human technique.

The learning hypothesis is that an idea's first use may be demanding while
later uses feel fluent. Annotate existing cheapest routes first. A learned
cost model would change which moves count as cheapest and needs a separate,
explicit experiment.

### 4. Independent places to work and their convergence

Extend successor width with evidence about independence:

- Local commuting diamonds: two different moves can be performed in either
  order, both orders remain legal cheapest routes, and they reach the same
  state.
- Larger groups of deductions whose internal progress can be interleaved;
  record their sizes, overlap, and how long multiple groups stay available.
- Convergence: where branches combine to enable a shared later deduction,
  followed by renewed opening of choices.

Diamonds show local order flexibility. They do not by themselves prove global
causal independence. In particular, doing one move can release a cheaper move
and temporarily make another formerly cheapest move ineligible; a missing
two-step diamond is not proof of logical dependence.

Keep three questions distinct: how many choices exist, how independently those
choices can develop, and how much the choice changes later hard work. We
already measure the third through best/worst hard-move counts. The first two
could distinguish useful freedom from many interchangeable clicks.

### 5. Finding a deduction versus understanding its proof

Equal MUS cardinality can conceal very different visual searches and proof
structures. Candidate features to inspect include:

- The number of cells with a cheapest deduction relative to unresolved cells.
- The spatial span and number of separate regions touched by the explanation.
- The rule types combined and whether the inference has a familiar name.
- Changes of working region along a route, including revisits after releases.

These are discoverability hypotheses, not established cognitive difficulty
metrics. Visual layout and a player's repertoire are additional inputs; the
SAT encoding alone cannot describe them. Count cells and literals separately,
since domains and assignment/elimination policies vary between puzzle types.

### 6. Typical experiences alongside extreme routes

Keep exact min/max bounds, and add an explicitly named baseline distribution:
uniform selection among distinct cheapest successor states at each step. It
gives probabilities of encountering hard chokes and expected hard-move counts,
without introducing more expensive moves. It is a modeling assumption, not a
claim about actual players.

Uniform local choices do **not** produce a uniform distribution over complete
routes. Store the policy with every probabilistic result, avoid weighting
duplicate proofs as extra choices, and retain existing extrema and witnesses.
For batch-dependent release metrics, a successor policy also needs an explicit
rule for choosing among alternative deduction batches reaching that successor.

## Progress is an epistemic question

An attractive future idea is to measure how much a move resolves uncertainty.
There is a trap: a uniquely solvable puzzle already has one solution under its
full rules and givens. Adding logically entailed facts does not reduce that
solution count. Counting auxiliary SAT assignments would introduce encoding
artifacts rather than player uncertainty.

A meaningful information measure therefore needs a specified bounded player
model: for example, candidate sets after selected simple techniques, or
solutions under a defined relaxation of constraints. Any such measure must
record that model. For now, newly settled cells and newly available deductions
are more transparent proxies for visible progress.

## Suggested order of work and validation

1. Add breakthrough payoff, routine tails, and cheap intervals to the saved
   graph analyser. These most directly extend the existing release and
   hard-move measurements without new MUS searches.
2. Add local commuting/convergence features and an explicit random-tie baseline.
3. Inspect representative proofs to develop family labels and spatial features.
4. Compare player experiences on a small, deliberately varied selection.

The pilot should compare puzzles within a type and board size, matching peak
difficulty and overall work where possible while varying cascade payoff,
independence, or repetition. Record experience with that puzzle type, move
timings, hints, and abandonment. After solving, separately ask about enjoyment,
absorption, frustration, perceived progress, and moments of discovery. A short
route replay can locate remembered breakthroughs without repeatedly
interrupting solving. Treat preliminary associations as exploratory; test
promising predictions on held-out puzzles and players.

For cross-type comparisons, retain raw MUS sizes but do not treat equal values
as calibrated equal human difficulty. Preserve puzzle family, encoding,
generation/search settings and graph hash with results. Repeat a subset with
more MUS-search effort to estimate how much a claimed story depends on missing
proofs. That sensitivity is especially relevant to apparent narrow chokes.

The immediate research questions are whether newly released easy work predicts
satisfaction after a hard step, whether independent fronts reduce getting
stuck, and whether repeated hard proof families feel easier or more tedious
than equally costly new ones. Those questions can be explored without deciding
in advance that linear or parallel puzzles are better.
