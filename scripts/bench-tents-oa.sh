#!/usr/bin/env bash
# Run tents benchmarks with --only-assign for comparison.
set -eu

OUTDIR="${BENCH_OUTDIR:-/tmp/bench-tents-oa}"
BINARY="/Users/caj/files/reps/demystify/rust/demystify-rs/target/release/demystify"
EPRIME="eprime"
INSTANCES="01 02 03 04 05 06 07 08 09 10 11 12 13 14 15 16 17 18 19 20"
EXTRA_ARGS="${BENCH_EXTRA_ARGS:-}"

mkdir -p "$OUTDIR"

run_one() {
    local name="$1" instance="$2" method="$3" model="$4" param="$5" extra="${6:-}"
    local method_flag
    case "$method" in
        standard)  method_flag="--mus-method mus" ;;
        core)      method_flag="--mus-method core" ;;
        core+mus)  method_flag="--mus-method core+mus" ;;
    esac
    local tag="${name}-${instance}.${method}"
    local outfile="$OUTDIR/$tag"
    local time_flag="-l"
    [ "$(uname)" = "Linux" ] && time_flag="-v"

    /usr/bin/time $time_flag \
        "$BINARY" --model "$model" --param "$param" \
        $method_flag --log progress --only-assign $EXTRA_ARGS $extra \
        > "${outfile}.stdout" 2> "${outfile}.stderr" || true

    python3 -c "
import re
text = open('${outfile}.stderr').read()
wall = re.search(r'([\d.]+)\s+real', text)
user = re.search(r'([\d.]+)\s+user', text)
syst = re.search(r'([\d.]+)\s+sys', text)
rss  = re.search(r'(\d+)\s+maximum resident set size', text)
if not wall:
    wall = re.search(r'Elapsed.*?:\s*([\d:.]+)', text)
    user = re.search(r'User time.*?:\s*([\d.]+)', text)
    syst = re.search(r'System time.*?:\s*([\d.]+)', text)
    rss  = re.search(r'Maximum resident.*?:\s*(\d+)', text)
d = {}
if wall: d['wall'] = wall.group(1)
if user: d['user'] = user.group(1)
if syst: d['sys']  = syst.group(1)
if rss:  d['rss_kb'] = rss.group(1)
conj = re.search(r'conjure/savilerow completed in ([\d.]+)s', text)
setup = re.search(r'parse setup completed in ([\d.]+)s', text)
solve = re.search(r'solve completed in ([\d.]+)s', text)
if conj:  d['t_conjure'] = conj.group(1)
if setup: d['t_setup'] = setup.group(1)
if solve: d['t_solve'] = solve.group(1)
for k, v in d.items():
    print(f'{k}={v}')
" > "${outfile}.time"
}

for diff in 6x6-easy 6x6-hard 10x10-easy 10x10-hard; do
    echo -n "  tents-oa-${diff} "
    for nn in $INSTANCES; do
        param="${EPRIME}/tents/puzzle-tents-com/${diff}-${nn}.param"
        if [ ! -f "$param" ]; then
            echo -n "?"
            continue
        fi
        run_one "tents-oa-$diff" "$nn" standard "$EPRIME/tents.eprime" "$param"
        run_one "tents-oa-$diff" "$nn" core "$EPRIME/tents.eprime" "$param"
        run_one "tents-oa-$diff" "$nn" "core+mus" "$EPRIME/tents.eprime" "$param"
        echo -n "."
    done
    echo " done"
done

echo "Done. Results in $OUTDIR"
