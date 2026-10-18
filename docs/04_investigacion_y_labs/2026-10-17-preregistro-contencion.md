---
estado: INMUTABLE
categoria_documental: PREREGISTRO
id_afirmacion: AF-01
estado_afirmacion: PREREGISTRADA
fecha_registro: 2026-10-17
plantilla: PLANTILLA_PREREGISTRO 1.0.0
revisa: docs/reportes/2026-06-29-clasificacion-dune-7843588-semana30.md
---

# Preregistro — Contencion entre bots en los intentos fallidos de arbitraje en Base

Retrospectivo en parte ([PRE-01]): la pregunta y la idea del modelo nulo se formularon despues de ver la muestra de 32k filas (EXPL-01 a EXPL-07). Ningun bloque de esa muestra entra al conjunto confirmatorio. El universo de bots, el bootstrap y la metrica primaria se fijaron con el set exploratorio de 819 bloques (EXPL-10 a EXPL-12; ADR 003), antes de descargar el confirmatorio.

## Pregunta y alcance

- Pregunta primaria: en Base, ¿que fraccion de los intentos fallidos de bots de arbitraje ocurre en contencion (glosario) con al menos otro bot dentro del mismo bloque, por encima de lo que predice el modelo nulo?
- Tipo de afirmacion: `ESTIMACION` con alcance `OBSERVADO` ([EVI-07], [EVI-13]); la interpretacion como competencia es `INFERIDO`.
- Poblacion: bloques de Base en cuatro semanas-estrato (inicio domingo, UTC): S1 2025-06-22 (antes de Flashblocks, 2025-07-07), S2 2025-08-10 (despues), S3 2025-09-14 (pico de bots on-chain discovery) y S4 2025-12-21 (fee floor de 500K wei desde 2025-12-18). En ellos, las transacciones a contratos del universo de bots (fundamento §4.1).
- Fuera de alcance: identificar operadores detras de los bots; MEV que no sea arbitraje ciclico; otras cadenas.

## Hipotesis

- Hipotesis principal (H1): con la metrica primaria CRw (fundamento §4.4 bis, ADR 003), el exceso E = CRw_obs − media(CRw_nulo) es ≥ 0.05 y CRw_obs supera el percentil 99 del nulo en al menos una semana-estrato.
- Alternativas plausibles ([PRE-04]):
  - R1, busqueda aislada: los intentos fallidos son rafagas propias sin sincronia entre bots; E ≈ 0.
  - R3, un operador con varios contratos: el traslape "entre bots" es redundancia propia de una misma entidad. Se controla agrupando contratos en entidades (Variables) y comparando con la version sin agrupar.
  - R2, pools concentradores: el traslape viene de que todos los bots leen los mismos pools grandes (WETH/USDC) y no de competir por la misma oportunidad. El nulo preserva la preferencia de pools de cada entidad; la metrica primaria pondera cada pool por $1/n_p$ y las sensibilidades excluyen los 1, 5, 10 y 20 pools mas leidos.
- Resultado que la falsaria: CRw_obs dentro del intervalo central del 95% del nulo en las cuatro semanas, o E < 0.05 en todas.
- Resultado inconcluso: menos del 50% de los intentos fallidos de bots tiene un conjunto de pools no vacio, o el IC 95% de E tiene ancho > 0.10.
- Condicion que invalidaria el estudio: mas del 5% de los bloques confirmatorios sin traza o sin receipts tras los reintentos de `fetch`, o una definicion de probe distinta a la de Wu y Oz (AFT 2026, §4.1).

## Evidencia y metodo

