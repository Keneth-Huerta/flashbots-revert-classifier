#!/usr/bin/env bash
# AF-04: verificacion cruzada Python frente a Rust sobre las semanas frescas (preregistro, seccion
# "Verification before opening the data"). Cada variante corre robustez_af04.py con --comparar sobre
# la salida de correr_af04_frescas.sh; la rejilla de T5(a) se compara aparte (lineas "grid").
# Puntos y conteos solamente; los intervalos vienen de otro generador y no se comparan.
# Reanudable: una variante cuya salida ya termina en "PARIDAD OK" no se repite.
# Uso: bash scripts/analisis/paridad_af04.sh [N_PARALELO]   (por defecto 3)
set -uo pipefail
cd "$(dirname "$0")/../.."
O=artefactos/af04/salidas
PY=(python3 -I scripts/analisis/robustez_af04.py --rows artefactos/af04/filas.csv
    --blocks datos/muestra_bloques_af04_anclas.csv --raw artefactos/rpc_af04 --split confirmatory)
PV4=(python3 -I scripts/analisis/robustez_af04.py --rows artefactos/af04/v4/filas.csv
     --blocks artefactos/af04/sub/anclas_F26.csv --raw artefactos/rpc_af04 --split confirmatory
     --v4-pool-manager 0x498581ff718922c3f8e6a244956af099b2652b2b)
N=${1:-3}

par() {  # par <salida_rust> <comando...>
  local n=$1; shift
  if [[ -f $O/py_$n.txt ]] && [[ $(tail -1 "$O/py_$n.txt") == "PARIDAD OK" ]]; then echo "$n: PARIDAD OK (previa)"; return; fi
  "$@" --comparar "$O/$n.txt" > "$O/py_$n.txt" 2>&1
  echo "$n: $(tail -1 "$O/py_$n.txt")"
}

trabajos=(
  "T1 ${PY[*]} --x-def later_other"
  "T1_x ${PY[*]} --x-def later_other --x-also prior_other"
  "T2_P ${PY[*]} --x-def donor_pool --donor-seed 1"
  "T2_don ${PY[*]} --x-def prior_other --donor-subset"
  "T2_P_s2 ${PY[*]} --x-def donor_pool --donor-seed 2"
  "T3_E ${PY[*]} --x-def prior_same"
  "T3_XgE ${PY[*]} --x-def prior_other --x-also prior_same"
  "T3_EgX ${PY[*]} --x-def prior_same --x-also prior_other"
  "T4_XgU ${PY[*]} --x-def prior_other --x-also user_prior"
  "T4_U ${PY[*]} --x-def user_prior"
  "T4_all ${PY[*]} --x-def any_prior"
  "T5_c ${PY[*]} --p-rule contract"
  "T5_csub ${PY[*]} --p-rule contract_subset"
  "T5_tsub ${PY[*]} --p-rule trace_on_contract_subset"
  "T5_size ${PY[*]} --p-size-strata"
  "T6_025 ${PY[*]} --bot-share 0.25"
  "T6_075 ${PY[*]} --bot-share 0.75"
  "T7_deg ${PY[*]} --max-sender-degree 10"
  "Sv4_T0 ${PV4[*]}"
  "Sv4_T1 ${PV4[*]} --x-def later_other"
  "Sv4_T2_P ${PV4[*]} --x-def donor_pool --donor-seed 1"
  "Sv4_T2_don ${PV4[*]} --x-def prior_other --donor-subset"
)
for t in "${trabajos[@]}"; do
  while (( $(jobs -rp | wc -l) >= N )); do wait -n; done
  # shellcheck disable=SC2086
  par $t &
done
wait

# T5(a): rejilla de faltantes, 121 puntos, mismos digitos impresos
"${PY[@]}" --missing-grid > "$O/py_T5_grid.txt" 2>&1
diff <(grep '^af04 grid q_R' "$O/T5_grid.txt" | sed 's/^af04 grid //') \
     <(grep '^py grid q_R' "$O/py_T5_grid.txt" | sed 's/^py grid //') > /dev/null \
  && echo "T5_grid: PARIDAD OK" || echo "T5_grid: PARIDAD FALLA"
