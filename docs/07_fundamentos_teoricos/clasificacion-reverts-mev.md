# Fundamento Teorico: Clasificacion de Reverts MEV por Heuristicas de Colocacion

## 1. Objetivo

Definir el marco conceptual y las reglas heurísticas que permiten clasificar
transacciones revertidas en la mempool de Ethereum en dos categorías operacionales:
*Spam Ciego* y *Contención de Alta Frecuencia (Colocación Elite)*. Este fundamento
es agnóstico al lenguaje de implementación y sustenta las decisiones de diseño del
clasificador `mev-revert-classifier`.

El objetivo último es proporcionar a investigadores de Flashbots una herramienta
para cuantificar la proporción de tráfico inútil (spam) frente a competencia real
(contención) en bundles y transacciones fallidas observadas on-chain.

## 2. Definiciones y Notacion

- **Revert:** Transaccion incluida en un bloque cuya ejecucion revirtio (status=0),
  consumiendo gas pero sin producir cambios de estado persistentes.

- **Spam Ciego (Blind Spam):** Transacciones revertidas enviadas sin conocimiento
  del estado de la mempool o sin targeting preciso. Se caracterizan por baja densidad
  temporal (few attempts per block), saltos de bloque sin patron de persistencia,
  y contratos objetivo dispersos.

- **Contencion de Alta Frecuencia (HF Contention / Elite Colocation):** Multiples
  transacciones revertidas dirigidas al mismo contrato dentro del mismo bloque,
  con diferenciales de gas minimos entre intentos. Indican competencia entre
  bots sofisticados que monitorean la mempool y disputan la misma oportunidad MEV.

- **Ventana de Bloque (Block Window):** Unidad atomica de agrupacion. Todas las
  transacciones dentro del mismo `block_number` compiten por inclusion en ese
  bloque especifico.

- **Target Contract:** Direccion del contrato interactuado (campo `to` o receptor
  del calldata). Es el proxy semantico de la "oportunidad MEV" que se disputa.

- **Umbral de Colision (Collision Threshold) `C_min`:** Numero minimo de reverts
  al mismo contrato en el mismo bloque para considerar que hubo contencion elite.
  Valor propuesto: `C_min = 3`.

- **Densidad Intra-Bloque `D_b`:** Numero de reverts hacia el mismo `target_contract`
  dentro del mismo `block_number`.

## 3. Marco Conceptual

El clasificador opera bajo el principio de que la **contienda por MEV es un
fenomeno localizado**: dos o mas bots que detectan la misma oportunidad (arbitraje,
liquidacion, sandwich) compiten enviando transacciones al mismo contrato en la
misma ventana temporal (el mismo bloque o bloques adyacentes).

El spam ciego, en cambio, es **ruido no correlacionado**: bots que disparan
transacciones a contratos variados sin coordinacion ni competencia real con otros
bots. No hay evidencia de que otro bot compitio por la misma oportunidad.

La clasificacion se realiza mediante agrupacion por `(block_number, target_contract)`
y aplicacion de umbrales de densidad.

## 4. Formulacion Matematica

Sea `T` el conjunto de transacciones revertidas. Cada transaccion `t ∈ T` tiene:

- `t.block`: Numero de bloque (entero)
- `t.target`: Direccion del contrato objetivo (hash/bytes)
- `t.gas_used`: Gas consumido (entero)
- `t.timestamp`: Timestamp del bloque (UNIX epoch en segundos)

### 4.1 Particion por Oportunidad

Se define la particion `P` sobre `T` agrupando por `(block, target)`:

```
P(b, c) = { t ∈ T | t.block = b ∧ t.target = c }
```

### 4.2 Regla de Clasificacion

Para cada grupo `G = P(b, c)`:

- Si `|G| ≥ C_min` → **HF Contention** (todas las transacciones del grupo)
- Si `|G| < C_min` → **Blind Spam** (todas las transacciones del grupo)

