---
estado: INMUTABLE
categoria_documental: PREREGISTRO
id_afirmacion: AF-03
estado_afirmacion: PREREGISTRADA
fecha_registro: 2026-10-18
---

# Preregistro: carrera dentro del bloque y cota de sincronia en Unichain y OP Mainnet

Autor: Keneth Issac Huerta Galindo. Fecha de redaccion: 2026-10-18.

Este documento fija, antes de descargar un solo dato de las semanas sorteadas, las hipotesis, los umbrales, la regla de muestreo, el analisis primario, las sensibilidades y los criterios de falsacion de un estudio de replicacion en dos cadenas. Es parcialmente retrospectivo: se escribio despues de conocer el resultado del estudio previo en Base y despues de pilotos de 30 bloques por cadena (ver "Evidencia observada antes de este registro").

## Pregunta y alcance

- Pregunta primaria: en cada cadena c (Unichain, OP Mainnet), ¿una transaccion de un bot revierte mas cuando otra entidad ya hizo swap antes en el mismo bloque en los pools que intenta, a igualdad de entidad y de posicion en el bloque? (H1-c, carrera).
- Pregunta secundaria con criterio propio: en cada cadena, ¿cuanto de la coincidencia entre entidades en un mismo momento puede deberse a sincronia de bloque, acotada por el bloque situado 2 segundos despues? (H2-c, cota).
- Contexto: el estudio previo del autor en Base (48 semanas de 2025, 56,448 bloques) encontro una diferencia de riesgos de Mantel-Haenszel de +0.0775 (IC 95% [+0.0656, +0.0928]) en la prueba de carrera, positiva en 48 de 48 semanas, y una cota de sincronia a 2 s de +0.0129 (IC 95% [+0.0113, +0.0144]). Aqui se pregunta si ambas cosas ocurren en otras cadenas OP Stack con otras reglas de bloque.
- Tipo de afirmacion: H1-c es una estimacion de alcance observado; leerla como competencia por estado es una inferencia (la llegada y la comision tambien fijan el orden). H2-c es una cota superior, no una identificacion.
- Poblacion: bloques de Unichain y de OP Mainnet en 12 semanas de 2025 por cadena (domingo a domingo, UTC), sorteadas como se indica abajo. Bots, entidades y pools se definen por semana y por cadena.
- Fuera de alcance: identificar operadores; atribuir diferencias entre cadenas a su diseno, a Flashblocks o a las comisiones (se describen, no se prueban); cualquier cadena distinta de Unichain y OP Mainnet; las 48 semanas de Base del estudio previo, salvo la sensibilidad de comparabilidad declarada abajo.

## Hipotesis

Las definiciones de probe, revert y entidad son las del estudio previo en Base: un probe es una transaccion exitosa de un bot que no emite logs de contratos ajenos y lleva valor de a lo mas 1 wei; un revert es una transaccion de un bot con `success = false`; un bot es un contrato con al menos 50% de sus transacciones de la semana como intentos fallidos; las entidades son las componentes conexas contrato-remitente por semana.

- **H1-c (carrera).** Sea $\Delta_{MH}$ la diferencia de riesgos de Mantel-Haenszel de la tasa de revert entre transacciones con y sin un swap exitoso previo de otra entidad en alguno de sus pools, variante sin probes, con celdas (semana, entidad, decil de posicion en el bloque). H1-c se afirma si $\Delta_{MH} \ge 0.02$ y el IC 95% (bootstrap por semanas completas) excluye 0.
  - Falsada si el limite superior del IC 95% es menor que 0.02.
  - Inconclusa si el IC contiene 0.02, o si hay menos de 1,000 celdas con ambos grupos en total.
  - No medible (ni afirmada ni falsada): si en los bloques ancla de la cadena hay menos de 500 reverts de transacciones que no son probes, tras aplicar la regla de exclusion. En ese caso se reporta la tasa de revert observada como dato. Se espera que pueda ocurrir en Unichain, donde un piloto de 30 bloques no mostro ningun revert; es un resultado valido. El umbral de 500 es un supuesto de precision minima, escrito sin haber visto datos de la muestra.
