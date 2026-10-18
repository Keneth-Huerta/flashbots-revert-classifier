---
estado: VIVO
categoria_documental: FUNDAMENTO
titulo: Fundamento Teorico - Identidad de pool en Uniswap v4 y su comparabilidad con pools por direccion
ultima_actualizacion: 2026-10-18
plantilla: PLANTILLA_FUNDAMENTO_TEORICO 1.0.0
complementa: docs/07_fundamentos_teoricos/contencion-entre-bots.md (§4.2, limite 5.2)
---

# Fundamento Teorico: Identidad de pool en Uniswap v4

## 1. Objetivo

El fundamento de contencion (`contencion-entre-bots.md` §4.2) define el pool como una
direccion de contrato: la que recibe una llamada de lectura o swap, o la que emite un evento
`Swap`. Eso no se sostiene en Uniswap v4: todos los pools viven dentro de un solo contrato, el
`PoolManager`, y el pool se distingue por un identificador de 32 bytes. El fundamento vigente
lo trata como un solo pool (limite 5.2), lo que une en un solo "pool" a todos los pools de v4 de
la cadena. En Unichain, donde casi toda la actividad DEX es v4, el extractor no ve pools; en
Base y OP, el `PoolManager` mezcla pools que no compiten entre si. Este documento define la
identidad de pool de v4 y argumenta que es la unidad comparable con el pool por direccion de
v2/v3, para que AF-03 compare cadenas con la misma definicion de oportunidad. No altera ni
reemplaza §4.2: la identidad de v4 es una opcion explicita del extractor.

## 2. Definiciones y Notacion

- $M_c$: direccion del `PoolManager` de la cadena $c$ (despliegue oficial de Uniswap v4).
- `PoolKey` $=(c_0, c_1, f, s, h)$: las dos monedas ordenadas, la comision, el espaciado de ticks
  y el contrato de hooks. Son 5 palabras de 32 bytes.
- `PoolId` $\;\iota(k) = \mathrm{keccak256}(\mathrm{abi.encode}(k))$, hash de las 160 bytes del
  `PoolKey` (`PoolId.toId`).
- Pseudo-pool de v4: la cadena `v4:0x<PoolId>` (66 caracteres tras el prefijo), que ocupa en
  $P(t)$ el lugar de una direccion de pool.
- Desbloqueo: todo cambio de estado en $M_c$ ocurre dentro de `unlock(bytes)` (0x48c89491), que
  llama de vuelta a `unlockCallback(bytes)` (0x91dd7346) del contrato que lo invoco; dentro
  del callback se llama a `swap` (0xf3cd914c), `take`, `sync`, `settle`, etc.

## 3. Marco Conceptual

Un pool de v2 o v3 es un contrato con su propio estado. El proxy de oportunidad de §4.2 es
"dos transacciones tocan el mismo estado de precio". En v4 el estado de cada pool es la
entrada $\iota(k)$ del mapping `_pools` del `PoolManager`; dos transacciones que operan el
mismo `PoolKey` leen y escriben el mismo estado de precio, y dos que operan `PoolKey`
distintos no lo comparten aunque llamen al mismo contrato. Por eso la direccion de $M_c$ no es
la unidad correcta: lo es $\iota(k)$.

La identidad por `PoolId` es comparable con la direccion de pool porque cumple lo que §4.2
necesita del pool y nada mas:

1. Es una clave del estado de precio (una sola por par, comision, espaciado y hooks).
2. Cualquier transaccion que lo toca deja un rastro observable en la traza o en los logs,
   incluso si revierte (ver abajo).
3. Un pool de v3 y un pool de v4 del mismo par son pools distintos tambien en la
   definicion por direccion (contratos distintos); la unidad no los mezcla.

Lo que cambia es el efecto sobre $\mathcal{U}_b$: el `PoolManager` deja de ser miembro del
universo de direcciones y entran los `PoolId` observados en el bloque.

## 4. Formulacion Matematica

Para una transaccion $t$ en el bloque $b$, sea $V(t)$ el conjunto de `PoolId` que toca:

- $V_{\text{call}}(t)$: $\iota(k)$ de cada llamada `swap` a $M_c$ en el arbol de la traza,
  con $k$ las 160 bytes que siguen al selector. Existe tambien si la transaccion revierte,
  porque `callTracer` conserva los argumentos de las llamadas fallidas.
- $V_{\text{log}}(t)$: `topics[1]` de cada evento
  `Swap(bytes32 indexed id, address indexed sender, int128 amount0, int128 amount1,
  uint160 sqrtPriceX96, uint128 liquidity, int24 tick, uint24 fee)` emitido por $M_c$
  (topic0 `0x40e9cecb9f5f1f1c5b9c97dec2917b7ee92e57ba5563708daca94dd84ad7112f`). Solo
  existe si la transaccion no revierte.
