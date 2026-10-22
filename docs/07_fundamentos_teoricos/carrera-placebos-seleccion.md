---
estado: VIVO
categoria_documental: FUNDAMENTO
titulo: Fundamento Teorico - Robustez de la prueba de carrera (placebos, seleccion de P(t), descomposicion de Simpson, heterogeneidad)
ultima_actualizacion: 2026-10-18
plantilla: PLANTILLA_FUNDAMENTO_TEORICO 1.0.0
---

# Fundamento Teorico: Robustez de la prueba de carrera

## 1. Objetivo

La prueba de carrera de AF-02 (apartado 4.6 de `contencion-nulos-secundarios.md`) midio una diferencia de
riesgos Mantel-Haenszel $\Delta_{MH} = +0.0775$ entre transacciones con y sin un swap exitoso
previo de otra entidad en sus pools. Las revisiones del borrador del preprint dejaron preguntas
que ese resultado no contesta: si el efecto distingue la lectura de carrera por estado de otras
tres explicaciones, si el conjunto de pools $P(t)$ (que sale de la traza de la propia
transaccion) sesga la comparacion, por que el signo se invierte cuando no se condiciona en
entidad (-0.0298), y si el efecto es el mismo en todas las semanas y entidades.

Este documento fija, en papel y antes de cualquier codigo o calculo, (a) que predice cada
explicacion para cada prueba, (b) los estimandos de los placebos, (c) como se acota el sesgo de
seleccion, (d) una descomposicion exacta de la inversion de Simpson y (e) los estadisticos de
heterogeneidad. No cambia los documentos sellados con AF-01, AF-02 ni AF-03; los extiende. El
analisis que usa esta teoria es un contrato nuevo (AF-04, `docs/adrs/006-diseno-af04.md`).

## 2. Definiciones y Notacion

Las de AF-02: universo $\mathcal{U}$ = transacciones $t$ de contratos bot, que no son probes, con
$P(t) \neq \emptyset$; entidad $e(t)$; bloque $b(t)$; indice $i(t)$ = `tx_index`; decil $d(t)$ de
$i(t)/(\text{largo del bloque})$; $Y(t) = 1$ si $t$ es revert. Celda $c = (e, d)$.

- $W_b(p)$: swaps exitosos del bloque $b$ en el pool $p$, cada uno con su indice $j$ y su entidad
  $e_j$ (de cualquier remitente: bots y usuarios). Salen de los receipts.
- $B_b(p) \subseteq W_b(p)$: los de transacciones del universo bot (los unicos que ve $X$).
- $X(t) = 1$ si existe $p \in P(t)$ y $j \in B_b(p)$ con $j < i(t)$ y $e_j \neq e(t)$ (la $X$ de AF-02).
- $\Delta_{MH}(Z)$: la diferencia de riesgos MH de $Y$ entre $Z = 1$ y $Z = 0$ dentro de las celdas
  $(e, d)$ con ambos grupos, pesos $w_c = n_{1c} n_{0c} / (n_{1c} + n_{0c})$. Con $Z = X$ es la de AF-02.
  Abreviatura: $\Delta = \Delta_{MH}(X)$.
- Regla de tres salidas $\mathcal{R}_3$ (la de AF-02): para una cantidad con estimacion $\hat\theta$ e IC 95% por
  bootstrap de semanas completas $[\ell, u]$ y umbral $\tau = 0.02$: **activa** si $\hat\theta \ge \tau$ y
  $\ell > 0$; **nula** si $u < \tau$; **inconclusa** en otro caso.
- Explicaciones (rivales, $[PRE-04]$ en el dossier):
  - **R-S, carrera por estado:** un swap previo en $p$ cambia el estado de $p$ y $t$ falla por eso. Solo actua
    lo que ocurre antes en el orden y en un pool de $P(t)$.
  - **R-C, choque comun del pool:** un choque no observado $Z_{p,b}$ (movimiento de precio, oportunidad) sube a la vez
    los swaps ajenos en $p$ (antes o despues) y los reverts de $t$ en $p$. El orden no importa.
  - **R-B, choque comun del bloque:** un choque $Z_b$ sube la actividad de todos los pools del bloque y los reverts.
  - **R-O, posicion residual:** el orden es no aleatorio: lo tardio revierte mas y tiene mas swaps previos. El decil no
    absorbe la posicion fina.
  - **R-U, estado consumido por cualquier swap:** subtipo de R-S en que tambien cuentan los swaps de usuarios; lo
    contrario es que la carrera sea especifica entre bots.

