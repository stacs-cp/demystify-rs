# Star Battle puzzles from puzzle-star-battle.com

These 25 puzzles were scraped on 2026-04-15 from
<https://www.puzzle-star-battle.com/> by fetching the `?size=<n>`
pages five times per category and extracting the `var task = '...'`
JavaScript variable (row-major comma-separated cage IDs).  Each is an
algorithmically-generated instance.

Use for benchmarking; attribute to <https://www.puzzle-star-battle.com/>.

## Category summary

| Category | Board | Stars/region | Difficulty | URL parameter |
|---|---|---|---|---|
| 8x8/1-hard   | 8x8   | 1 | Hard   | `?size=4` |
| 10x10/2-normal | 10x10 | 2 | Normal | `?size=5` |
| 10x10/2-hard | 10x10 | 2 | Hard   | `?size=6` |
| 14x14/3-normal | 14x14 | 3 | Normal | `?size=7` |
| 14x14/3-hard | 14x14 | 3 | Hard   | `?size=8` |

## Files

Named `<board>-<stars>star-<difficulty>-<NN>.param`.  The site-assigned
puzzle IDs at download time are recorded here for traceability:

| File | Puzzle ID |
|---|---|
| `8x8-1star-hard-01.param` | 10,766,498 |
| `8x8-1star-hard-02.param` | 12,412,965 |
| `8x8-1star-hard-03.param` | 14,728,957 |
| `8x8-1star-hard-04.param` | 14,845,055 |
| `8x8-1star-hard-05.param` | 15,117,930 |
| `10x10-2star-hard-01.param` | 1,932,625 |
| `10x10-2star-hard-02.param` | 11,557,794 |
| `10x10-2star-hard-03.param` | 11,666,744 |
| `10x10-2star-hard-04.param` | 14,081,321 |
| `10x10-2star-hard-05.param` | 2,165,675 |
| `10x10-2star-normal-01.param` | 11,449,731 |
| `10x10-2star-normal-02.param` | 4,343,293 |
| `10x10-2star-normal-03.param` | 6,171,142 |
| `10x10-2star-normal-04.param` | 773,526 |
| `10x10-2star-normal-05.param` | 8,470,197 |
| `14x14-3star-hard-01.param` | 1,695,697 |
| `14x14-3star-hard-02.param` | 1,700,396 |
| `14x14-3star-hard-03.param` | 13,402,952 |
| `14x14-3star-hard-04.param` | 19,076,338 |
| `14x14-3star-hard-05.param` | 5,507,606 |
| `14x14-3star-normal-01.param` | 1,691,303 |
| `14x14-3star-normal-02.param` | 3,759,456 |
| `14x14-3star-normal-03.param` | 5,050,615 |
| `14x14-3star-normal-04.param` | 6,933,941 |
| `14x14-3star-normal-05.param` | 909,138 |