Donde `C_min = 3` (configurable). La eleccion de `C_min` se fundamenta en que
2 colisiones pueden ser coincidencia; 3 o mas indican competencia sistematica.

### 4.3 Metricas de Salida

- **Spam Ratio:** `|T_spam| / |T|`
- **Contention Ratio:** `|T_contention| / |T|`
- **Unique Targets (Spam):** Numero de contratos unicos con `|G| < C_min`
- **Unique Targets (Contention):** Numero de contratos unicos con `|G| ≥ C_min`
- **Avg Gas per Cluster (Spam vs Contention):** Gas medio consumido por grupo
- **Max Collision Density:** `max(|G|)` entre todos los grupos

## 5. Limites, Riesgos y Casos de Falla

- **Riesgo 1: Bloques de Alta Congestion.** En bloques con cientos de reverts,
  la probabilidad de colision aleatoria aumenta. `C_min = 3` podria generar falsos
  positivos de contencion. **Mitigacion:** Permitir `C_min` configurable via CLI.

- **Riesgo 2: MEV Multibloque.** Oportunidades que abarcan mas de un bloque
  (ej. arbitrajes cross-chain) generaran colisiones en bloques adyacentes que
  el modelo actual no captura. **Mitigacion:** Futura heuristica de ventana temporal
  expandida (+1/-1 bloque).

- **Riesgo 3: Contratos Router (Uniswap, aggregators).** Un contrato muy usado
  (ej. Uniswap Universal Router) puede recibir reverts de bots no relacionados
  que compiten por oportunidades distintas dentro del mismo contrato. **Mitigacion:**
  Futura heuristica de calldata-similarity para desambiguar.

- **Riesgo 4: Sandwiches Fallidos.** Un sandwich attack fallido puede generar
  exactamente 2 reverts (frontrun + backrun) que caerian en spam con `C_min = 3`.
  **Mitigacion:** Documentar como limitacion conocida; version futura con deteccion
  de patrones sandwich.

## 6. Implicaciones para Arquitectura y ADR

Este fundamento teorico condiciona:

- La estructura de datos central: `HashMap<(u64, String), Vec<Transaction>>`
  para la particion `P(b, c)` en tiempo `O(n)`.
- La ausencia de necesidad de paralelismo `rayon` para datasets < 1M filas
  (la agrupacion es inherentemente secuencial por dependencia de claves).
- La salida de CLI debe imprimir las metricas de la seccion 4.3 exactamente.

- **ADR relacionado:** `adrs/001-arquitectura-pipeline-rust.md`
- **Modulos impactados:** Motor de clasificacion, modulo CLI, modulo de I/O CSV.

## 7. Criterios de Validacion Empirica

| Criterio | Metrica | Umbral | Resultado esperado |
|----------|---------|--------|--------------------|
| Procesamiento | Tiempo de ejecucion | < 1s para 100k filas | Aceptable |
| Clasificacion | Spam + Contention = 100% | Exacto (sin filas huerfanas) | Obligatorio |
| Determinismo | Mismo CSV → mismos ratios | 100% reproducible | Obligatorio |
| Robustez CSV | Manejo de columnas faltantes/campos nulos | No panic | Obligatorio |

## 8. Referencias

- Flashbots Research Forum: "Transaction spam heuristics" discussion, query 7809756 en Dune Analytics.
- Flashbots. (2024). *MEV-Boost and the Block Builder Ecosystem.*
- Dune Analytics. SQL query 7809756 por m1kuwill, dataset de transacciones revertidas en bundles.
- Ethereum Yellow Paper, definicion de status=0 (revert) en ejecucion de transacciones.

## Nota editorial (2026-10-17)

La implementacion agrupo por `bot_address`, no por contrato objetivo, y por eso no mide contencion. La medicion de contencion se define en `docs/07_fundamentos_teoricos/contencion-entre-bots.md`, que reemplaza a este documento.