## 3. Marco Conceptual

$X \to Y$ en AF-02 se compara con ceros que no son un nulo: es una asociacion condicionada. Un placebo
es una $\tilde{X}$ construida para que, bajo R-S, no pueda afectar a $Y$, y bajo un rival si. Lo que
discrimina es el patron de signos de varios placebos, no uno solo (tabla de §4.1).

Un placebo temporal mira lo que viene despues: el estado no viaja hacia atras, de modo que R-S no puede explicar
una asociacion con swaps posteriores; un choque comun si. Un placebo de pool conserva el bloque, la entidad y la
posicion, pero cambia el pool: el estado de un pool ajeno al de $t$ no puede consumirse por R-S; un choque de bloque
si. Un placebo de entidad cambia quien hizo el swap previo: separa competencia entre entidades de repeticion propia.

Los placebos construidos con orden ($\tilde{X}$ "despues") correlacionan con la posicion al reves que $X$. Por eso un
placebo temporal negativo no distingue R-S de R-O; solo un placebo temporal activo (positivo) falsa R-S.

## 4. Formulacion Matematica

### 4.1 Estimandos de los placebos y predicciones

Todos usan el universo $\mathcal{U}$, las celdas $(e, d)$, los pesos MH y el bootstrap de semanas de AF-02.

- **Temporal (T).** $X_T(t) = 1$ si existe $p \in P(t)$ y $j \in B_b(p)$ con $j > i(t)$ y $e_j \neq e(t)$. Estimando
  $\Delta_T = \Delta_{MH}(X_T)$. Version condicionada $\Delta_{T|X}$: celdas $(e, d, X)$.
- **De pool (P).** Para cada $t$ se elige un donante $t'$ del mismo bloque, del universo, de otra entidad, con
  $P(t') \cap P(t) = \emptyset$ y el $|P(t')|$ mas cercano a $|P(t)|$ (empates y eleccion final por
  $\text{sha256}(\text{semilla}, b, \text{hash}(t))$, deterministico). $X_P(t) = 1$ si algun $p' \in P(t')$ tiene un swap
  previo de otra entidad distinta de $e(t)$ (mismo criterio que $X$, con los pools del donante). Sin donante, $t$ queda
  fuera de esta prueba; $\Delta$ se recalcula sobre el mismo subconjunto ($\Delta^{don}$) y el contraste
  $\Delta^{don} - \Delta_P$ se toma sobre ese subconjunto. Conserva bloque, entidad, decil, posicion exacta y tamano de
  $P$; rompe el vinculo con los pools propios.
- **De entidad (E).** $X_E(t) = 1$ si existe $p \in P(t)$ y $j \in B_b(p)$ con $j < i(t)$ y $e_j = e(t)$ (la misma
  entidad, otra transaccion, no $t$). $\Delta_E = \Delta_{MH}(X_E)$; version condicionada $\Delta_{E|X}$ y $\Delta_{X|E}$.
- **Swaps de usuarios (U).** $X_{U}(t) = 1$ si existe $p \in P(t)$ y $j \in W_b(p) \setminus B_b(p)$ con $j < i(t)$ y
  remitente fuera de la entidad $e(t)$ (swaps exitosos que no son de contratos bot). Estimandos $\Delta_U =
  \Delta_{MH}(X_U)$, $\Delta_{X|U}$ (celdas $(e, d, X_U)$) y $\Delta_{U|X}$. $X_{all} = \max(X, X_U)$.

Patron de predicciones (signo de la cantidad; $\approx 0$ quiere decir "nula" por $\mathcal{R}_3$; "+" quiere decir "activa"):

