# Cake partitioning and greedy minimisation for MUS extraction

## Background

Given a set of constraints $C$ and a target literal $\ell$, a *minimal unsatisfiable subset* (MUS) is a smallest subset $M \subseteq C$ such that $M \cup \{\neg\ell\}$ is unsatisfiable and no proper subset of $M$ has this property.  SAT solvers can cheaply produce *unsatisfiable cores* — subsets of the assumptions that participated in the proof of unsatisfiability — but these cores are far from minimal.

In the Demystify system, the constraint set $|C|$ for a typical puzzle instance is large (tens of thousands), while MUS sizes are small (1–5 constraints in practice).

## The problem: loose SAT cores

When asked to prove that a literal $\ell$ is implied by $C$, the SAT solver returns an UNSAT core that is a subset of $C \cup \{\neg\ell\}$.  In principle, this core could be close to the MUS size.  In practice, cores are vastly larger.

**Empirical data (miracle sudoku, step 17, 30,784 constraint literals):**

| Statistic | Value |
|-----------|-------|
| Constraint set size | 20,523 |
| Median initial core size | 2,451 |
| Mean initial core size | 2,334 |
| Min initial core size | 4 |
| Max initial core size | 3,834 |
| Actual MUS size (step 17) | 3 |

The SAT solver returns cores roughly 800× larger than the MUS.

## Greedy minimisation (element-by-element)

The standard approach to minimise an UNSAT core is greedy deletion: iterate over the core elements, try removing each one, and keep the removal if the remaining set is still unsatisfiable.

```
greedy_minimise(core, max_size):
    known_necessary = []
    for each element e in core:
        if core \ {e} is UNSAT (with new core from solver):
            core ← new core       # e was redundant, shrink
        else:
            known_necessary.append(e)
            if |known_necessary| = max_size:
                return known_necessary if UNSAT, else None
    return core
```

Each iteration makes one SAT call.  With an initial core of $n$ elements and a MUS of size $k$, this requires up to $n$ SAT calls before confirming $k$ necessary elements and exiting early.  When $n \approx 2500$ and $k = 3$, the algorithm processes on average $\sim\!n/2$ elements before finding all $k$ necessary ones.

**Empirical cost (miracle sudoku, old algorithm):**  A single greedy minimisation call on one chunk of 20,523 constraints with an initial core of 3,522 elements took **468 seconds**.  The worst observed single call took **1,406 seconds**.

## Cake partitioning (bulk shrinking)

The *cake-cutting* strategy exploits the fact that when searching for a MUS of size $\le k$, we can partition the core into $k+1$ groups and, by the pigeonhole principle, at least one group contains no MUS elements.  Removing that group preserves all MUS elements, and the SAT solver returns a new (smaller) core over the reduced set.

```
cake_shrink(core, max_size):
    num_groups = max_size + 1
    while |core| > 2 × num_groups:
        for i in 0..num_groups:
            remaining = core with group i removed
            result = SAT_solve(remaining)
            if UNSAT (with new core):
                core ← new core     # greedily take first success
                break
        else:
            return None              # every group is needed → MUS > max_size
    return core
```

Key properties:

- **Each iteration makes at most $k+1$ SAT calls** (one per group), compared to up to $n$ calls for element-by-element.
- **Greedy variant**: unlike standard cake-cutting which tests all groups and picks the best, we take the first successful removal and immediately restart the loop with the smaller core.  This saves up to $k$ SAT calls per iteration.
- **Early rejection**: if no group can be removed, we know immediately that no MUS of size $\le k$ exists.  This turns the bulk phase into a fast negative filter — critical when most literals have no small MUS.
- **Terminates at $2(k+1)$**: we stop the bulk phase when the core is small enough for element-by-element to be cheap ($\le 2(k+1)$ elements).

### Analysis

Each successful iteration reduces the core by at least a factor of $k/(k+1)$ (we remove one group of size $\lfloor|C|/(k+1)\rfloor$, and the new core from the solver is a subset of the remainder).  Starting from $n$ elements and stopping at $2(k+1)$, the number of iterations is at most:

$$\lceil \log_{(k+1)/k}(n / (2(k+1))) \rceil$$

For $n = 2500$ and $k = 3$: $\lceil \log_{4/3}(2500/8) \rceil = \lceil \log_{1.33}(312) \rceil \approx 20$ iterations, each with at most 4 SAT calls = **80 SAT calls**.  Compare to ~2,500 SAT calls for pure element-by-element.

### Combined algorithm

In practice, we run cake shrinking first, then element-by-element on the residual:

```
greedy_minimise(initial_core, max_size):
    core = cake_shrink(initial_core, max_size)
    if core is None: return None
    return element_by_element(core, max_size)
```

The element-by-element phase handles at most $2(k+1)$ elements, requiring at most $2(k+1)$ SAT calls.  Total SAT calls: $\sim\!20(k+1) + 2(k+1) = 22(k+1)$.

## The Cake algorithm for MUS search

The outer Cake algorithm for finding a MUS of size $\le k$ in a constraint set $C$:

1. Partition $C$ into $k+1$ groups of roughly equal size.
2. For each group $i$, let $C_i = C \setminus \text{group}_i$ (the complement, containing $k/(k+1)$ of the constraints).
3. For each $C_i$, run `quick_mus(C_i \cup \{\neg\ell\}, \text{max\_size}=k+1)`:
   - Get an initial UNSAT core from the SAT solver.
   - Run `greedy_minimise` (with bulk shrinking) to find a MUS of size $\le k+1$.
4. By pigeonhole, if a MUS of size $\le k$ exists, at least one $C_i$ contains all MUS elements.

This makes at most $k+1$ calls to `quick_mus`.  With the bulk shrinking improvement, each call either rejects quickly (4 SAT calls when no small MUS exists) or finds the MUS efficiently.

## Empirical results

**Miracle sudoku, 18 solve steps, 30,784 constraint literals:**

| | Old (element-by-element only) | New (cake + bulk shrink) |
|---|---|---|
| Step 17 | did not terminate (>30 min) | 309 seconds |
| Total (18 steps) | did not terminate | 363 seconds |
| Total SAT calls (18 steps) | — | 39,244 |
| Worst single quick\_mus call | 1,406 seconds | — |

The improvement comes from two sources:

1. **Fast rejection**: for literals with no MUS of size $\le k$, the bulk phase returns `None` after at most $k+1$ SAT calls.  Previously, greedy minimisation ground through ~2,500 core elements before concluding "not found."

2. **Efficient shrinking**: for literals that do have a small MUS, the bulk phase reduces a 2,500-element core to $\le 2(k+1)$ elements in ~20 iterations of $k+1$ SAT calls each, rather than ~2,500 individual SAT calls.
