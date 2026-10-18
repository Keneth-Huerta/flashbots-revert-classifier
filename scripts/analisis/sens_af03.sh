#!/usr/bin/env bash
# Sensibilidades preregistradas de AF-03 (no deciden). Uso: sens_af03.sh <unichain|op>
# Solo lee filas y crudo local; no llama al RPC. Salidas en artefactos/af03/sens_*.txt
set -u
c="$1"
B=target/release/mev-revert-classifier
D=artefactos/af03
RAW=artefactos/rpc_af03/$c
ANC=datos/muestra_bloques_af03_${c}_anclas.csv
ALL=datos/muestra_bloques_af03_${c}.csv
ROWS=$D/filas_${c}.csv
SINV4=$D/noV4/filas_${c}_sinv4.csv
COM="--chain $c --split confirmatory --seed 20261017"
R="$B race $COM --raw $RAW --blocks $ANC --bootstrap 1000 --permutations 1000 --sin-probes-only"
N="$B neighbor $COM --blocks $ALL --bootstrap 1000"
t() { echo "inicio $1 $(date -u +%FT%TZ)"; }
t sinv4;     $R --rows $SINV4 --boot-unit week > $D/sens_${c}_race_sinv4.txt 2>&1
$N --rows $SINV4 --boot-unit week > $D/sens_${c}_neighbor_sinv4.txt 2>&1
t nosys;     $R --rows $ROWS --boot-unit week --exclude-system false > $D/sens_${c}_race_nosys.txt 2>&1
$N --rows $ROWS --boot-unit week --exclude-system false > $D/sens_${c}_neighbor_nosys.txt 2>&1
t bins50;    $R --rows $ROWS --boot-unit week --bins 50 > $D/sens_${c}_race_bins50.txt 2>&1
t cellblock; $R --rows $ROWS --boot-unit week --cell-block > $D/sens_${c}_race_cellblock.txt 2>&1
t boothour;  $R --rows $ROWS --boot-unit hour > $D/sens_${c}_race_boothour.txt 2>&1
t deg10;     $R --rows $ROWS --boot-unit week --max-sender-degree 10 > $D/sens_${c}_race_deg10.txt 2>&1
t nbr_drop;  $N --rows $ROWS --boot-unit week --drop-blocks-without-rows > $D/sens_${c}_neighbor_drop.txt 2>&1
t nbr_hour;  $N --rows $ROWS --boot-unit hour > $D/sens_${c}_neighbor_boothour.txt 2>&1
echo "fin $(date -u +%FT%TZ)"