| Cantidad | R-S | R-C | R-B | R-O |
|---|---|---|---|---|
| $\Delta$ (primaria) | + | + | + | + |
| $\Delta_T$ | $\le 0$ (nula o negativa: quien va antes de un ganador del pool suele ser el ganador) | + | + | $\le 0$ (espejo de la posicion) |
| $\Delta_P$ | $\approx 0$ | $\approx 0$ | + | + |
| $\Delta^{don} - \Delta_P$ | $= \Delta^{don}$ (+) | $= \Delta^{don}$ (+) | $\approx 0$ | $\approx 0$ |
| $\Delta_{E}$ | + si el estado lo consume cualquier ganador; $\approx 0$ si solo cuenta un rival | + | + | + |

Cuatro consecuencias:

1. $\Delta_T$ **activa** falsa R-S (R-S no puede producirla) a favor de R-C o R-B. $\Delta_T$ nula o negativa no la prueba:
   es compatible con R-S y con R-O.
2. $\Delta_P$ **activa** falsa R-S a favor de R-B o R-O. $\Delta_P$ nula la deja viva frente a R-B y R-O.
3. R-C se separa de R-B con $\Delta_T$ activa y $\Delta_P$ nula (R-C) frente a ambas activas (R-B).
4. R-S queda **reforzada** (no probada: causalidad no identificada) si $\Delta$ es activa, T es nula y P es nula **y
   discrimina**: el limite inferior de $\Delta^{don} - \Delta_P$ es $\ge 0.02$. Sin esa condicion, un $\Delta_P$ nulo
   no distingue nada: si $\Delta^{don}$ tampoco es activa en el subconjunto con donante, R-S, R-C, R-B y R-O predicen
   todos un efecto cercano a 0, y el resultado es inconcluso. Eso no excluye una interaccion de orden (R-O) con el
   decil; se controla con 50 tramos y celdas $(e, b)$, que son sensibilidades y no deciden.
   (Actualizado 2026-10-20, antes del sello de AF-04: se anade la condicion de discriminacion; ver la tabla total de
   la regla conjunta en el preregistro de AF-04 y la errata del ADR 006, §7.)

   Un placebo nulo por $\mathcal{R}_3$ ($u < 0.02$) incluye el caso de estimado negativo con IC que excluye 0. Para $\Delta_T$
   es compatible con R-S y con R-O (se declara). Para $\Delta_P$ ninguna de las cuatro explicaciones predice un signo
   negativo: se reporta como anomalia (la regla de donante puede correlacionar con la posicion) y no cambia la clase.
   Solo el estimando primario de cada prueba decide la lectura conjunta ($\Delta$, $\Delta_T$ con 10 tramos y celdas
   $(e, d)$, $\Delta_P$ con la semilla de donante 1 y su contraste); las sensibilidades se reportan y no deciden.

$\Delta_E$ no es un placebo en el sentido de efecto nulo bajo R-S. Es el contraste que decide una frase: si $\Delta_E$ es
activa, un swap previo de la propia entidad tambien sube el revert y la lectura "competencia entre bots" pierde
soporte frente a "repeticion propia" (la redundancia de calldata ya medida en AF-01); si es nula, el efecto es especifico
de otra entidad. Ambas son informacion; ninguna cambia el veredicto de $\Delta$.

### 4.2 Seleccion por $P(t)$ dependiente del resultado

$P(t)$ sale de la traza de la propia transaccion. Un revert temprano puede no tocar ningun pool: $P(t) = \emptyset$ y la
transaccion sale del universo; ademas $|P(t)|$ es menor en reverts que en exitos (DIAGNOSTICO sobre AF-01: 13.1% de los
reverts con $P$ vacio contra 0.7% de los exitos; $|P|$ medio 1.64 contra 3.09). Dos mecanismos que empujan en
direcciones posibles distintas: la exclusion de reverts (datos faltantes que dependen de $Y$) y el sesgo de
$P(X=1)$ por $|P|$ (menos pools, menos oportunidad de $X=1$, que deprime $r_1$).

