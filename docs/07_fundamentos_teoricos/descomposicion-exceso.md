---
estado: VIVO
categoria_documental: FUNDAMENTO
titulo: Fundamento Teorico - Descomposicion del exceso E (filtro analitico)
ultima_actualizacion: 2026-10-17
plantilla: PLANTILLA_FUNDAMENTO_TEORICO 1.0.0
---

# Fundamento Teorico: Descomposicion del exceso E

## 1. Objetivo

Explicar en papel que mide el exceso $E$ de AF-01 antes de interpretarlo: cuanto es
sincronia de bloque, cuanto es deriva dentro de la hora y cuanto es efecto del diseno
(bloques por hora). Es el filtro analitico: proposiciones con prueba, de las que salen
predicciones que se contrastan despues con los datos (§7). No cambia los documentos
sellados con AF-01.

## 2. Definiciones y Notacion

- Hora $H$ con $n = |\mathcal{B}_H|$ bloques sorteados; tiempos $t_b$.
- Entidades $u$; paquete $X_{u,b}$ (posiblemente vacio) de la entidad $u$ en el bloque $b$.
- Para un intento fallido $f$ de la entidad $v$ en el bloque $b$, y una asignacion
  $c = (c_u)_{u \neq v}$ de un bloque a cada otra entidad, la configuracion vista por $f$ es
  $Y(c) = \{X_{u,c_u}\}_{u \neq v}$ y su puntuacion $s(f; c) \in [0,1]$ es la de
  `contencion-entre-bots.md` §4.4 bis con los pools de $Y(c)$. Observado: $c_u = b$ para todo $u$.
- Contribucion aislada: $a_u(f; c_u)$ = puntuacion de $f$ si solo existiera la entidad $u$
  con su paquete en $c_u$.
- Curva de distancia: $C(\Delta) = \mathbb{E}[s(f; c)]$ con todos los $c_u = b'$ y
  $|t_b - t_{b'}| = \Delta$; $C(0) = CR_{w,obs}$. $C(0^+) = \lim_{\Delta \downarrow 0} C(\Delta)$ sobre $b' \neq b$.

## 3. Marco Conceptual

El nulo primario manda el paquete de $f$ a un bloque al azar de la hora y hace lo mismo,
independientemente, con cada otra entidad. Visto desde $f$, cada otra entidad termina
aportando el paquete de un bloque uniforme de la hora, que con probabilidad $1/n$ es el
mismo bloque original. Asi, $E$ compara "el mismo bloque" contra "una mezcla de bloques de la
hora". Esa mezcla difiere del mismo bloque por tres razones: sincronia real (salto en
$\Delta = 0$), deriva (los bloques cercanos se parecen mas que los lejanos) y la no linealidad
de la union de pools entre entidades.

## 4. Formulacion Matematica

Iteracion 2 (2026-10-17): la version 1 de estas proposiciones paso por una auditoria
adversarial (EXPL-19). Prop. 1 y 3 sobrevivieron con correccion, la Prop. 2 era falsa tal
como estaba (condicion insuficiente) y el alcance de la Prop. 4 era mas estrecho. Lo que
sigue es la version corregida; la anterior queda en el historial de git.

El nulo implementado (`src/contend.rs`, `null_hour`) es el descrito: cada **entidad** permuta
sus paquetes, incluidas las celdas vacias, entre los $n$ bloques sorteados de la hora; el
paquete de $f$ se mueve con $f$, pero $c_u = \pi_u^{-1}(\pi_v(b))$ sigue uniforme e independiente
entre las $u \neq v$. Los pesos $w_p$ y $P(f)$ no cambian. ($\S4.5$ de `contencion-entre-bots.md`
dice "bot" donde el codigo usa entidad.)

### 4.1 Proposicion 1 (esperanza exacta del nulo, con union)

Sea $\hat{w}_p = w_p / \sum_{q \in P(f)} w_q$ y $m_{u,p} = |\{c \in \mathcal{B}_H : p \in X_{u,c}\}|$. En el
nulo primario

$$\mathbb{E}_{nulo}[s(f)] = \sum_{p \in P(f)} \hat{w}_p \Big[1 - \prod_{u \neq v}\Big(1 - \frac{m_{u,p}}{n}\Big)\Big],
\qquad e(f) = s(f;b) - \mathbb{E}_{nulo}[s(f)].$$

