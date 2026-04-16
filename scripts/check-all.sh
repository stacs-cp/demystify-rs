#!/usr/bin/env bash

for i in $(cd eprime; ls *.e*); do
    dir="${i%.*}"
    for f in $(cd eprime/$dir; ls *.param); do
        for flags in "" "-html"; do
            mkdir -p "etc/example-outputs/$i"
            echo cargo run --release --bin main -- --model "eprime/$i" --param "eprime/$dir/$f" --merge 100 $flags \> "etc/example-outputs/$i/$f$flags.stdout" 2\> "etc/example-outputs/$i/$f$flags.stderr"
        done
    done
done