**(a) Cotas por esquinas de datos faltantes.** Sea $\mathcal{M}$ el conjunto de transacciones de contratos bot con $P = \emptyset$
(la misma regla de bot y entidad). Su $X$ no esta definido. Se asigna $X = 1$ a una fraccion $q_R$ de los reverts de
$\mathcal{M}$ y a una fraccion $q_S$ de los exitos de $\mathcal{M}$, dentro de cada celda $(e, d)$ (conteos esperados,
fraccionarios), y se recalcula $\Delta(q_R, q_S)$. La cota practica es el minimo y el maximo sobre una rejilla
$q_R, q_S \in \{0, 0.1, \dots, 1\}$ que incluye las esquinas. Aviso: el peso MH $w_c = n_1 n_0/(n_1 + n_0)$ no es
lineal en los conteos, asi que $\Delta(q_R, q_S)$ no es monotona en general y las esquinas no son cotas exactas
en el sentido de Manski (1990); la rejilla es la cota practica y se declara asi. **Punto de inflexion (tipping):**
el menor $q_R$ (con $q_S$ a su esquina menos favorable) para el cual $\Delta(q_R, q_S) < \tau$.
Si no existe $q_R \in [0, 1]$ que baje $\Delta$ por debajo de $\tau$, el hallazgo es robusto a cualquier
asignacion de $\mathcal{M}$.

**(b) Regla de pools independiente del resultado.** Para un contrato $c$ y una semana, $P_c$ = los pools que reunen al
menos 10% de los swaps exitosos de $c$ esa semana (a lo mas 5, desempate por direccion). Se aplica igual a todas las
transacciones de $c$, exito, revert o probe, y define $X_c(t)$ con $P_c$ en lugar de $P(t)$. Contratos sin swaps exitosos
en la semana quedan fuera en ambos resultados. $X_c$ es un indicador con error de medida que no depende de $Y$: tiende a
atenuar la asociacion hacia 0 (sesgo hacia el nulo si el error no es diferencial), de modo que un $\Delta_c$ activo es
conservador. Es la regla de Anchuri et al. (pool del revert inferido desde los swaps exitosos de su contrato), acotada
a una semana. El 10% y el tope de 5 son un `SUPUESTO` fijado sin datos.

**(c) Control por tamano de $P$.** Celdas $(e, d, g)$ con $g = \min(|P(t)|, 3)$: dentro de $g$, $X$ no cambia su probabilidad
solo por tener mas pools.

El calldata no sirve como fuente de $P(t)$: en un bloque revisado, 34 pools con swap y ninguno aparecia en el calldata
de los intentos fallidos (EXPL-08).

### 4.3 Descomposicion exacta de la inversion de Simpson

Sea $\Delta_{dec}$ la diferencia de riesgos MH con celdas solo de decil, pesos $w_d = n_{1d} n_{0d}/(n_{1d} + n_{0d})$
(la columna "solo decil" de AF-02, $-0.0298$), y $\Delta_{MH}$ la de celdas $(e, d)$. Sea $S$ el conjunto de celdas
$(e, d)$ con ambos grupos y $S_d = \{e : (e, d) \in S\}$.

Para $(e, d) \in S$: $n_{xed}$, $r_{xed}$ (tasa de revert), $\Delta_{ed} = r_{1ed} - r_{0ed}$. Dentro de $d$, restringido a $S$:
$n^S_{xd} = \sum_{e \in S_d} n_{xed}$, $\pi_{x,e|d} = n_{xed}/n^S_{xd}$, $r^S_{xd} = \sum_e \pi_{x,e|d}\, r_{xed}$. Sea $D_S$ el
estimador de solo decil calculado sobre las transacciones de $S$ (con sus pesos $w^S_d$, normalizados $\omega_d$) y
$\mathrm{Sup} := \Delta_{dec} - D_S$ (efecto de incluir celdas de un solo grupo).

**Proposicion 1 (identidad de Kitagawa con punto medio).** Para cada $d$:
$$ r^S_{1d} - r^S_{0d} = \sum_{e \in S_d} \bar\pi_{e|d}\, \Delta_{ed} \;+\; \sum_{e \in S_d} (\pi_{1,e|d} - \pi_{0,e|d})\, \bar r_{ed}, $$
con $\bar\pi_{e|d} = (\pi_{1,e|d} + \pi_{0,e|d})/2$ y $\bar r_{ed} = (r_{1ed} + r_{0ed})/2$.