*Prueba.* $s(f;c) = \sum_p \hat{w}_p\, \mathbb{1}[\exists u: p \in X_{u,c_u}]$. Los $c_u$ son uniformes e
independientes, asi que $P(\text{ningun } u \text{ toca } p) = \prod_u (1 - m_{u,p}/n)$; se suma por
linealidad. $\square$

Caso particular: si cada pool de $f$ lo toca a lo sumo una entidad ajena en la hora, la
formula se reduce a la aditiva y $e(f) = (1 - 1/n)[s(f;b) - \bar{s}_{-b}(f)]$. Con $k$ entidades
redundantes en un pool que solo tocan en $b$, el factor es $(1-1/n)^k$, no $(1-1/n)$
(contraejemplo: $n = 2$, $k = 2$ da $e = 0.25$). Por eso **$E$ no escala en general con
$1 - \mathbb{E}[1/n]$**; si escala asi, es porque la redundancia por pool es baja. Una hora con
$n = 1$ aporta $E = 0$ exactamente.

### 4.2 Proposicion 2 (cota con asociacion por pool)

Sea $c$ uniforme en $\mathcal{B}_H$ y $N_{u,p} = \{c : p \notin X_{u,c}\}$. Si para todo $p \in P(f)$

$$P_c\Big(\bigcap_u N_{u,p}\Big) \ge \prod_u P_c(N_{u,p})$$

(por ejemplo, si los indicadores $\mathbb{1}[p \in X_{u,c}]$ estan asociados en el sentido de
Esary-Proschan), entonces $e(f) \le (1 - 1/n)[s(f;b) - C_{-b}(f)]$, con
$C_{-b}(f) = \frac{1}{n-1}\sum_{c \neq b} s(f; c_u = c\ \forall u)$.

*Prueba.* $\frac{1}{n}s(f;b) + (1 - \frac{1}{n})C_{-b}(f) = \sum_p \hat{w}_p[1 - P_c(\cap_u N_{u,p})] \le
\sum_p \hat{w}_p[1 - \prod_u P_c(N_{u,p})] = \mathbb{E}_{nulo}[s(f)]$ por la Prop. 1. $\square$

La correlacion positiva por pares no basta con tres o mas entidades (contraejemplo: $n = 12$,
tres entidades cuyos bloques sin $p$ son $\{1,2,3,4\}$, $\{1,2,5,6\}$, $\{3,4,5,6\}$: lado derecho
0, $e = 1/27$), y con correlacion negativa el signo se invierte. La cota vale intento a
intento; el agregado es la media por intento de $(1 - 1/n_f)(s_f - C_{-b,f})$, y no esta
definido para $n = 1$.

### 4.3 Proposicion 3 (sincronia frente a deriva)

Modelo: $C(\Delta) = g(\Delta) + J\,\mathbb{1}[\Delta = 0]$, con $g$ continua y $J \ge 0$ el salto en el
mismo bloque. En el caso aditivo, intento a intento,
$e_f = (1 - 1/n_f)[J_f + g_f(0) - \bar{g}_f]$, con $\bar{g}_f$ el promedio de $g$ sobre las
distancias a los otros bloques sorteados de la hora; con union y la condicion de la Prop. 2,
$e_f \le (1 - 1/n_f)[J_f + g_f(0) - \bar{g}_f]$. El agregado no se factoriza si $n_f$ se correlaciona
con $J_f$ o $g_f$.

Consecuencias (solo dependen de la curva $C$, no de la aditividad):

1. $E$ mezcla una atenuacion de $J$ (factor $\le 1 - 1/n$) con un termino de deriva
   $g(0) - \bar{g} \ge 0$ cuando $g$ decrece. El signo neto depende de los datos.
2. $C(0) - C(\Delta_1) = J + [g(0^+) - g(\Delta_1)]$ es **cota superior** de $J$ si
   $g(\Delta_1) \le g(0^+)$; no requiere $g$ monotona.
3. Bajo $g$ continua con $g(\Delta) \le g(0^+)$, el conjunto identificado es
   $J \in [0,\ C(0) - C(\Delta_1)]$ para cualquier $\Delta_1$ observado. Con el tramo (0, 2) min
   (unos 200 pares por semana), $J \le C(0) - C(0\text{-}2\,\text{min})$ = 0.043, 0.089, 0.039 y 0.087
   (S1 a S4; `artefactos/sens_auditoria/deriva.txt`, sin IC aun): cota mas ajustada que
   $E_{vec}$. Ninguna cota inferior positiva es posible sin restringir la forma de $g$; la
   curva observada es convexa, y bajo convexidad la cota inferior es 0. **$J = 0$ no se excluye
   con los datos actuales.**
