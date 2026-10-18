#!/usr/bin/env bash
# Descarga de AF-03 (preregistro 2026-10-18-preregistro-af03.md, seccion Ejecucion): fetch por cadena
# sobre la muestra completa con el sello. Lee .env al entorno sin imprimirlo.
cd "$(dirname "$0")/../.."
set -a; [ -f .env ] && . ./.env; set +a
S=docs/04_investigacion_y_labs/2026-10-18-sello-af03.sha256
for c in op unichain; do
  echo "inicio $c $(date -u +%FT%TZ)"
  target/release/mev-revert-classifier fetch --chain $c --v4-pools --blocks datos/muestra_bloques_af03_$c.csv --split confirmatory --seal $S --post-seal --raw-dir artefactos/rpc_af03/$c --out artefactos/af03/filas_$c.csv
  echo "fin $c rc=$? $(date -u +%FT%TZ)"
done