- Universo del bloque: $\mathcal{V}_b = \bigcup_{t \in b} V_{\text{call}}(t) \cup V_{\text{log}}(t)$.
- $V_{\text{read}}(t)$: lecturas de estado sin swap, restringidas a $\mathcal{V}_b$: (a) un
  argumento de 32 bytes de una llamada cualquiera igual a un $p \in \mathcal{V}_b$
  (`StateView.getSlot0(id)`, 0xc815641c); (b) un argumento de `extsload` a $M_c$ igual a
  $\mathrm{keccak256}(p \Vert 0^{31}6) + j$, $j \in \{0,1,2,3\}$, las palabras de `slot0`,
  crecimiento de comision global y liquidez (`StateLibrary`, `POOLS_SLOT = 6`).

$$V(t) = V_{\text{call}}(t) \cup V_{\text{log}}(t) \cup V_{\text{read}}(t),
\qquad P_{\text{v4}}(t) = P(t) \setminus \{M_c\} \;\cup\; \{\texttt{v4:}p : p \in V(t)\},$$

donde $P(t)$ es el de §4.2 y $\mathcal{U}_b$ pierde $M_c$. La metrica de §4.4 y §4.4 bis no
cambia: solo recibe elementos de pool de otra clase.

Clasificacion (§4.3): no cambia. Una transaccion con unlock y swap emite `Swap` desde
$M_c \neq \text{to}(t)$, es un log externo y la transaccion es exitosa; con estado 0 es revert; un
unlock que no emite logs (lectura con `extsload`, o `take`/`settle` sin swap) es probe.

Supuestos:

- Solo `swap` cuenta como toque de un pool; `modifyLiquidity`, `donate` e `initialize` no
  (mismo criterio que §4.2, que no cuenta la provision de liquidez).
- Los hooks no emiten `Swap` con el topic de $M_c$ ni cambian la identidad.

## 5. Limites, Riesgos y Casos de Falla

- Una lectura por `extsload` de un pool que nadie swapea en el bloque no se atribuye: el slot
  es un hash que solo se invierte contra candidatos de $\mathcal{V}_b$. Es el mismo limite que el
  universo por bloque de §4.2 (un pool sin swap en el bloque no entra en $\mathcal{U}_b$).
- Las lecturas de `extsload` sobre ticks, bitmaps y posiciones usan claves hasheadas con otros
  datos y no se atribuyen; solo las cuatro palabras de estado se atribuyen.
- `take`/`sync`/`settle` sin swap usan al `PoolManager` como fuente de prestamo flash. Con la
  definicion antigua esa transaccion "toca el pool" `PoolManager`; con la de v4 no toca ninguno.
  Esto reduce la contencion espuria, pero cambia los resultados de Base si se activa la opcion.
- El mismo `PoolId` solo identifica un pool dentro de una cadena: la comparacion entre cadenas
  no mezcla identificadores.
- Otros singleton (p. ej. Balancer v3 Vault) tienen el mismo problema y no estan cubiertos.

## 6. Implicaciones para Arquitectura y ADR

- Opcion `--v4-pools` de `fetch` y `--chain base|unichain|op`; por defecto el extractor y sus
  salidas no cambian (Base, AF-01 y AF-02).
- `src/extract.rs` (`extract_block_with`, `V4Mode`), `src/keccak.rs`, `src/chain.rs`.
- ADR pendiente: decidir si AF-03 preregistra la opcion como definicion primaria de pool en
  las tres cadenas o como sensibilidad.

## 7. Criterios de Validacion Empirica

| Criterio | Metrica | Umbral | Resultado esperado |
|----------|---------|--------|--------------------|
| La identidad calculada de la llamada coincide con la del evento | `PoolId` de `swap` igual a `topics[1]` en transacciones exitosas del piloto | 100% | Coincide (tx Unichain 0xf302d431, test `v4.rs`) |
| Las palabras de estado son las de `extsload` | `keccak256(id . 6)` igual al argumento de `extsload` observado | igualdad | Coincide (tx Unichain 0xfed70c37) |
| El cambio no altera Base por defecto | salida de `fetch --offline` sin la opcion | identica | Identica en piloto y 60 bloques de la muestra |
| Reverts de v4 reciben pool | revert con `swap` a $M_c$ | pseudo-pool presente | Presente (tests) |

## 8. Referencias

- Uniswap v4-core: `PoolId.sol`, `PoolKey.sol`, `IPoolManager.sol` (evento `Swap`, `swap`,
  `unlock`), `StateLibrary.sol` (`POOLS_SLOT`).
- Despliegues oficiales de v4: `https://docs.uniswap.org/contracts/v4/deployments`.
- `docs/07_fundamentos_teoricos/contencion-entre-bots.md` §4.2, §5.2.
- `docs/00_registro_exploracion.md`: EXPL-21 (piloto de cadenas), EXPL-26.
