#!/usr/bin/env bash
# AF-04: corridas preregistradas sobre las 48 semanas ya vistas de 2025 (T6, T7, T8, T9
# retrospectivo, T10 y la concordancia de T1 a T5). Comandos del preregistro
# (docs/04_investigacion_y_labs/2026-10-18-preregistro-af04.md); salidas en artefactos/af04/vistas/.
# Requires the rows and receipts of AF-02 in artefactos/af02/ and artefactos/rpc_af02/; T10 also
# requires the AF-02 traces in artefactos/rpc_af02/, re-extracted without RPC.
# Uso: bash scripts/analisis/correr_af04_vistas.sh [prueba ...]   (sin argumentos: todas menos T10)
set -uo pipefail
cd "$(dirname "$0")/../.."
B=target/release/mev-revert-classifier
O=artefactos/af04/vistas
R=artefactos/af02/rows.csv
BL=datos/muestra_bloques_af02_anclas.csv
SELLO=docs/04_investigacion_y_labs/2026-10-18-sello-af04.sha256
mkdir -p "$O"
C=(race --raw artefactos/rpc_af02 --blocks "$BL" --split confirmatory
   --boot-unit week --bootstrap 1000 --permutations 1000 --seed 20261017 --sin-probes-only)

corre() {
  local n=$1; shift
  local s; s=$(date +%s)
  echo "inicio $n $(date -u +%FT%TZ)"
  "$B" "$@" > "$O/$n.txt" 2> "$O/$n.err"
  echo "fin $n rc=$? $(( $(date +%s) - s ))s"
}

filas() {  # regenera rows.csv byte a byte (artefactos/af02/LEEME_rows.txt)
  [[ -f $R ]] && return
  { head -1 artefactos/af02/filas/W01.csv
    for f in $(command ls artefactos/af02/filas/W*.csv | grep -v manifest | sort); do tail -n +2 "$f"; done; } > "$R"
  sha256sum "$R"
}

base() { corre P "${C[@]}" --rows "$R" --emit-weeks "$O/P.weeks.csv"; }
T1() { corre T1 "${C[@]}" --rows "$R" --x-def later-other; }
T2() {
  corre T2_P "${C[@]}" --rows "$R" --x-def donor-pool --donor-seed 1 --emit-weeks "$O/T2_P.weeks.csv"
  corre T2_don "${C[@]}" --rows "$R" --x-def prior-other --donor-subset --emit-weeks "$O/T2_don.weeks.csv"
  corre T2_contraste race-contrast --a "$O/T2_don.weeks.csv" --b "$O/T2_P.weeks.csv" --bootstrap 1000 --seed 20261017
}
T3() {
  corre T3_E "${C[@]}" --rows "$R" --x-def prior-same --emit-weeks "$O/T3_E.weeks.csv"
  corre T3_contraste race-contrast --a "$O/P.weeks.csv" --b "$O/T3_E.weeks.csv" --bootstrap 1000 --seed 20261017
}
T4() {
  corre T4_XgU "${C[@]}" --rows "$R" --x-def prior-other --x-also user-prior
  corre T4_U "${C[@]}" --rows "$R" --x-def user-prior
}
T5() {
  corre T5_grid "${C[@]}" --rows "$R" --missing-grid
  corre T5_c "${C[@]}" --rows "$R" --p-rule contract
  corre T5_size "${C[@]}" --rows "$R" --p-size-strata
}
T6() {
  corre T6_025 "${C[@]}" --rows "$R" --bot-share 0.25
  corre T6_075 "${C[@]}" --rows "$R" --bot-share 0.75
}
T7() {
  corre T7_deg "${C[@]}" --rows "$R" --max-sender-degree 10
  corre T7_loo "${C[@]}" --rows "$R" --loo 5
}
T8() { corre T8 "${C[@]}" --rows "$R" --decompose; }
T9() { corre T9 "${C[@]}" --rows "$R" --het; }
T10() {  # filas re-extraidas de las trazas guardadas con pseudo-pools v4 (sin RPC)
  local R4=artefactos/af04/vistas_v4
  mkdir -p "$R4"
  if [[ ! -f $R4/listo ]]; then
    "$B" fetch --offline --chain base --v4-pools --blocks datos/muestra_bloques_af02.csv --split confirmatory \
      --seal "$SELLO" --raw-dir artefactos/rpc_af02 --out "$R4/filas.csv" > "$O/T10_fetch.txt" 2>&1 && touch "$R4/listo"
    echo "fetch T10 rc=$?"
  fi
  local V=(race --rows "$R4/filas.csv" --raw artefactos/rpc_af02 --blocks "$BL" --split confirmatory
           --boot-unit week --bootstrap 1000 --permutations 1000 --seed 20261017 --sin-probes-only
           --chain base --v4-swap-pools)
  corre T10 "${V[@]}" --exclude-system true --emit-weeks "$O/T10.weeks.csv"
  corre T10_contraste race-contrast --a "$O/T10.weeks.csv" --b "$O/P.weeks.csv" --bootstrap 1000 --seed 20261017
  corre T10_sinexcl "${V[@]}" --emit-weeks "$O/T10_sinexcl.weeks.csv"
  corre T10_neighbor neighbor --chain base --exclude-system true --rows "$R4/filas.csv" \
    --blocks datos/muestra_bloques_af02.csv --split confirmatory --boot-unit week --bootstrap 1000 --seed 20261017
}

PRUEBAS=("$@")
[[ ${#PRUEBAS[@]} -eq 0 ]] && PRUEBAS=(base T1 T2 T3 T4 T5 T6 T7 T8 T9)
filas
for p in "${PRUEBAS[@]}"; do "$p"; done