- **H2-c (sincronia de bloque).** $U_{2s} = C_0 - C_{2s}$, agregada, con $C_0$ la coincidencia de pools entre entidades en el bloque ancla y $C_{2s}$ la misma medida con el bloque situado 2 s despues (+1 bloque en OP, +2 bloques en Unichain). Se declara "sincronia de bloque no mayor a 2 puntos en c" si el limite superior del IC 95% de $U_{2s}$ es menor que 0.02. Si no, se reporta $U_{2s}$ con su IC como cota superior, sin afirmar que exista.
  - En Unichain el vecino a 2 s es el segundo bloque siguiente, no el contiguo: la cota no es sobre el bloque inmediato como en Base. El vecino contiguo (+1 bloque) no se muestrea en este diseno.
- Los umbrales (0.02, 0.02) se copian del estudio previo y no se ajustan. Hay cuatro decisiones por separado (H1 y H2, en dos cadenas); no se combinan ni se elige la favorable. Las sensibilidades no deciden.
- Alternativas plausibles y como se separan:
  - Un resultado positivo es efecto del builder o de transacciones de sistema mal excluidas: se corre con y sin la regla de exclusion.
  - La diferencia con Base es por la definicion de pool: se corre con y sin pools de Uniswap v4 por PoolId.
  - Posicion residual dentro del decil: sensibilidad con 50 tramos.
  - Actividad del bloque (los bloques con mas swaps tienen mas reverts por cualquier causa): sensibilidad con celdas (entidad, bloque).
  - Entidad mal agrupada: sensibilidad sin remitentes que llaman a 10 o mas contratos bot.
  - Deriva entre bloques para H2-c: la mide la curva $C_0, C_{2s}, C_{30s}, C_{5min}$.
  - Regimen de bloque (Flashblocks, comisiones): no se prueba; se reporta $\Delta_{MH}$ por semana.
  - En Unichain, ausencia de reverts por proteccion de la cadena: no se distingue de ausencia de competencia; se declara.
- Condiciones que invalidan el estudio en una cadena: mas del 5% de los bloques sin traza o sin receipts tras reintentos; menos de 1,000 celdas con ambos grupos (H1-c); menos de 2,000 pares ancla-vecino a 2 s con intentos fallidos (H2-c); que la regla de exclusion cambie despues del sello sin declararse como desviacion.

## Evidencia y metodo

- Fuente: Alchemy JSON-RPC (Unichain mainnet y OP mainnet), `debug_traceBlockByNumber` con `callTracer` y `eth_getBlockReceipts`, con el cliente de linea de comandos del estudio (`mev-revert-classifier`), en la version y el commit que se declaran al sellar.
- Unidad: el bloque ancla (muestra aleatoria estratificada por hora); dentro, la transaccion. Los vecinos solo aportan la coincidencia $C_{k}$ de H2-c.
- Regla de sorteo de semanas: semanas calendario de 2025 que inician en domingo UTC (2025-01-05 a 2025-12-28, 52 semanas). Elegibles: en OP Mainnet, las 52; en Unichain, las que inician estrictamente despues del dia de lanzamiento con trafico de usuarios L (46 semanas, ver abajo). Para cada cadena se toman las 12 semanas con menor `sha256("mev-af03-v1:<cadena>:<inicio_de_semana>")`, con `<cadena>` en {`unichain`, `op`} y `<inicio_de_semana>` en formato `AAAA-MM-DD`; se ordenan por fecha y se numeran U01 a U12 y O01 a O12.
  - Unichain (46 elegibles): U01 2025-02-23, U02 2025-03-30, U03 2025-06-01, U04 2025-06-22, U05 2025-06-29, U06 2025-09-07, U07 2025-09-28, U08 2025-10-12, U09 2025-10-19, U10 2025-11-02, U11 2025-12-07, U12 2025-12-21.
  - OP Mainnet (52 elegibles): O01 2025-01-12, O02 2025-01-19, O03 2025-03-23, O04 2025-05-04, O05 2025-06-15, O06 2025-07-13, O07 2025-08-17, O08 2025-09-21, O09 2025-09-28, O10 2025-10-05, O11 2025-10-12, O12 2025-10-19.
