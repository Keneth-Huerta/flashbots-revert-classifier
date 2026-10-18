---
estado: VIVO
categoria_documental: FUNDAMENTO
titulo: Fundamento Teorico - Nulos secundarios de contencion (co-presencia y orden en el bloque)
ultima_actualizacion: 2026-10-17
plantilla: PLANTILLA_FUNDAMENTO_TEORICO 1.0.0
---

# Fundamento Teorico: Nulos secundarios de contencion

## 1. Objetivo

Separar dos lecturas del exceso E de AF-01 que el nulo primario no distingue: que las
entidades coincidan en los mismos bloques (co-presencia, posible choque comun) y que, al
coincidir, persigan los mismos pools; y corregir el confusor del estudio de eventos. Este
documento no cambia `contencion-entre-bots.md`, que esta sellado con AF-01; lo extiende.
Ambos analisis son exploratorios: el confirmatorio de AF-01 ya se abrio.

## 2. Definiciones y Notacion

Las de `contencion-entre-bots.md` §2 y §4: entidad $v$, hora $H$, bloques sorteados
$\mathcal{B}_H$, paquete $X_{v,b}$, pools $P(t)$, intentos fallidos $F$, metrica $CR_w$.

## 3. Marco Conceptual

El nulo primario rompe a la vez en que bloques esta cada entidad y que pools toca en ellos.
Si conservamos la presencia y solo barajamos el contenido, lo que queda de exceso es
coincidencia por la oportunidad concreta del bloque. Por otro lado, perder una carrera por
estado deja una huella de orden: el perdedor va despues del ganador.

## 4. Formulacion Matematica

### 4.1 Segundo nulo: permutacion entre bloques activos (exploratorio (e))

El nulo primario (`contencion-entre-bots.md` §4.5) mueve el paquete de la entidad $v$ a cualquier bloque de la hora, tambien a
uno donde $v$ no estuvo. Rompe dos cosas a la vez: **co-presencia** (en que bloques coinciden
las entidades) y **eleccion de pools dado que coinciden**. Un choque comun a nivel de bloque
(mas oportunidades o mas volatilidad en ciertos bloques) produce co-presencia sin que las
entidades persigan los mismos pools, y el nulo primario lo cuenta como exceso.

Sea $\mathcal{B}_{v,H} = \{b \in \mathcal{B}_H : X_{v,b} \neq \emptyset\}$ el conjunto de bloques de la hora donde $v$ tiene
paquete. El segundo nulo toma una permutacion uniforme $\sigma_{v,H}$ de $\mathcal{B}_{v,H}$, independiente
entre entidades, y mueve $X_{v,b}$ a $\sigma_{v,H}(b)$. Conserva exactamente el patron de
presencia de cada entidad (en que bloques esta) y rompe solo que contenido de pools cae en
cada uno de sus bloques. Con $CR_w^{(e,k)}$ las replicas:

$$E_e = CR_{w,obs} - \frac{1}{K}\sum_k CR_w^{(e,k)}.$$

Lectura: $E_e > 0$ indica que, cuando dos entidades coinciden en un bloque, comparten mas
pools de los que tocarian con su contenido de otro bloque de la misma hora: la coincidencia
es por la oportunidad de ese bloque y no solo por presencia. $E - E_e$ estima la parte del
exceso que se explica por co-presencia. Limite: una entidad con un solo bloque activo en la
hora queda fija, y una hora con todas las entidades en un solo bloque aporta $E_e = 0$; se
reporta la fraccion de intentos fallidos en entidades-hora con $|\mathcal{B}_{v,H}| \ge 2$.
Mismo bootstrap de horas (`contencion-entre-bots.md` §4.6). Exploratorio: el confirmatorio ya se abrio ([PRE-02]).

### 4.2 Estudio de eventos: orden dentro del bloque (exploratorio (d), corregido)

