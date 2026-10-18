---
estado: INMUTABLE
categoria_documental: PREREGISTRO
id_afirmacion: AF-02
estado_afirmacion: PREREGISTRADA
fecha_registro: 2026-10-18
plantilla: PLANTILLA_PREREGISTRO 1.0.0
revisa: docs/reportes/2026-10-17-veredicto-af01-contencion.md
---

# Preregistro — Carrera dentro del bloque y cota de sincronia en 48 semanas de Base

Retrospectivo en parte ([PRE-01]): las dos pruebas se formularon despues de abrir el confirmatorio de AF-01 (EXPL-16 a EXPL-24) y del piloto de vecinos sobre la semana S4 de AF-01 (EXPL-25). Ninguna de las 48 semanas de este estudio se ha descargado: las cuatro semanas de AF-01 quedan fuera. El diseno y sus alternativas estan en `docs/adrs/004-diseno-af02.md`.

## Pregunta y alcance

- Pregunta primaria: en Base, ¿un bot falla mas cuando otra entidad ya hizo swap antes en el mismo bloque en los pools que el intenta, a igualdad de entidad y de posicion en el bloque? (H1, carrera).
- Pregunta secundaria con criterio propio: ¿cuanto de la coincidencia entre entidades en el mismo bloque puede deberse a sincronia de bloque, acotada por el bloque siguiente a 2 s? (H2, cota).
- Tipo de afirmacion: H1 `ESTIMACION`, alcance `OBSERVADO`; la lectura como competencia por estado es `INFERIDO` (llegada y fee tambien fijan el orden). H2 es una cota superior, no una identificacion (`descomposicion-exceso.md`, Prop. 3, Consec. 4).
- Poblacion: bloques de Base en 48 semanas de 2025 (inicio domingo, UTC, del 2025-01-05 al 2025-12-28), todas salvo 2025-06-22, 2025-08-10, 2025-09-14 y 2025-12-21 (AF-01). Universo de bots y entidades por semana, con las reglas de AF-01.
- Fuera de alcance: otras cadenas (AF-03); atribuir diferencias a Flashblocks o a cambios de fee (se describen por semana, no se prueban); identificar operadores.

## Hipotesis

- **H1 (carrera):** la diferencia de riesgos de Mantel-Haenszel agregada sobre las 48 semanas, $\Delta_{MH}$ (`contencion-nulos-secundarios.md` §4.6), variante sin probes, celdas (semana, entidad, decil de posicion), es ≥ 0.02 y su IC 95% excluye 0.
  - Falsada si el limite superior del IC 95% es < 0.02.
  - Inconclusa en cualquier otro caso.
- **H2 (sincronia de bloque):** $U_{2s} = C_{0} - C_{2s}$ agregada (intentos fallidos de anclas con vecino $b+1$).
  - Se declara "sincronia de bloque no mayor a 2 puntos" si el limite superior del IC 95% de $U_{2s}$ es < 0.02.
  - Si no, se reporta $U_{2s}$ con su IC como cota superior de la sincronia, sin afirmar que exista.
- Alternativas ([PRE-04]):
  - R-pos, posicion residual dentro del decil: sensibilidad con 50 tramos.
  - R-act, actividad del bloque (bloques con mas swaps tienen mas reverts por cualquier causa): sensibilidad con celdas (entidad, bloque), que absorbe todo lo comun al bloque.
  - R-ent, entidad mal agrupada: sensibilidad sin remitentes de grado ≥ 10.
  - R-der, deriva entre bloques para H2: la mide la curva $C_{0}, C_{2s}, C_{30s}, C_{5min}$.
- Condiciones que invalidan: mas del 5% de los bloques sin traza o sin receipts tras reintentos; menos de 1,000 celdas con ambos grupos en total (H1); menos de 2,000 pares ancla-vecino a 2 s con intentos fallidos (H2).

## Evidencia y metodo