- Dia de lanzamiento de Unichain, L = 2025-02-12. Criterio escrito antes de consultar cabeceras de bloque: una sonda es el bloque con timestamp 12:00:00 UTC de un dia; tiene trafico si su cabecera lista al menos 10 transacciones; L es el primer dia cuya sonda y las 6 siguientes tienen trafico. Resultado de ese criterio, con una sonda por dia del 2024-11-05 al 2025-03-01 (117 consultas de solo cabecera): ningun dia lo cumple, porque los bloques de 1 s de Unichain listan pocas transacciones aun con usuarios (a lo mas 7 por bloque en las sondas). Se adopto entonces, tras ver la serie diaria de conteos de ese rango, un segundo criterio: al menos 2 transacciones (una sola es el deposito de atributos L1, que no es trafico de usuarios), mismo esquema de 7 dias consecutivos. Sondas con una sola transaccion hasta 2025-02-11; desde 2025-02-12 todas tienen al menos 2. L = 2025-02-12. Esta modificacion es posterior a ver los conteos y se declara como tal; solo afecta que semanas son elegibles (la primera es la que inicia el 2025-02-16). No se consulto ningun dato de bloques distinto del numero de transacciones de la cabecera de esas sondas.
- Muestreo (identico al del estudio previo): por cada hora UTC de cada semana, las 3 anclas con menor `sha256("mev-contention-v1:<bloque>")`; las dos primeras con vecino a +2 s y la tercera con vecinos a +30 s y +5 min, medidos en reloj de pared. El reloj de bloque es de 2 s en OP Mainnet (desplazamientos +1, +15 y +150 bloques) y de 1 s en Unichain (+2, +30 y +300 bloques). Si un vecino coincide con un ancla u otro vecino, se toma el siguiente candidato de la hora. Por semana: 504 anclas y 672 vecinos; por cadena: 6,048 anclas y 14,112 bloques en total (28,224 entre las dos cadenas). La columna `exploratory` del archivo se fijo en `false` en todas las filas: ningun bloque es exploratorio, todos son confirmatorios.
  - Muestra completa de Unichain (14,112 bloques, columnas `role` y `anchor`), sha256 `3ff72449971e02f9bc636ee338c634d24a7a1d16a39ce9701c02692dace5c742`; solo anclas (6,048), sha256 `ea3bf3e2674eab63bd9823386829b30a4aabf3041a1df4842d90dd65f02925ec`.
  - Muestra completa de OP Mainnet (14,112 bloques), sha256 `a8491e85de4404870257be2d2dcaebbf0641fd95ae0fdba702dc0bbfeb9a5e5b`; solo anclas (6,048), sha256 `9f4c79e8d39a101f668653b15a5276bd5e99f93455fc7df6efc01b0275429755`.
- Regla de exclusion de transacciones de sistema y de builder: una transaccion cuyo remitente o destino esta en la lista de su cadena no entra a la poblacion (ni a bots, ni a entidades, ni a pools). La lista (archivo CSV `cadena,rol,direccion,fuente`) contiene: el deposito de atributos L1 de OP Stack (remitente `0xdeaddeaddeaddeaddeaddeaddeaddeaddead0001`, destino `0x4200000000000000000000000000000000000015`) en las tres cadenas; y en Unichain, ademas, el proxy FlashblockNumber (`0x3c3a8a41e095c76b03f79f70955fff3b03cf753e`), el proxy BlockBuilderPolicy (`0xd44f9d1331659f417a3e22c9e29529d498b66a29`), el FlashtestationRegistry (`0x7289e4e24fd8fb0382432ee7362c8610ef6f791e`) y la cuenta del builder (`0xcabba9e7f4b3a885c5aa069f88469ac711dd4acc`). sha256 del archivo: `f7dfb94922bd6574ffd67e38bbbfdb5431736c8854ed63fca87d6407a836d60e`. La lista no cambia despues del sello; un cambio se declara como desviacion. Se reportan los conteos excluidos por regla y cadena.
  - Juicio declarado: en OP Mainnet, llamadas a protocolos que no son arbitraje (reclamos de recompensas de vaults de Velodrome, oraculos del AMM deportivo de Thales) pueden aparecer en los pilotos como intentos fallidos. No se excluyen en el analisis primario, porque la lista no tiene direcciones completas verificadas para ellas; solo entran al universo si cumplen la regla de bot. Se reportara su peso cuando se identifiquen en los datos; no se corre una sensibilidad de exclusion sobre ellas.
