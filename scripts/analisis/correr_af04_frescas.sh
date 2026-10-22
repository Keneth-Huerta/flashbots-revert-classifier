#!/usr/bin/env bash
# AF-04: corridas preregistradas sobre las 48 semanas frescas (T0 a T7, T9 y S-v4).
# Los comandos son los del preregistro (docs/04_investigacion_y_labs/2026-10-18-preregistro-af04.md);
# este script solo los encadena y guarda cada salida en artefactos/af04/salidas/<prueba>.txt.
# T0 por anio filtra la muestra sellada por estrato (F24, F26), sin tocar el archivo sellado.
# Uso: bash scripts/analisis/correr_af04_frescas.sh [prueba ...]   (sin argumentos: todas)
set -uo pipefail
cd "$(dirname "$0")/../.."
B=target/release/mev-revert-classifier
D=artefactos/af04
O=$D/salidas
BL=datos/muestra_bloques_af04_anclas.csv
mkdir -p "$O" "$D/sub"
C=(race --rows "$D/filas.csv" --raw artefactos/rpc_af04 --split confirmatory
   --boot-unit week --bootstrap 1000 --permutations 1000 --seed 20261017 --sin-probes-only)

corre() {  # corre <nombre> <args...>
  local n=$1; shift
  local s; s=$(date +%s)
  echo "inicio $n $(date -u +%FT%TZ)"
  "$B" "$@" > "$O/$n.txt" 2> "$O/$n.err"
  echo "fin $n rc=$? $(( $(date +%s) - s ))s"
}

T0() {
  corre T0 "${C[@]}" --blocks "$BL" --emit-weeks "$O/T0.weeks.csv"
  for y in F24 F26; do
    { head -1 "$BL"; grep "^$y-" "$BL"; } > "$D/sub/anclas_$y.csv"
    corre "T0_$y" "${C[@]}" --blocks "$D/sub/anclas_$y.csv"
  done
}
T1() {
  corre T1 "${C[@]}" --blocks "$BL" --x-def later-other
  corre T1_x "${C[@]}" --blocks "$BL" --x-def later-other --x-also prior-other
  corre T1_b50 "${C[@]}" --blocks "$BL" --x-def later-other --bins 50
  corre T1_blk "${C[@]}" --blocks "$BL" --x-def later-other --cell-block
}
T2() {
  corre T2_P "${C[@]}" --blocks "$BL" --x-def donor-pool --donor-seed 1 --emit-weeks "$O/T2_P.weeks.csv"
  corre T2_don "${C[@]}" --blocks "$BL" --x-def prior-other --donor-subset --emit-weeks "$O/T2_don.weeks.csv"
  corre T2_contraste race-contrast --a "$O/T2_don.weeks.csv" --b "$O/T2_P.weeks.csv" --bootstrap 1000 --seed 20261017
  for s in 2 3 4 5; do
    corre "T2_P_s$s" "${C[@]}" --blocks "$BL" --x-def donor-pool --donor-seed "$s"
  done
}
T3() {
  corre T3_E "${C[@]}" --blocks "$BL" --x-def prior-same --emit-weeks "$O/T3_E.weeks.csv"
  corre T3_XgE "${C[@]}" --blocks "$BL" --x-def prior-other --x-also prior-same
  corre T3_EgX "${C[@]}" --blocks "$BL" --x-def prior-same --x-also prior-other
  corre T3_contraste race-contrast --a "$O/T0.weeks.csv" --b "$O/T3_E.weeks.csv" --bootstrap 1000 --seed 20261017
}
T4() {
  corre T4_XgU "${C[@]}" --blocks "$BL" --x-def prior-other --x-also user-prior
  corre T4_U "${C[@]}" --blocks "$BL" --x-def user-prior
  corre T4_all "${C[@]}" --blocks "$BL" --x-def any-prior
}
T5() {
  corre T5_grid "${C[@]}" --blocks "$BL" --missing-grid
  corre T5_c "${C[@]}" --blocks "$BL" --p-rule contract
  corre T5_csub "${C[@]}" --blocks "$BL" --p-rule contract-subset
  corre T5_tsub "${C[@]}" --blocks "$BL" --p-rule trace-on-contract-subset
  corre T5_size "${C[@]}" --blocks "$BL" --p-size-strata
}
T6() {
  corre T6_025 "${C[@]}" --blocks "$BL" --bot-share 0.25
  corre T6_075 "${C[@]}" --blocks "$BL" --bot-share 0.75
}
T7() {
  corre T7_deg "${C[@]}" --blocks "$BL" --max-sender-degree 10
  corre T7_loo "${C[@]}" --blocks "$BL" --loo 5
}
T9() {
  corre T9 "${C[@]}" --blocks "$BL" --het --het-groups F24,F26
}
Sv4() {  # re-extraccion offline de F26 con pseudo-pools v4 (sin RPC, sin costo)
  { head -1 "$BL"; grep '^F26-' "$BL"; } > "$D/sub/anclas_F26.csv"
  local R4=$D/v4
  mkdir -p "$R4"
  if [[ ! -f $R4/listo ]]; then
    "$B" fetch --blocks "$D/sub/anclas_F26.csv" --split confirmatory --seal docs/04_investigacion_y_labs/2026-10-18-sello-af04.sha256 \
      --raw-dir artefactos/rpc_af04 --out "$R4/filas.csv" --offline --chain base --v4-pools \
      > "$O/Sv4_fetch.txt" 2>&1 && touch "$R4/listo"
    echo "fetch Sv4 rc=$?"
  fi
  local V=(race --rows "$R4/filas.csv" --raw artefactos/rpc_af04 --blocks "$D/sub/anclas_F26.csv" --split confirmatory
           --boot-unit week --bootstrap 1000 --permutations 1000 --seed 20261017 --sin-probes-only --v4-swap-pools)
  corre Sv4_T0 "${V[@]}"
  corre Sv4_T1 "${V[@]}" --x-def later-other
  corre Sv4_T2_P "${V[@]}" --x-def donor-pool --donor-seed 1 --emit-weeks "$O/Sv4_T2_P.weeks.csv"
  corre Sv4_T2_don "${V[@]}" --x-def prior-other --donor-subset --emit-weeks "$O/Sv4_T2_don.weeks.csv"
  corre Sv4_T2_contraste race-contrast --a "$O/Sv4_T2_don.weeks.csv" --b "$O/Sv4_T2_P.weeks.csv" --bootstrap 1000 --seed 20261017
}

PRUEBAS=("$@")
[[ ${#PRUEBAS[@]} -eq 0 ]] && PRUEBAS=(T0 T1 T2 T3 T4 T5 T6 T7 T9 Sv4)
sha256sum "$D/filas.csv" "$BL" > "$O/entradas.sha256"
for p in "${PRUEBAS[@]}"; do "$p"; done