La version anterior comparaba los fallos de otras entidades alrededor de exitos ciclicos con
los de exitos no ciclicos. Confusor: los dos grupos difieren en numero de pools, actividad
del bloque y entidad, asi que la diferencia no aisla nada.

Pregunta discriminante: si un intento fallido perdio frente a un arbitraje exitoso que
compartia pools, deberia ir **despues** del exito en el orden del bloque (encontro el estado
ya consumido). Para cada exito $s$ de la entidad $u$ en el bloque $b$ con pools $P(s)$, sea
$O(s) = \{f \in F : \text{bloque}(f) = b,\ \text{entidad}(f) \neq u,\ P(f) \cap P(s) \neq \emptyset\}$ y

$$D = \frac{\sum_s |\{f \in O(s) : \text{idx}(f) > \text{idx}(s)\}|}{\sum_s |O(s)|}.$$

Nulo: dentro de cada bloque se permuta uniformemente el `tx_index` entre las transacciones de
bots con pools del bloque (exitos y fallos juntos), conservando cuales son y sus pools; se
recalcula $D$. El exceso $D_{obs} - \bar{D}_{nulo}$ y su p de una cola se reportan por semana
y por tipo de fallo (revert y probe por separado), con bootstrap de horas. Prediccion bajo
competencia por estado: exceso positivo en reverts; en probes, que no dependen de que el
estado se consuma, cercano a 0. Limite: el orden en Base lo fija la llegada al Flashblock y
la prioridad de fee, no la causalidad; un exceso indica orden compatible con perder la
carrera, no prueba la causa.


### 4.3 Deriva dentro de la hora: curva por distancia y contraste con el vecino

El nulo primario supone bloques intercambiables dentro de la hora. Si el traslape entre
entidades decae con la distancia temporal (deriva de que pools estan "calientes"), $E$ mezcla
"mismo bloque" con "mismo momento". Sea, para intentos fallidos de la entidad $v$ en $b$ y la
actividad de las demas entidades en $b'$,
$s_{b,b'}(f) = \sum_{p \in S_{b'}(f)} w_p / \sum_{p \in P(f)} w_p$, con
$S_{b'}(f) = \{p \in P(f): \exists t,\ \text{bloque}(t) = b',\ \text{entidad}(t) \neq v,\ p \in P(t)\}$.
La curva $C(\Delta) = $ media de $s_{b,b'}$ sobre pares con $|ts(b) - ts(b')| \in \Delta$ describe
la deriva; $C(0) = CR_{w,obs}$. Estadistico de sincronia de bloque libre de deriva:

$$E_{vec} = C(0) - C(\Delta_{min}),$$

con $\Delta_{min}$ la menor distancia disponible. En la muestra actual los bloques de una hora
distan en promedio ~10 min; el diseno que lo resuelve baja ademas $b+1$ (2 s), $b+15$ (30 s)
y $b+150$ (5 min) para un subconjunto de bloques (AF-02). Con la muestra actual, $C(\Delta)$
por tramos de 2-5, 5-10, 10-20, 20-40 y 40-60 min es sensibilidad exploratoria.

### 4.4 Robustez a la definicion de entidad y a la concentracion

$CR_w$ es una media por intento: si pocas entidades concentran los intentos, la afirmacion
es sobre ellas. Se reportan (a) $E$ sin cada una de las 5 entidades con mas intentos
(leave-one-out), (b) $CR_w$ con igual peso por entidad (media de medias por entidad) y su
nulo, y (c) variantes de agrupamiento: union solo con ≥2 remitentes compartidos y sin
remitentes de grado ≥10 (relayers o bundlers que llaman a muchos contratos).

### 4.5 Dependencia entre horas

El bootstrap de horas trata las 168 horas como independientes. Se agrega un bootstrap de
bloques de horas contiguas (6 y 12 h) y un jackknife por dia (7 dias), como cotas de
sensibilidad para el ancho del IC.