- Definicion de pool (Uniswap v4): el `PoolManager` es un solo contrato que aloja todos los pools de v4; un pool se identifica por su `PoolId` = keccak256 de la codificacion ABI del `PoolKey` (monedas ordenadas, comision, espaciado de ticks, contrato de hooks). En el analisis primario de ambas cadenas, un swap de v4 aporta el pseudo-pool `v4:<PoolId>` y el `PoolManager` deja de contar como un pool (PoolManager de Unichain `0x1f98400000000000000000000000000000000004`; de OP `0x9a13f98cb987694c9f086b1f5eb990eea8264ec3`). Los pools de v2 y v3 siguen siendo la direccion del contrato. Transacciones de un bot sin pool identificable (por ejemplo, prestamos flash del `PoolManager` sin swap) quedan sin pool; se reportan, no se corrigen. En Base (sensibilidad de comparabilidad) el `PoolManager` es `0x498581ff718922c3f8e6a244956af099b2652b2b`.
- Variables (H1-c): $X(t) = 1$ si otra entidad tuvo un swap exitoso antes en el bloque en algun pool de $P(t)$ (el conjunto de pools de la transaccion $t$); $Y(t) = 1$ si $t$ es revert. Pesos (H2-c): $w_p = 1/n_p$ calculados solo sobre anclas.
- Analisis primario, por cadena c y con `--chain <c>` (la exclusion de sistema es la opcion por defecto en ambas cadenas):
  - Descarga: `mev-revert-classifier fetch --chain <c> --v4-pools --blocks <muestra_completa_c> --split confirmatory --raw-dir <crudo_c> --out <filas_c>` (con `--seal` del archivo de sello). Se descargan las 12 semanas completas de la cadena antes de correr cualquier analisis sobre ella.
  - H1-c: `mev-revert-classifier race --chain <c> --rows <filas_c> --raw <crudo_c> --blocks <anclas_c> --split confirmatory --boot-unit week --bootstrap 1000 --permutations 1000 --seed 20261017`; se lee la linea agregada, variante sin probes.
  - H2-c: `mev-revert-classifier neighbor --chain <c> --rows <filas_c> --blocks <muestra_completa_c> --split confirmatory --boot-unit week --bootstrap 1000 --seed 20261017`; se lee $U_{2s}$ de la linea agregada.
  - IC por bootstrap de semanas completas (12 unidades por cadena): la dependencia dentro de la semana queda dentro de la unidad. Con 12 unidades el IC es menos confiable que con 48 en el estudio previo; se declara junto a cada IC.
- Sensibilidades (se reportan todas, no deciden):
  - Con pools por direccion, sin pseudo-pools de v4 (filas re-extraidas del mismo crudo, sin `--v4-pools`).
  - Sin la regla de exclusion (`--exclude-system false`).
  - H1-c con 50 tramos de posicion (`--bins 50`), con celdas (entidad, bloque) (`--cell-block`), con probes, con bootstrap de horas (`--boot-unit hour`) y sin remitentes de grado 10 o mayor (`--max-sender-degree 10`).
  - H2-c con $U_{30s}$ y $U_{5min}$, sin vecinos vacios (`--drop-blocks-without-rows`) y con bootstrap de horas.
  - La distribucion de $\Delta_{MH}$ por semana (en cuantas semanas es mayor que 0) y la cobertura de pools de los intentos fallidos con y sin v4.
  - Sensibilidad de comparabilidad con Base: las 48 semanas de 2025 del estudio previo, re-extraidas sin costo de RPC desde el crudo ya guardado, con la regla de este estudio (`--v4-pools` y exclusion de sistema, `--chain base`), se reportan como analisis aparte; no reemplazan el resultado previo de Base.
  - La comparacion entre cadenas (Base, Unichain, OP Mainnet) es descriptiva: diferencia de $\Delta_{MH}$ y de $U_{2s}$ con IC por bootstrap independiente por cadena, declarando 48 contra 12 semanas. No se afirma por que difieren.