- Fuente o adquisicion: Alchemy JSON-RPC (Base mainnet): `debug_traceBlockByNumber` con `callTracer` y `eth_getBlockReceipts`, via `mev-revert-classifier fetch` (ADR 002). Definicion de probe y revert de Wu y Oz (AFT 2026, §4.1), validada en 234 de 234 intentos contra Dune 7809756 (EXPL-09). Pools segun `docs/07_fundamentos_teoricos/contencion-entre-bots.md` §4.2.
- Unidad de analisis: el bloque (conglomerado de muestreo). Dentro del bloque, la transaccion.
- Muestreo: en cada semana-estrato, estratificado por hora (168 estratos): los 6 bloques con menor `sha256("mev-contention-v1:<bloque>")` de cada hora; 1,008 bloques por semana, 4,032 en total. Lista en `datos/muestra_bloques_v1.csv` (sha256 360a20bc…).
- Particion: un bloque es exploratorio si el primer byte de `sha256("explore-v1:<bloque>")` es < 52 (819 bloques, 20.3%); es exploratorio y sirve para fijar detalles de implementacion; el 80% restante es confirmatorio y queda sellado ([VAL-07], [VAL-11]).
- Entidad: los contratos bot se agrupan en componentes conexas del grafo bipartito contrato–remitente (`from`): dos contratos que comparten algun remitente son la misma entidad. La contencion se cuenta entre entidades distintas.
- Variables por transaccion: `to` (contrato bot), `from`, `tx_index`, exito o falla, tipo (probe, revert, arbitraje exitoso), gas, fee y conjunto de pools tocados (direcciones del universo de pools que aparecen como destino en sus trazas, incluidas las `staticcall`).
- Transformaciones: metrica primaria CRw, media sobre intentos fallidos de la fraccion de su peso $1/n_p$ en pools que otra entidad toca en el mismo bloque (fundamento §4.4 bis; ADR 003, decidido con el set exploratorio, EXPL-12). Metrica secundaria: CR binaria (§4.4). Sensibilidades: exclusion de los 1, 5, 10 y 20 pools mas leidos; contratos sin agrupar; solo bots con arbitraje ciclico; proporcion de fallos 0.25 y 0.75.
- Controles: modelo nulo. En cada estrato horario se reasigna al azar, para cada entidad, su paquete de transacciones de un bloque a otro bloque sorteado de la misma hora; asi se preservan las rafagas y las preferencias de pools de cada entidad y se rompe la sincronia. 1,000 permutaciones por semana.
- Exclusiones: los bloques 39872527-39873117 (muestra ya vista); transacciones sin trazas, que se reportan aparte.
- Analisis primario: CRw_obs, CRw_nulo y E por semana-estrato con `contend --metric weighted --split confirmatory`, 1,000 permutaciones, IC 95% por bootstrap de horas completas (2,000 replicas × 50 permutaciones), semilla 20261017.
- Analisis exploratorios separados: (a) conciliacion con Anchuri, Felten y Mamageishvili (arXiv 2610.03922) en S2: fraccion de pools-bloque con una sola entidad; (b) rafaga propia: fraccion de las tx de una entidad en un bloque con calldata identica; (c) CR por ventana de Flashblock aproximada por `tx_index`; (d) estudio de eventos de los fallos de otras entidades alrededor de cada arbitraje exitoso con pools en comun; (e) un segundo nulo que permuta solo entre bloques donde la entidad estuvo activa; (f) la heuristica de la v0.1.0 sobre la muestra nueva, para cuantificar ERR-01 y ERR-02. Ninguno decide H1.
- Multiplicidad: cuatro pruebas primarias (una por semana, metrica CRw), corregidas con Holm. La binaria y las sensibilidades se reportan todas y no deciden H1 ([VAL-08]). Con 1,000 permutaciones el p minimo es 1/1001.

## Ejecucion

- Presupuesto y tamano justificado: Alchemy Pay As You Go con tope de $5 USD; estimado 170 CU por traza de bloque. En el exploratorio (~200 bloques por semana) el IC 95% de E con CRw tuvo ancho de 0.016 a 0.034; con ~800 bloques confirmatorios por semana se espera un ancho menor, muy por debajo de 0.10.
- Criterio de parada: una sola corrida confirmatoria por semana-estrato; si falla por cuota se repite completa, sin mirar resultados parciales.
- Semillas, orden o aleatorizacion: hash deterministico para el muestreo; semilla fija registrada para permutaciones y bootstrap.
- Artefactos esperados: crudo RPC en gzip en `artefactos/rpc/muestra_v1/` con manifiesto sha256, filas extraidas en `artefactos/confirmatorio/rows.csv`, salida de `contend`, registro de ejecucion en `docs/reportes/` ([EJE-09]) y reporte de veredicto ([EVI-23]).
- Nivel prometido de verificacion externa: `REPRODUCCION_NO_INDEPENDIENTE` (sellado del autor, sin custodio).

## Riesgos y autoridad

- Etica, consentimiento y permisos: datos publicos on-chain; no hay sujetos humanos.
- Privacidad y seguridad: las direcciones son contratos de bots; no se intenta vincularlas con personas. Las tablas publicadas no bajan de 5 transacciones por celda ([PRI-02]).
- Licencias o restricciones: terminos de servicio de Alchemy; datos on-chain publicos.
- Conflictos de interes: el autor publico la afirmacion previa (ERR-01) y presenta una charla el 2026-10-29; el resultado se reporta sea cual sea.

## Sellado

- Responsable: Keneth Huerta.
- Version o hash: `docs/04_investigacion_y_labs/2026-10-17-sello-contencion.sha256` (sha256 de este archivo, del fundamento, de los ADR 002 y 003, de la muestra, del codigo y del lockfile) y tag `AF-01-sellado`. `fetch --split confirmatory` verifica el sello antes de abrir un bloque.
- Evidencia observada antes de este registro: `artefactos/dune_7843588_w30_32k.csv` (sha256 b25bc7f3d142532d…), bloques 39872527-39873117 (EXPL-01 a EXPL-07); el piloto de 5 bloques de ese rango (EXPL-09); los 819 bloques exploratorios de `datos/muestra_bloques_v1.csv` (EXPL-10 a EXPL-12). Ningun bloque confirmatorio se ha descargado.