*Prueba.* El lado derecho es $\tfrac12 \sum_e [(\pi_1 + \pi_0)(r_1 - r_0) + (\pi_1 - \pi_0)(r_1 + r_0)] = \tfrac12 \sum_e [2\pi_1 r_1 - 2\pi_0 r_0]
= \sum_e \pi_1 r_1 - \sum_e \pi_0 r_0 = r^S_{1d} - r^S_{0d}$. $\square$

Agregando con $\omega_d$ ($\sum_d \omega_d = 1$) se obtiene $D_S = W_S + C_S$, donde
$$ W_S = \sum_d \omega_d \sum_{e} \bar\pi_{e|d}\,\Delta_{ed}, \qquad C_S = \sum_d \omega_d \sum_{e} (\pi_{1,e|d} - \pi_{0,e|d})\, \bar r_{ed}. $$

**Corolario (cuatro terminos, identidad exacta).** Con $\mathrm{Wgt} := W_S - \Delta_{MH}$:
$$ \Delta_{dec} - \Delta_{MH} = \underbrace{C_S}_{\text{composicion entre entidades}} + \underbrace{\mathrm{Wgt}}_{\text{ponderacion}} + \underbrace{\mathrm{Sup}}_{\text{soporte}}. $$
Es una identidad algebraica, no una estimacion: vale en los datos observados y en cada remuestreo de semanas.

Lectura. $C_S$ es la parte debida a que las entidades sobrerrepresentadas en $X = 1$ tienen tasas de revert distintas
(como $\sum_e(\pi_1 - \pi_0) = 0$, $C_S$ es una covarianza entre la sobrerrepresentacion en $X = 1$ y el nivel de revert
de la entidad, ponderada por decil). $\mathrm{Wgt}$ es la diferencia entre promediar los efectos $\Delta_{ed}$ con los
pesos de composicion del decil y con los pesos MH de celda. $\mathrm{Sup}$ es el efecto de las celdas con un solo grupo,
que $\Delta_{MH}$ descarta y $\Delta_{dec}$ conserva. Es contabilidad descriptiva: no dice por que las entidades
difieren.

Brecha de AF-02: $G = \Delta_{dec} - \Delta_{MH} = -0.0298 - 0.0775 = -0.1073$ (`DATO`, ya visto).

**Concentracion y entidades.** La entidad se define por semana; para seguirla entre semanas se usa la llave del
contrato bot con mas transacciones en la entidad-semana (`SUPUESTO`: el contrato bot es estable entre semanas;
una entidad que rota de contrato se parte). Se reportan: la participacion de cada llave en el peso MH
$\sum_{c \in \text{llave}} w_c / \sum_c w_c$, las cinco mayores y el indice de Herfindahl; $\Delta^{(-k)}$ (leave-one-out) quitando cada
una de las cinco llaves mayores; y la distribucion del peso MH segun el signo de $\Delta_{ew}$ de cada entidad-semana
(estimado con celdas de decil dentro de la entidad-semana, solo entidad-semanas con al menos 30 transacciones por grupo).

### 4.4 Heterogeneidad entre semanas

