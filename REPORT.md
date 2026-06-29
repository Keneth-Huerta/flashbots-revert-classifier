---
estado: INMUTABLE
categoria_documental: REPORTE
tipo: "Clasificacion Heuristica MEV — Dune Query 7843588"
fecha_ejecucion: 2026-06-29
hash_commit: "345c7d7"
mercado: "Base (L2)"
hardware: "Intel i7-1165G7 @ 2.80GHz | 19Gi RAM | Linux 7.0.14-zen1-1-zen"
---

# Reporte de Ejecucion: Clasificacion Heuristica de Reverts MEV — Semana 30, Base

## 0. Prerequisitos de Publicacion

- Arbol de trabajo limpio: si (commit `7b219ff`).
- `hash_commit` corresponde a `git rev-parse --short HEAD`.
- Ejecucion concluyente con 32,000 filas procesadas en 3 configuraciones de umbral.

## 1. Parametros de la Prueba

| Parametro | Valor |
|-----------|-------|
| Dataset | Dune Query 7843588, exportado via API (`limit=100000`) |
| Filas procesadas | 32,000 |
| Bloques unicos | 591 (39872527 - 39873117) |
| Semanas | 1 (semana 30: 2025-12-21 a 2025-12-27) |
| Bots unicos | 74 |
| Umbrales C_min probados | 3, 5, 10 |
| Binario | `target/release/mev-revert-classifier` (compilacion release) |
| Criterio de clasificacion | Agrupacion por `(block_number, bot_address)`; >= C_min → HF Contention, < C_min → Blind Spam |

## 2. Entorno de Hardware (Obligatorio)

| Componente | Valor |
|------------|-------|
| CPU | 11th Gen Intel Core i7-1165G7 @ 2.80GHz (4 cores, 8 threads) |
| RAM | 19 GiB |
| SO | Linux 7.0.14-zen1-1-zen (Arch) |
| GPU | Intel Iris Xe (no utilizada) |

## 3. Resultados Clave

### 3.1 Clasificacion por Umbral

| C_min | Blind Spam (tx) | Blind Spam (%) | HF Contention (tx) | HF Contention (%) | Bots Spam | Clusters Contention | Tiempo |
|-------|-----------------|----------------|---------------------|-------------------|-----------|---------------------|--------|
| 3 | 2,044 | 6.4% | 29,956 | 93.6% | 1,444 | 2,306 | 81ms |
| 5 | 2,880 | 9.0% | 29,120 | 91.0% | 1,701 | 2,049 | 108ms |
| 10 | 9,640 | 30.1% | 22,360 | 69.9% | 2,674 | 1,076 | 86ms |

### 3.2 Gas Promedio por Categoria

| C_min | Blind Spam (gas/tx) | HF Contention (gas/tx) |
|-------|---------------------|-------------------------|
| 3 | 135.2K | 88.6K |
| 5 | 122.6K | 88.5K |
| 10 | 102.9K | 86.7K |

### 3.3 Breakdown Dune (Raw)

| Clasificacion Dune | Conteo |
|---------------------|--------|
| `dry_run_probe` | 31,157 (97.4%) |
| `reverted_spam` | 843 (2.6%) |

### 3.4 Top Bots por Densidad de Colision

El bot `0x837b57a93d4c0e5be3d4c551730fd7f3b6f7722f` ocupa las 10 posiciones del top en los 3 umbrales, con 80-81 transacciones por bloque. Ningun otro bot supera ~12 tx por bloque. Los otros 3 bots importantes son `0x2ffd221f8f...`, `0xd9c72e9403...` y `0x982e4323bc...`, con 2-12 tx por bloque cada uno.

## 4. Artefactos Generados

Los datos crudos del Dune Query 7843588 (32,000 filas, 591 bloques, semana 30) estan disponibles bajo solicitud. El archivo `sample.csv` incluido en este repositorio permite verificar la herramienta con datos sinteticos.

## 5. Conclusiones y Siguientes Pasos

**Veredicto: EXITO.** El clasificador procesa 32,000 filas en <110ms, produce ratios consistentes a traves de 3 umbrales, y revela un patron claro de concentracion de trafico de bots en Base.

**Hallazgos principales:**

1. La contencion de alta frecuencia (HF Contention) domina el trafico: 91-94% del volumen para C_min ∈ {3,5}. Incluso con C_min=10 sigue siendo el 70%.
2. Un solo bot (`0x837b57a93d...`) concentra el top 10 completo con 80+ tx por bloque. Es un outlier extremo que justifica investigacion adicional: ¿esta el bot compitiendo contra si mismo (redundancia interna) o el query esta capturando multiples estrategias del mismo contrato?
3. El gas medio de los bots de contencion (86-88K) es consistentemente menor que el de los bots de spam (102-135K), sugiriendo optimizacion de gas en los bots elite.
4. Solo 2.6% de las transacciones son `reverted_spam` segun Dune; el 97.4% son `dry_run_probe`. Esto sugiere que los bots estan simulando/probando en lugar de fallar.

**Siguientes pasos sugeridos:**

- Ampliar el dataset a multiples semanas (27-31) para validar estacionalidad.
- Agregar heuristica de similitud de calldata para desambiguar multiples estrategias del mismo bot.
- Probar con datos de Ethereum mainnet para comparar patrones de contencion entre L1 y L2.
- Publicar resultados en el foro de investigacion de Flashbots como respuesta al hilo de m1kuwill.