4. $C(0) - C(2\,\text{s}) \ge J$ (vecino $b+1$), con igualdad solo si se supone
   $g(0^+) = g(2\,\text{s})$ y que la oportunidad no se arrastra ni se consume entre $b$ y $b+1$. Es
   la cota superior mas ajustada posible con bloques de 2 s, no una identificacion.
5. Para AF-02 el estimando pasa de $E$ a la curva corta $C(0) - C(\Delta)$ con
   $\Delta \in \{2\,\text{s}, 30\,\text{s}, 5\,\text{min}\}$, reportada como cota superior de la sincronia, junto
   con las pruebas de orden dentro del bloque.

### 4.4 Proposicion 4 (pruebas de orden y deriva entre bloques)

Un estadistico que solo depende del orden dentro de cada bloque, contrastado con un nulo que
permuta `tx_index` dentro del bloque, es valido condicionado al contenido de cada bloque, sea
cual sea $g$. *Prueba:* el estadistico y su distribucion nula son funciones del contenido de
cada bloque por separado; bajo orden intercambiable el valor p es exacto. $\square$

No protege contra la **deriva dentro del bloque**: el estado evoluciona por Flashblocks
(200 ms), y si los reverts caen tarde por diseno (fee, momento de llegada) tienen mas swaps
previos sin competencia. El control con probes (exceso ~0) lo atenua, no lo descarta.

Consecuencia: el exceso de orden en reverts es la unica evidencia actual que no puede ser
deriva **entre bloques**. Lo falsaria un exceso igual en probes y reverts, o que desaparezca
al estratificar por Flashblock o por posicion en el bloque.

## 5. Limites, Riesgos y Casos de Falla

- Las Proposiciones 1 y 3 son exactas solo para puntuacion aditiva; con union valen como
  aproximacion y la Proposicion 2 da el signo bajo una condicion no verificada en general.
- $n$ varia por hora y $J$, $g$ pueden variar por entidad y por hora: las formulas valen
  intento a intento y se promedian; un promedio de cocientes no es el cociente de promedios.
- La Proposicion 4 no excluye otros confusores de orden (prioridad de fee, llegada al Flashblock).

## 6. Implicaciones para Arquitectura y ADR

- El estimando de AF-02 deja de ser $E$ y pasa a ser la cota superior de $J$ (vecino $b+1$) con $C(\Delta)$ en
  2 s, 30 s y 5 min, y las pruebas de orden dentro del bloque como evidencia de competencia.
- Script de contraste: `scripts/analisis/descomposicion.py` (§7).

## 7. Criterios de Validacion Empirica (filtro empirico)

| Prediccion | Prueba con datos existentes | Resultado que la falsaria |
|---|---|---|
| P1: $E$ igual a la media por intento de la formula exacta de la Prop. 1 | recalcular $\mathbb{E}_{nulo}$ por pool con $m_{u,p}$ y comparar con la media del nulo de `contend` | diferencia fuera del error de Monte Carlo (seria un error de implementacion) |
| P1b: si la redundancia por pool es baja, $E_{expl}/E_{conf} \approx (1-\mathbb{E}[1/n])_{expl}/(1-\mathbb{E}[1/n])_{conf}$ | cocientes con $\mathbb{E}[1/n]$ ponderado por intento, y redundancia media por pool | cociente lejos del predicho con redundancia baja |
| P2: $e_f \le (1-1/n_f)(s_f - C_{-b,f})$ donde se cumple la asociacion | intento a intento en el confirmatorio | violaciones frecuentes donde la condicion se cumple |
| P3: $C(0) - C(\Delta)$ crece con $\Delta$ | curva de EXPL-18 | curva no monotona |
| P4: el exceso de orden es de reverts, no de probes, y sobrevive por posicion | `orden_swap.py` estratificado por Flashblock aproximado (`tx_index`) | exceso igual en probes o que desaparece al estratificar |

## 8. Referencias

- `docs/07_fundamentos_teoricos/contencion-entre-bots.md` (sellado con AF-01), §4.4 bis y §4.5.
- `docs/07_fundamentos_teoricos/contencion-nulos-secundarios.md`, §4.2 y §4.3.
- EXPL-13, EXPL-16, EXPL-18 en `docs/00_registro_exploracion.md`.