Con $\hat\Delta_w$ y $\hat v_w$ (varianza por bootstrap de horas dentro de la semana $w$, $K$ semanas), el estadistico
$Q = \sum_w (\hat\Delta_w - \hat\Delta_{FE})^2/\hat v_w$, $\hat\Delta_{FE} = \sum_w \hat\Delta_w/\hat v_w / \sum_w 1/\hat v_w$, y
(DerSimonian y Laird, 1986)
$$ \hat\tau^2 = \max\!\Big(0, \frac{Q - (K - 1)}{S_1 - S_2/S_1}\Big), \quad S_1 = \sum_w \hat v_w^{-1}, \; S_2 = \sum_w \hat v_w^{-2}, \qquad I^2 = \max\!\Big(0, \frac{Q - (K - 1)}{Q}\Big). $$
Se reportan $\hat\tau^2$, $I^2$, el estimador de efectos aleatorios y el intervalo de prediccion
$\hat\Delta_{RE} \pm 1.96\sqrt{\hat\tau^2 + \widehat{\mathrm{Var}}(\hat\Delta_{RE})}$. La tendencia con el indice de semana
(pendiente por minimos cuadrados ponderados y Spearman) es **retrospectiva en AF-02**: la serie ya se vio ($\rho = -0.64$,
0.115 en las primeras 24 semanas contra 0.062 en las ultimas 24, DIAGNOSTICO). Su unica lectura prospectiva es la que
se declara para semanas nuevas (ADR 006, T9). El modelo de efectos aleatorios supone semanas intercambiables; con tendencia
no lo son, y por eso se reporta $I^2$ junto a la pendiente.

### 4.5 Sensibilidad a la definicion de bot y de entidad

El universo es una funcion del resultado a nivel de contrato: un contrato es bot si al menos una fraccion $s$ de sus
transacciones de la semana son intentos fallidos ($s = 0.5$ en AF-02). Subir o bajar $s$ cambia que contratos entran,
no el estimando dentro de cada celda; un efecto homogeneo en el espacio de contratos es estable en $s$. Se corre
$s \in \{0.25, 0.75\}$. La entidad se define por remitentes compartidos; remitentes que llaman a muchos contratos
(relayers, bundlers) unen entidades distintas, y `--max-sender-degree 10` los ignora (AF-02 lo dejo pendiente).

## 5. Limites, Riesgos y Casos de Falla

- **Orden no causal.** La llegada y la comision fijan el orden ademas del estado. Ningun placebo identifica causalidad;
  discriminan explicaciones.
- **Placebo temporal ambiguo si negativo.** Ver §3: solo falsa R-S cuando es activo.
- **Donante sin vecinos.** En bloques con pocas transacciones de bots, $P$ no tiene donante; la cobertura se reporta y
  $\Delta^{don}$ cubre el mismo subconjunto.
- **$P_c$ con error de medida.** Atenua; un $\Delta_c$ nulo no es evidencia de ausencia (se compara con $\Delta$ sobre el mismo
  subconjunto de transacciones).
- **Cotas por esquinas.** No exactas por la no linealidad del peso MH; se declara como cota practica.
- **Dependencia dentro de la semana.** El bootstrap de semanas completas la deja dentro de la unidad, pero con 48 o menos
  unidades el IC es aproximado.
- **Multiples contrastes.** Hay decisiones separadas por umbral e IC, sin correccion, sin elegir la favorable; la regla
  conjunta de lectura (ADR 006 §1.2) se declara antes y no usa p.
- **Descomposicion.** Es contabilidad; dos descomposiciones distintas (otro punto de referencia que el punto medio) reparten
  distinto. El punto medio es simetrico y exacto; no es unico.
- **Llave de entidad entre semanas.** Aproximada; la concentracion se subestima si una entidad rota de contrato.

## 6. Implicaciones para Arquitectura y ADR

- ADR relacionado: `docs/adrs/006-diseno-af04.md`.
- Modulos impactados: `race` (definiciones alternativas de $X$ y de $P$, opciones opt-in, salida por defecto sin cambios),
  un modulo de descomposicion y heterogeneidad (nuevo), `scripts/analisis/` (verificacion cruzada).
- La implementacion escribe el donante del placebo de pool de forma deterministica (hash), no con un generador secuencial,
  para que no dependa del orden de procesamiento ni de la paralelizacion.

## 7. Criterios de Validacion Empirica