### 4.6 Prueba de carrera dentro del bloque (exploratoria en AF-01, EXPL-24; primaria candidata de AF-02)

Universo: tx $t$ de entidades bot con $P(t) \neq \emptyset$, sin probes (variante principal).
Sea $X(t) = 1$ si otra entidad $e' \neq e(t)$ tuvo un swap exitoso (log Swap en receipt) en
el mismo bloque, con $\text{tx\_index} < \text{tx\_index}(t)$, en algun pool de $P(t)$; y
$Y(t) = 1$ si $t$ es revert. Celdas $c = (e, d)$ con $d$ el decil de
$q = \text{tx\_index} / (\max \text{tx\_index}_b + 1)$. Con $n_{1c}, n_{0c}$ tx con $X = 1$ y
$X = 0$ y tasas $\hat r_{1c}, \hat r_{0c}$:

$$\Delta_{MH} = \frac{\sum_c w_c(\hat r_{1c} - \hat r_{0c})}{\sum_c w_c}, \qquad w_c = \frac{n_{1c} n_{0c}}{n_{1c} + n_{0c}},$$

sobre celdas con ambos grupos. Bajo $H_0$ ($Y \perp X$ dado la celda), permutar $X$ dentro de
cada celda deja $E[\Delta_{MH}] = 0$ exacto, porque cada $d_c$ es diferencia de medias de una
particion aleatoria de tamaño fijo; el p de una cola es $(1 + \#\{\Delta^* \ge \Delta\})/(K+1)$.
La permutacion trata las tx como intercambiables dentro de la celda; como hay dependencia
dentro del bloque y de la hora, el IC se toma por bootstrap de unidades grandes (horas en
AF-01; semanas completas en AF-02) y el criterio se fija sobre el IC, no sobre el p.

Lo que $\Delta_{MH} > 0$ no establece: causalidad (llegada y fee fijan el orden; $X$ solo ve
swaps exitosos). Rivales: posicion residual dentro del decil (sensibilidad con 50 tramos) y
actividad del bloque (sensibilidad con celdas $(e, b)$, que absorbe todo lo comun al bloque).

## 5. Limites, Riesgos y Casos de Falla

- **Entidades con un solo bloque activo** quedan fijas en el segundo nulo; si dominan, $E_e$ pierde poder. Se reporta su peso.
- **Orden no causal:** el `tx_index` en Base depende de la llegada al Flashblock y de la prioridad de fee.
- **Multiplicidad:** ambos se reportan por semana sin corregir como exploratorios; no deciden AF-01.

## 6. Implicaciones para Arquitectura y ADR

- `contend` agrega `--null active` (segundo nulo) sin cambiar el nulo por defecto.
- `scripts/analisis/eventos.py` reemplaza la comparacion contra exitos no ciclicos por el estadistico $D$ y su nulo de orden.

## 7. Criterios de Validacion Empirica

| Criterio | Metrica | Umbral | Resultado esperado |
|----------|---------|--------|--------------------|
| Segundo nulo calibrado | p bajo datos sinteticos sin sincronia de pools pero con co-presencia sembrada | fraccion de p < 0.05 entre 0.03 y 0.07 | no confunde co-presencia con contencion |
| Segundo nulo con poder | p con pools compartidos sembrados en bloques co-presentes | p < 0.01 | detecta contencion por pools |
| Nulo de orden calibrado | p con `tx_index` aleatorio | fraccion de p < 0.05 entre 0.03 y 0.07 | sin sesgo |
| Nulo de orden con poder | p con fallos sembrados despues del exito | p < 0.01 | detecta el orden |

## 8. Referencias

- `docs/07_fundamentos_teoricos/contencion-entre-bots.md` (sellado con AF-01).
- Wu, F. y Oz, B. (2026). *To Wait or to Probe: Arbitrage Competition on High-Throughput Blockchains.* AFT 2026.