- Fuente: Alchemy JSON-RPC (Base mainnet), `debug_traceBlockByNumber` con `callTracer` y `eth_getBlockReceipts`, via `mev-revert-classifier fetch` (v0.6.0). Probe, revert y pools como en AF-01.
- Unidad: el bloque ancla (muestra aleatoria estratificada); dentro, la transaccion. Los vecinos solo aportan $S_{b'}$ en H2.
- Muestreo: `sample --neighbors` por semana: en cada hora UTC, las 3 anclas con menor `sha256("mev-contention-v1:<bloque>")`; las dos primeras con vecino $b+1$ y la tercera con $b+15$ y $b+150$ (si un vecino choca con un ancla u otro vecino, se toma el siguiente candidato). 504 anclas y 672 vecinos por semana; 24,192 anclas y 56,448 bloques en total. Todos son confirmatorios: el exploratorio de este diseno fue AF-01 y el piloto EXPL-25.
  - Lista completa: `datos/muestra_bloques_af02.csv` (sha256 776f9f9c…, columnas `role` y `anchor`).
  - Solo anclas, derivada de la anterior: `datos/muestra_bloques_af02_anclas.csv`, entrada de `race`.
- Entidad: componentes conexas contrato–remitente por semana, como en AF-01.
- X y Y (H1): $X(t) = 1$ si otra entidad tuvo un swap exitoso (log Swap en receipt) antes en el bloque en algun pool de $P(t)$; $Y(t) = 1$ si $t$ es revert.
- Pesos (H2): $w_p = 1/n_p$ calculados solo sobre anclas; universo y entidades solo sobre anclas.
- Analisis primario:
  - H1: `race --rows <filas> --raw artefactos/rpc_af02 --blocks datos/muestra_bloques_af02_anclas.csv --split confirmatory --boot-unit week`, B = 1000, K = 1000, semilla 20261017; linea agregada, variante sin probes.
  - H2: `neighbor --rows <filas> --blocks datos/muestra_bloques_af02.csv --split confirmatory --boot-unit week`, semilla 20261017; linea agregada, $U_{2s}$.
  - IC por bootstrap de semanas completas (48 unidades): la dependencia dentro de la semana (bots que persisten, eventos) queda dentro de la unidad.
- Sensibilidades (se reportan todas, no deciden): H1 con 50 tramos, con celdas (entidad, bloque), con probes, con bootstrap de horas, sin remitentes de grado ≥ 10; distribucion de $\Delta_{MH}$ por semana (cuantas semanas > 0). H2 con $U_{30s}$, $U_{5min}$, sin vecinos vacios (`--drop-blocks-without-rows`), con bootstrap de horas. `contend` primario sobre las anclas por semana, como replica descriptiva de AF-01.
- Multiplicidad: H1 y H2 son afirmaciones distintas con criterio propio; no se combinan ni se elige la favorable. Las sensibilidades no deciden ([VAL-08]).
- Verificacion cruzada: `scripts/analisis/carrera_tasa.py` y `scripts/analisis/vecino.py` sobre las mismas filas deben dar las mismas estimaciones puntuales; si no, el estudio se detiene y se registra.

## Ejecucion

- Presupuesto: tope Alchemy $5 USD; gastado antes de este estudio $0.13 + piloto. Con ~37 CU por bloque medidos en el piloto (EXPL-25; ~61 en AF-01), 56,448 bloques ≈ 2.1 a 3.4 M CU ≈ $1.1 a $1.8 USD (`ESTIMACION`); ~7 h de descarga con 4 workers.
- Disco: ~0.33 MB de crudo por bloque (piloto EXPL-25), ~18 GB en total. Por semana (`scripts/ingesta/descarga_af02.py`): `fetch` de la semana, filas en `artefactos/af02/filas/`, trazas empaquetadas en un `.tar` por semana en `artefactos/af02_trazas/`, `dvc add` y `dvc push` de ese archivo y borrado local de trazas y cache; los receipts se quedan en `artefactos/rpc_af02/` (los lee `race`).
- Criterio de parada: se descargan las 48 semanas completas antes de correr `race` o `neighbor` sobre cualquiera. Durante la descarga solo se miran cobertura y errores de `fetch`. Una sola corrida primaria.
- Semillas: hash deterministico para el muestreo; 20261017 para bootstrap y permutaciones.
- Artefactos: `artefactos/rpc_af02/` (receipts gzip, manifiesto sha256), `artefactos/af02_trazas/` (trazas por semana, en DVC), `artefactos/af02/rows.csv` (union de las filas por semana), salidas de `race`, `neighbor` y verificacion cruzada; registro de ejecucion y reporte de veredicto en `docs/reportes/`.
- Verificacion externa prometida: `REPRODUCCION_NO_INDEPENDIENTE`.

## Riesgos y autoridad

- Etica y privacidad: datos publicos on-chain, contratos de bots; tablas publicadas con ≥ 5 tx por celda ([PRI-02]).
- Licencias: terminos de Alchemy.
- Conflictos de interes: el autor presenta la señal de carrera como exploratoria en una charla el 2026-10-29; el resultado de AF-02 se reporta sea cual sea, incluida la falsacion de H1.

## Sellado

- Responsable: Keneth Huerta.
- Sello: `docs/04_investigacion_y_labs/2026-10-18-sello-af02.sha256` (este archivo, ADR 004, `contencion-nulos-secundarios.md`, `descomposicion-exceso.md`, las dos muestras, `src/`, `Cargo.toml`, `Cargo.lock`, `scripts/analisis/carrera_tasa.py`, `scripts/analisis/vecino.py`, `scripts/analisis/comun_contencion.py`, `scripts/analisis/orden_swap.py`, `scripts/ingesta/descarga_af02.py`) y tag `AF-02-sellado`. `fetch --seal` verifica el sello antes de abrir un bloque.
- Evidencia observada antes de este registro: todo AF-01 (exploratorio y confirmatorio, S1 a S4) y el piloto de vecinos de S4 (EXPL-25). Ningun bloque de las 48 semanas se ha descargado.
