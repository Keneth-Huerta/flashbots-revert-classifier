#!/bin/zsh
# Descarga de AF-04 (preregistro 2026-10-18-preregistro-af04.md, seccion Execution): fetch de las 48
# semanas frescas de Base (24 de 2024, 24 de 2026), solo anclas, con el sello, una semana a la vez.
# Lee .env al entorno sin imprimirlo.
#
# Puertas, en este orden (todas ANTES de cualquier llamada de red):
#   1. Debe existir el comprobante de registro externo (RECIBO), con un DOI y con el sha256 del
#      preregistro. El comprobante lo escribe el responsable al depositar; no forma parte del sello.
#   2. La muestra de anclas debe tener el sha256 declarado en el preregistro.
#   3. `fetch --seal` verifica el sha256 de cada archivo del sello (sin --post-seal; este script no
#      usa --post-seal).
# Tope duro de US$1.00 para el estudio: entre semanas se lee `alchemy usage summary --json`; si el gasto
# desde el inicio mas el de una semana (US$0.02, cota) pasa de US$1.00, se detiene (codigo 3) y queda
# pendiente una decision del responsable. Las semanas ya descargadas se omiten al reanudar.
#
# Uso: scripts/ingesta/descarga_af04.sh            descarga completa
#      MAXB=500 scripts/ingesta/descarga_af04.sh   prueba corta de N bloques por semana (tambien exige las puertas)
cd /home/valge/Documents/repos/mev-revert-classifier || exit 1

PRE=docs/04_investigacion_y_labs/2026-10-18-preregistro-af04.md
RECIBO=docs/04_investigacion_y_labs/2026-10-18-registro-externo-af04.txt
S=docs/04_investigacion_y_labs/2026-10-18-sello-af04.sha256
BLOQUES=datos/muestra_bloques_af04_anclas.csv
SHA_ANCLAS=e5a935c531c4b818039c956f4a6606b79de966cc334a5ed1255ccd09a3729c3a
TOPE=1.00
POR_SEMANA=0.02

# Puerta 1: comprobante de registro externo.
[[ -f $RECIBO ]] || { echo "falta $RECIBO (comprobante del registro externo: DOI y sha256 del preregistro): no se descarga"; exit 2; }
grep -Eq '10\.[0-9]{4,9}/[^[:space:]]+' $RECIBO || { echo "$RECIBO no contiene un DOI: no se descarga"; exit 2; }
SHA_PRE=$(sha256sum $PRE | cut -d' ' -f1)
grep -q $SHA_PRE $RECIBO || { echo "$RECIBO no contiene el sha256 actual del preregistro ($SHA_PRE): no se descarga"; exit 2; }
# Puerta 2: la muestra es la declarada.
[[ $(sha256sum $BLOQUES | cut -d' ' -f1) == $SHA_ANCLAS ]] || { echo "la muestra de anclas no tiene el sha256 declarado: no se descarga"; exit 2; }
[[ -f $S ]] || { echo "falta el sello $S: no se descarga"; exit 2; }

set -a; source .env; set +a
RAW=artefactos/rpc_af04
OUT=artefactos/af04
LOG=$OUT/descarga.log
mkdir -p $OUT/filas $OUT/semanas $RAW
uso() { alchemy usage summary --json | python3 -c 'import json,sys; d=json.load(sys.stdin)["data"]; print(d["usageLimit"]["used"])'; }
USO0=$(uso)
echo "uso inicial USD: $USO0 $(date -u +%FT%TZ)" | tee -a $LOG
for SEM in $(tail -n +2 $BLOQUES | cut -d, -f1 | sort -u); do
  [[ -f $OUT/filas/$SEM.listo ]] && continue
  USO=$(uso)
  python3 -c "import sys; sys.exit(0 if float('$USO') - float('$USO0') + $POR_SEMANA <= $TOPE else 1)" || {
    echo "tope US\$$TOPE: gasto $USO - $USO0 + $POR_SEMANA; se detiene antes de $SEM; decide el responsable" | tee -a $LOG; exit 3; }
  head -1 $BLOQUES > $OUT/semanas/$SEM.csv
  awk -F, -v w=$SEM '$1==w' $BLOQUES >> $OUT/semanas/$SEM.csv
  echo "inicio $SEM $(date -u +%FT%TZ)" | tee -a $LOG
  target/release/mev-revert-classifier fetch --chain base --blocks $OUT/semanas/$SEM.csv --split confirmatory --seal $S --raw-dir $RAW --out $OUT/filas/$SEM.csv ${MAXB:+--max-blocks $MAXB} 2>&1 | tee -a $LOG
  RC=$pipestatus[1]
  echo "fin $SEM rc=$RC $(date -u +%FT%TZ)" | tee -a $LOG
  [[ $RC == 0 ]] || { echo "fetch fallo en $SEM; se detiene"; exit 4; }
  touch $OUT/filas/$SEM.listo
done
# Union de las filas por semana (un solo encabezado), en orden de semana.
FILAS=($(ls $OUT/filas/F2*.csv | grep -v manifest | sort))
head -1 $FILAS[1] > $OUT/filas.csv
for f in $FILAS; do tail -n +2 $f >> $OUT/filas.csv; done
echo "uso final USD: $(uso) (inicial $USO0) $(date -u +%FT%TZ)" | tee -a $LOG
