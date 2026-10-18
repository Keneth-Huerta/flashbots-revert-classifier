---
estado: VIVO
categoria_documental: FUNDAMENTO
titulo: Fundamento Teorico - Contencion entre bots en intentos fallidos de arbitraje
ultima_actualizacion: 2026-10-17
plantilla: PLANTILLA_FUNDAMENTO_TEORICO 1.0.0
reemplaza: docs/07_fundamentos_teoricos/clasificacion-reverts-mev.md
---

# Fundamento Teorico: Contencion entre bots en intentos fallidos de arbitraje

## 1. Objetivo

Definir como se mide si los intentos fallidos de bots de arbitraje en Base ocurren en
contencion con otros bots por la misma oportunidad, y como se separa esa contencion de la
coincidencia. Sustituye la heuristica de densidad por bot de la v0.1.0, que no podia medir
competencia (ERR-01). Es agnostico al lenguaje.

## 2. Definiciones y Notacion

- $\mathcal{B}$: bloques sorteados de una semana-estrato; $h(b)$ es la hora UTC del bloque $b$.
- Transaccion $t$: `from`, `to`, estado, logs, valor y arbol de llamadas (traza `callTracer`).
- Bot: direccion `to` de un contrato que cumple los criterios de 4.1.
- Intento fallido: probe o revert (glosario), enviado a un bot.
- $P(t)$: conjunto de pools tocados por $t$ (4.2). Proxy de la oportunidad.
- $F$: intentos fallidos con $P(t) \neq \emptyset$; $A$: todas las transacciones a bots con $P(t) \neq \emptyset$ (fallidas y exitosas).
- Paquete $X_{v,b}$: transacciones del bot $v$ en el bloque $b$ (puede ser vacio).

## 3. Marco Conceptual

Si dos bots distintos persiguen la misma oportunidad, sus transacciones leen los mismos
pools en la misma ventana. Pero dos bots tambien pueden leer el mismo pool sin competir:
porque ambos escanean pools populares todo el tiempo. Por eso el traslape observado se
compara contra un modelo nulo que conserva todo lo que cada bot hace por su cuenta (cuantas
transacciones manda, en cuantos bloques aparece, que pools prefiere) y solo destruye la
sincronia entre bots. Lo que sobra por encima del nulo es contencion atribuible a
oportunidades compartidas en el tiempo.

## 4. Formulacion Matematica

### 4.1 Universo de bots y entidades (`SUPUESTO`)

Una direccion $v$ es bot en la semana-estrato si:

1. recibe al menos 5 intentos fallidos con $P(t) \neq \emptyset$ en $\mathcal{B}$, y
2. al menos la mitad de sus transacciones en $\mathcal{B}$ son intentos fallidos.

El criterio 2 excluye routers publicos y agregadores, que casi siempre tienen exito; los
bots tienen tasas de exito de 4.6% a 42.5% (Wu y Oz, Tabla 1). Version anterior: "a lo mas
3 remitentes distintos"; se descarto con el set exploratorio porque los bots grandes rotan
cientos de EOAs y dejaba fuera el 93% de los intentos fallidos (EXPL-10).

**Entidades.** Los bots se agrupan en componentes conexas del grafo bipartito
contrato–remitente: dos contratos que comparten algun `from` son el mismo operador. La
contencion se cuenta entre entidades, no entre contratos.

Sensibilidades: contar contratos sin agrupar; exigir al menos un arbitraje ciclico exitoso
en $\mathcal{B}$ (como Wu y Oz); umbral de proporcion 0.25 y 0.75. Un arbitraje ciclico
exitoso es una transaccion exitosa con al menos 2 eventos Swap y un token que el bot envia y
recibe con contrapartes distintas del remitente de la transaccion.

### 4.2 Pools tocados

Para cada bloque $b$ se define el universo de pools $U_b$: las direcciones que reciben, en
cualquier transaccion de $b$, una llamada con un selector de lectura o swap de pool
(`getReserves` 0x0902f1ac, `slot0` 0x3850c7bd, `globalState` 0xe76c01e4, `liquidity`
0x1a686502, `swap` V2 0x022c0d9f, `swap` V3 0x128acb08, `extsload` 0x1e2eaeaf, 0x35fd631a,
0xdbd035ff, `token0` 0x0dfe1681, `token1` 0xd21220a7, `ticks` 0xf30dba93, `tickBitmap`
0x5339c296, `positions` 0x514ea4bf), mas las que emiten un evento Swap (Uniswap V2/V3/V4, Aerodrome, PancakeSwap V3)
en $b$. Entonces

$$P(t) = \big(\text{callees}(t) \cup \text{args}_{\texttt{balanceOf}}(t)\big) \cap U_b \setminus \{\text{to}(t)\},$$

donde $\text{args}_{\texttt{balanceOf}}(t)$ son las direcciones consultadas con `balanceOf`
(0x70a08231), que capturan las lecturas por saldo. El universo es por bloque para no guardar
el arbol completo de llamadas (ADR 002).

El PoolManager singleton de Uniswap V4 cuenta como un solo pool (limite 5.2).

### 4.3 Clasificacion de transacciones

- Revert: estado 0.
- Probe: estado 1, ningun log emitido por una direccion distinta de `to`$(t)$, valor ≤ 1 wei.
- Exitosa: estado 1 con al menos un log externo.

### 4.4 Metrica primaria

Un intento fallido $f \in F$ del bot $v$ en el bloque $b$ esta en contencion si

$$\exists\, t \in A:\ \text{bloque}(t) = b,\ \text{to}(t) \neq v,\ P(t) \cap P(f) \neq \emptyset.$$

$$CR = \frac{|\{f \in F : f \text{ en contencion}\}|}{|F|}.$$