| Criterio | Metrica | Umbral | Resultado esperado |
|----------|---------|--------|--------------------|
| Placebos calibrados | fraccion de $p < 0.05$ de cada $\Delta$ con $\tilde{X}$ aleatorio dentro de celda (200 ensayos) | entre 0.02 y 0.09 | sin falsos positivos |
| Poder, efecto de estado sembrado | $\Delta$ con efecto +0.05 solo en $X$ (antes): $\Delta_T$, $\Delta_P$ | $\Delta$ detectada en al menos 18 de 20; $\Delta_T$ y $\Delta_P$ no activas | R-S sembrada se ve como R-S |
| Poder, choque de pool sembrado | efecto +0.05 sobre swaps antes y despues en el mismo pool | $\Delta_T$ activa en al menos 18 de 20 | R-C sembrada falsa R-S |
| Poder, choque de bloque sembrado | efecto sobre actividad del bloque | $\Delta_P$ activa en al menos 18 de 20 | R-B sembrada se ve |
| Placebo de entidad con poder | efecto sembrado solo con swap previo propio | $\Delta_E$ activa en al menos 18 de 20 | detecta |
| Identidad de Simpson | $\lvert\Delta_{dec} - \Delta_{MH} - (C_S + \mathrm{Wgt} + \mathrm{Sup})\rvert$ | menor que $10^{-12}$ en datos aleatorios y en cada remuestreo | identidad exacta |
| Simpson sintetico | dos entidades con tasas base distintas y reparto distinto de $X$ | signo de $\Delta_{dec}$ opuesto al de $\Delta_{MH}$ y $C_S$ con el signo de $G$ | reproduce la inversion |
| Cotas | $\Delta$ verdadero con faltantes simulados, en la rejilla | dentro de $[\min, \max]$ de la rejilla | contiene |
| Defecto sin cambio | salida de `race` sin opciones nuevas | identica byte a byte a la de v0.7.0 sobre el confirmatorio de AF-01 y sobre una semana de AF-02 | sin regresion |
| Paridad Rust y Python | estimaciones puntuales y conteos de cada variante | igualdad exacta | sin discrepancia |

## 8. Referencias

- apartado 4.6 de `docs/07_fundamentos_teoricos/contencion-nulos-secundarios.md` (prueba de carrera), `descomposicion-exceso.md`,
  `uniswap-v4-pools.md`.
- Anchuri, Felten y Mamageishvili (2026), arXiv:2610.03922, "Priority Gas Auction Cadence and Searcher Competition: Evidence from Flashblocks on Base" (titulo, autores y la cota superior de 1.9% a 2.7% verificados contra el resumen el 2026-10-18; que el pool del revert se infiera de los swaps exitosos del contrato NO esta en el resumen y queda por verificar en el PDF): pool del revert desde los swaps exitosos de su contrato; revert
  tras un ganador de menor comision, cota superior.
- Greenland, S. y Robins, J. M. (1985). *Estimation of a common effect parameter from sparse follow-up data.* Biometrics 41:55-68
  (pesos de la diferencia de riesgos; verificado 2026-10-18 en fuentes secundarias: autores, revista, 1985 y DOI 10.2307/2530643; volumen y paginas 41:55-68 NO verificados contra la fuente; resultado usado: el estimador MH de la diferencia de riesgos es consistente con datos dispersos).
- Lipsitch, M., Tchetgen Tchetgen, E. y Cohen, T. (2010). *Negative controls: a tool for detecting confounding and bias in
  observational studies.* Epidemiology 21(3):383-388, doi:10.1097/EDE.0b013e3181d61eeb (verificado 2026-10-18 contra PMC3053408).
- Manski, C. F. (1990). *Nonparametric bounds on treatment effects.* American Economic Review, Papers and Proceedings 80(2):319-323 (verificado 2026-10-18 en fuentes secundarias; corregida la revista: es Papers and Proceedings; el texto original no se leyo).
- Kitagawa, E. M. (1955). *Components of a difference between two rates.* JASA 50(272):1168-1194, diciembre (verificado 2026-10-18 contra el escaneo de la revista; la forma simetrica con promedio de las estructuras es la del punto medio de la Proposicion 1; un indice secundario da otras paginas, se conserva 1168-1194).
- DerSimonian, R. y Laird, N. (1986). *Meta-analysis in clinical trials.* Controlled Clinical Trials 7(3):177-188, doi:10.1016/0197-2456(86)90046-2 (verificado 2026-10-18; estimador de momentos no iterativo de la varianza entre estudios).