- Multiplicidad: cuatro afirmaciones distintas con criterio propio; no se combinan, no se calculan p-values para decidir (decide el umbral y el IC). Las permutaciones solo dan p-values descriptivos.
- Verificacion cruzada: una segunda implementacion independiente de ambas estimaciones (en Python) sobre las mismas filas debe dar las mismas estimaciones puntuales; si no, el estudio se detiene y se registra. Calibracion y poder de `race` (X aleatorio dentro de celdas y efecto sembrado de 5 puntos, sobre datos sinteticos) son pruebas automaticas del codigo en el commit sellado y deben pasar antes de abrir los datos; el estimador no depende de la cadena (`--chain` solo fija el reloj de bloque y la lista de exclusion).

## Ejecucion

- Presupuesto y tamano justificado: 28,224 bloques entre las dos cadenas. Con unos 62 CU por bloque medidos en un piloto de 60 bloques (estimacion) son unos 1.75 M CU, alrededor de US$0.9; el tope del plan es US$5 y el saldo antes del estudio es US$2.88. El estudio no gasta mas de US$1.00 sin una decision explicita adicional. Disco: unos 4.5 KB por bloque en Unichain y unos 85 KB en OP en el piloto, menos de 2.5 GB en total.
- Criterio de parada: se descargan las 24 semanas completas antes de correr `race` o `neighbor` sobre cualquiera. Durante la descarga solo se miran cobertura y errores. Una sola corrida primaria por cadena. Si el gasto se acerca al tope sin las semanas completas, se detiene y se reporta.
- Semillas: hash deterministico para el muestreo; 20261017 para bootstrap y permutaciones.
- Artefactos esperados: crudo (receipts y trazas) por cadena con sha256 en manifiesto, filas por cadena, salidas de `race` y `neighbor`, verificacion cruzada, registro de ejecucion y reporte de veredicto.
- Verificacion externa prometida: ninguna independiente; la reproduccion la hace el mismo autor con codigo y muestra publicados.

## Riesgos y autoridad

- Etica y privacidad: datos publicos on-chain, contratos de bots; tablas publicadas con al menos 5 transacciones por celda.
- Licencias: terminos del proveedor de RPC.
- Conflictos de interes: el autor presenta resultados de este estudio en una charla y en un preprint; el resultado se reporta sea cual sea, incluida la falsacion de cualquier hipotesis o el resultado "no medible".
- Falsacion y limites: H1-c se falsa si el limite superior del IC 95% de $\Delta_{MH}$ es menor que 0.02 en la cadena; H2-c solo se afirma como "no mayor a 2 puntos" bajo el criterio indicado y de otro modo se reporta como cota. Una lectura causal de las diferencias entre cadenas queda como inferencia.

## Sellado

- Responsable: Keneth Issac Huerta Galindo.
- Version o hash: el sha256 de este archivo se registra en un tercero antes de descargar datos (ver la ultima seccion) y junto con el de las muestras y la lista de exclusion, el codigo en el commit declarado al sellar y las versiones de dependencias.
- Evidencia observada antes de este registro:
  - El resultado del estudio previo en Base (48 semanas de 2025).
  - Pilotos de 30 bloques por cadena de los ultimos 7 dias previos a este registro (Unichain y OP Mainnet), fuera de las semanas sorteadas, usados para identificar las transacciones de sistema y de builder y para medir tamanos y cobertura de pools. No son estimaciones poblacionales.
  - Las 117 cabeceras de bloque de Unichain descritas arriba (solo numero de transacciones).
  - Ningun bloque, receipt ni traza de las 24 semanas sorteadas se ha descargado.

## Publicacion y registro externo

- Se publicara o citara fuera del repo privado: si.
- Forma publicable: si.
- Registro externo, a llenar ANTES de abrir la evidencia reservada. Este archivo se registra con esta seccion en blanco, para que su sha256 sea el que se deposita; los datos del deposito se anotan fuera de este archivo y se enlazan a este hash.
  - Servicio:
  - Identificador (URL o DOI):
  - Fecha y hora UTC del registro:
  - sha256 del archivo publicable:
- Caso retroactivo: no aplica.
