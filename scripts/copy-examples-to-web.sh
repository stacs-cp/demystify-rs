#!/usr/bin/env bash

rm -rf demystify-web/examples/
mkdir demystify-web/examples/
for i in \
    eprime/sudoku.eprime \
    eprime/sudoku/puzzlingexample.param \
    eprime/miracle.eprime \
    eprime/miracle/original.param \
    eprime/star-battle.eprime \
    eprime/star-battle/FATAtalkexample.param \
    eprime/binairo.essence \
    eprime/binairo/diiscu.param
do
    cp --parents "$i" demystify-web/examples/
done