### 4.4 bis Metrica ponderada por especificidad (primaria, ADR 003)

Sea $n_p$ el numero de paquetes entidad-bloque de la semana-estrato que tocan el pool $p$, y
$w_p = 1/n_p$. Para un intento fallido $f$ de la entidad $v$ en el bloque $b$, sea
$S(f) = \{p \in P(f) : \exists\, t \in A,\ \text{bloque}(t) = b,\ \text{entidad}(t) \neq v,\ p \in P(t)\}$.

$$s(f) = \frac{\sum_{p \in S(f)} w_p}{\sum_{p \in P(f)} w_p}, \qquad CR_w = \frac{1}{|F|}\sum_{f \in F} s(f).$$

$s(f) \in [0, 1]$ es la fraccion de la masa de especificidad del intento que otra entidad
tambien toca en el bloque. Un pool concentrador pesa poco; un pool que solo toca un paquete
en la semana nunca cuenta como compartido. Los pesos se calculan una vez con la muestra de
la semana y el nulo los conserva, porque permutar paquetes no cambia $n_p$. La metrica
binaria $CR$ de §4.4 es el caso $s(f) = \mathbb{1}[S(f) \neq \emptyset]$.

### 4.5 Modelo nulo

En cada hora $H$, sea $\mathcal{B}_H$ el conjunto de bloques sorteados de esa hora. Para cada
bot $v$ se toma una permutacion uniforme $\pi_{v,H}$ de $\mathcal{B}_H$, independiente entre
bots, y el paquete $X_{v,b}$ se mueve al bloque $\pi_{v,H}(b)$. Se recalcula CR. Con $K$
permutaciones se obtiene $CR^{(1)},\dots,CR^{(K)}$.

- Exceso: $E = CR_{obs} - \frac{1}{K}\sum_k CR^{(k)}$.
- Valor p de una cola: $p = \frac{1 + |\{k : CR^{(k)} \ge CR_{obs}\}|}{K + 1}$.

El nulo conserva el numero y el tamano de los paquetes de cada bot y sus pools; solo rompe
en que bloque cae cada paquete dentro de la hora.

### 4.6 Incertidumbre

Bootstrap por conglomerados de hora: cada replica remuestrea con reemplazo las 168 horas de
la semana y conserva cada hora intacta, con todos sus bloques. Se recalcula $CR_{obs}$ y,
con $K' < K$ permutaciones dentro de cada hora remuestreada, la media del nulo; el IC 95%
de $E$ es el percentil de $CR^{*}_{obs} - \overline{CR}^{*}_{nulo}$. La version anterior
remuestreaba bloques dentro de la hora: un bloque repetido dentro de una hora deja el nulo
mal definido y el IC de $E$ podia excluir a $E$ (EXPL-11).

### 4.7 Supuestos

- Una oportunidad compartida implica pools compartidos dentro del mismo bloque de 2 s.
- La sincronia entre bots dentro de la hora no la produce un ritmo comun ajeno a las oportunidades (por ejemplo, picos de minuto); el estrato horario lo atenua, pero no lo elimina.

## 5. Limites, Riesgos y Casos de Falla

- **Riesgo 1: pools concentradores.** Si todos leen WETH/USDC en cada bloque, CR es alto en ambos lados y E sale cerca de 0 aunque haya competencia. Sensibilidad: repetir sin los 10 pools mas leidos.
- **Riesgo 2: singleton V4.** Todo lo de V4 parece el mismo pool: infla el traslape. Sensibilidad: repetir sin el PoolManager.
- **Riesgo 3: universo de bots.** Umbrales 5 y 3 elegidos a priori; sensibilidad con el subconjunto con arbitraje exitoso.
- **Riesgo 4: pocos bloques por hora.** Con 6 bloques por hora el nulo tiene $6!$ arreglos por bot; suficiente con varios bots, pobre si una hora tiene un solo bot activo (esas horas aportan cero a ambos lados).
- **Riesgo 5: ventana.** Un bloque de 2 s contiene 10 Flashblocks; la contencion intra-Flashblock no se separa aqui (exploratorio por `tx_index`).
- **Riesgo 6: bots siempre presentes.** Un bot activo en todos los bloques con los mismos pools no cambia bajo la permutacion: su traslape no se distingue de la coincidencia y E queda en 0 aunque compita. El nulo solo detecta contencion por oportunidades que varian de bloque a bloque.

## 6. Implicaciones para Arquitectura y ADR

- ADR relacionado: `docs/adrs/002-ingesta-rpc-trazas.md`.
- Modulos impactados: ingesta RPC (trazas y receipts), extraccion de $P(t)$, analisis de contencion con permutaciones y bootstrap (RNG con semilla fija).

## 7. Criterios de Validacion Empirica

| Criterio | Metrica | Umbral | Resultado esperado |
|----------|---------|--------|--------------------|
| Nulo calibrado | p bajo datos sinteticos sin sincronia | fraccion de p < 0.05 entre 0.03 y 0.07 | nulo sin sesgo |
| Potencia | p con sincronia sembrada | p < 0.01 | detecta contencion sembrada |
| Cobertura de trazas | transacciones con traza | ≥ 95% | estudio valido |
| Cobertura de pools | intentos fallidos con $P(t) \neq \emptyset$ | ≥ 50% | resultado concluyente |

## 8. Referencias

- Wu, F. y Oz, B. (2026). *To Wait or to Probe: Arbitrage Competition on High-Throughput Blockchains.* AFT 2026, LIPIcs 24. doi:10.4230/LIPIcs.AFT.2026.24.
- Good, P. (2005). *Permutation, Parametric and Bootstrap Tests of Hypotheses.* Springer.
- Davison, A. C. y Hinkley, D. V. (1997). *Bootstrap Methods and Their Application.* Cambridge University Press.
