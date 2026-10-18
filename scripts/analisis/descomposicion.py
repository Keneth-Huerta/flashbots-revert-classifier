"""Contraste de las predicciones P1, P1b, P2 y P4 de `descomposicion-exceso.md` (7).

P1: esperanza EXACTA del nulo primario (Prop. 1), intento a intento:
    E_nulo[s(f)] = sum_p w^_p [1 - prod_{u != v} (1 - m_{u,p}/n)], con m_{u,p} = bloques
    sorteados de la hora donde la entidad u toca p (su paquete) y n = bloques sorteados de la
    hora. Media sobre intentos fallidos con pools, frente a la columna `null` de `contend`
    (metrica ponderada; exploratorio: seccion "== metric weighted" de
    `contend_v3_metricas.txt`; confirmatorio: `contend_primario.txt`). Error de Monte Carlo
    de la media del nulo: (banda 95% / 3.92) / sqrt(permutaciones), combinado con el
    redondeo a 3 decimales de la tabla (uniforme +-0.0005). Debe coincidir; si no,
    es un error de implementacion.
P1b: cociente E_expl/E_conf frente a (1-E[1/n])_expl/(1-E[1/n])_conf (E[1/n] ponderado por
    intento), IC predicho por bootstrap de horas, IC observado por delta con los IC de E;
    mas la redundancia por pool: media de k_p = entidades ajenas que tocan p en la hora,
    sobre los pools de cada intento (y fraccion de pares (f, p) con k_p >= 2 entre los que
    tienen k_p >= 1).
P2: e_f = s_f - E_nulo(f) (exacto) frente a (1-1/n_f)(s_f - C_-b,f), con C_-b,f = media de
    s(f;b') sobre los otros bloques sorteados de la hora (bloque entero). Excluye n = 1.
    Fraccion de intentos con violacion (e_f > cota + 1e-12), total y donde se cumple la
    condicion de asociacion de la Prop. 2: para todo p en P(f), P_c(ningun u toca p en c)
    >= prod_u (1 - m_{u,p}/n) (c uniforme en los bloques de la hora, incluido b).
P4: exceso de orden (`orden_swap.py`, funciones reutilizadas) con el nulo restringido a
    permutar tx_index DENTRO de celdas de posicion: quintiles de la posicion relativa
    (tx_index / tamano del bloque) y 10 tramos (deciles) como aproximacion de flashblock;
    el tamano de bloque es max(tx_index)+1 de las filas de rows.csv (proxy). Control sin
    celdas = nulo de bloque entero. Se reporta el exceso total (IC de bootstrap de horas) y
    el de los intentos de cada celda (sin IC; p de permutacion), para reverts y probes.
IC 95%: bootstrap de horas (B=500, semilla 20261017) sobre las sumas por hora.
"""
import argparse
import os
import re
import sys
import time
from collections import defaultdict

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402
import deriva as dv  # noqa: E402
import orden_swap as osw  # noqa: E402

SEMILLA = 20261017
B_BOOT = 500
K_P4 = 500
B_P4 = 300
TOL = 1e-12
(C_M, C_INV, C_S0, C_ENULL, C_N1, C_EF, C_RHS, C_VIOL, C_NCOND, C_VCOND,
 C_KSUM, C_NPOOL, C_NK1, C_NK2) = range(14)
NC = 14


class Acum:
    """Acumula por hora sumas ponderadas por intento (m) para P1, P1b y P2."""

    def __init__(self, bloques, txs):
        bl = set(bloques)
        self.horas = sorted({cc.block_timestamp(b) // 3600 for b in bl})
        hmap = {h: i for i, h in enumerate(self.horas)}
        self.nb = defaultdict(int)  # hora -> bloques sorteados
        for b in bl:
            self.nb[hmap[cc.block_timestamp(b) // 3600]] += 1
        paquete = defaultdict(set)
        for b, ent, pools, _ in txs:
            if pools and b in bl:
                paquete[(b, ent)].update(pools)
        n_p = defaultdict(int)
        self.m = defaultdict(lambda: defaultdict(int))  # (hora, pool) -> {entidad: m_up}
        for (b, ent), ps in paquete.items():
            for p in ps:
                n_p[p] += 1
                self.m[(hmap[cc.block_timestamp(b) // 3600], p)][ent] += 1
        self.w = {p: 1.0 / v for p, v in n_p.items()}
        self.v = np.zeros((len(self.horas), NC))

    def __call__(self, bi, h, m, s, a, mh, ent, pools, M):
        n = int(mh.sum()) + 1
        r = self.v[h]
        tot = sum(self.w[p] for p in pools)
        enull, ok, kmed = 0.0, True, []
        blk = mh.copy()
        blk[bi] = True
        for k, p in enumerate(pools):
            otros = [c for u, c in self.m[(h, p)].items() if u != ent]
            prod = 1.0
            for c in otros:
                prod *= 1.0 - c / n
            enull += self.w[p] / tot * (1.0 - prod)
            kmed.append(len(otros))
            if (M[blk, k] == 0).mean() < prod - TOL:
                ok = False
        r[C_M] += m
        r[C_INV] += m / n
        r[C_S0] += m * s[bi]
        r[C_ENULL] += m * enull
        r[C_KSUM] += m * sum(kmed)
        r[C_NPOOL] += m * len(pools)
        r[C_NK1] += m * sum(1 for x in kmed if x >= 1)
        r[C_NK2] += m * sum(1 for x in kmed if x >= 2)
        if n == 1:
            return
        ef = s[bi] - enull
        rhs = (1.0 - 1.0 / n) * (s[bi] - s[mh].mean())
        viol = ef > rhs + TOL
        r[C_N1] += m
        r[C_EF] += m * ef
        r[C_RHS] += m * rhs
        r[C_VIOL] += m * viol
        if ok:
            r[C_NCOND] += m
            r[C_VCOND] += m * viol


def razones(v, num, den):
    """sum(v[..., num]) / sum(v[..., den]) por fila de remuestreo (v: (b, NC))."""
    with np.errstate(invalid="ignore", divide="ignore"):
        return v[..., num].sum(-1) / v[..., den].sum(-1)


def boot_horas(v, rng, b=B_BOOT):
    """(b, NC) sumas de v remuestreando horas con reposicion."""
    H = len(v)
    pesos = rng.multinomial(H, np.full(H, 1.0 / H), size=b).astype(float)
    return pesos @ v


def pct(x):
    x = x[~np.isnan(x)]
    return (float(np.percentile(x, 2.5)), float(np.percentile(x, 97.5)))


def leer_E(path, seccion=None):
    """{estrato: (E, E_lo, E_hi, intentos, null, banda_lo, banda_hi, permutaciones)} de la
    tabla de `contend`; con `seccion`, solo esa ('== metric <seccion>')."""
    out, activo, perm = {}, seccion is None, 1000
    rx = re.compile(r"^(S\d+)\s+\d+\s+\d+\s+(\d+)\s+[\d.]+\s+[\d.]+\s+\[[^\]]*\]\s+([\d.]+)\s+"
                    r"\[([\d.]+),([\d.]+)\]\s+([-+]?[\d.]+)\s+\[([-+\d.]+),([-+\d.]+)\]")
    with open(path) as fh:
        for linea in fh:
            mp = re.search(r"permutations (\d+)", linea)
            if mp:
                perm = int(mp.group(1))
            if linea.startswith("== metric"):
                activo = seccion is None or linea.strip() == f"== metric {seccion}"
                continue
            m = rx.match(linea)
            if activo and m:
                g = m.groups()
                out[g[0]] = (float(g[5]), float(g[6]), float(g[7]), int(g[1]),
                             float(g[2]), float(g[3]), float(g[4]), perm)
    return out


def analizar_estrato(e):
    txs = [(r.block, e.entidad(r), r.pools, r.fallida) for r in e.filas_bot if r.pools]
    ac = Acum(e.bloques, txs)
    res = dv.construir(e.bloques, txs, gancho=ac)
    return res, ac.v


def _ic(x):
    return "n/a" if x is None else f"[{x[0]:+.3f},{x[1]:+.3f}]"


# ---------------------------------------------------------------- P4

def celda_rel(tx_index, tam, q):
    """Celda 0..q-1 de la posicion relativa tx_index / tam."""
    return min(q - 1, int(q * tx_index / max(tam, 1)))


def con_nulo_en_celdas(d, bloques_ord, tam, q):
    """Copia de `d` cuyo `bloque` agrupa por (bloque, celda): `osw._rango_nulo` ordena por
    esa clave, asi que solo se permuta dentro de cada celda y el orden entre celdas se
    conserva (las celdas crecen con la posicion)."""
    cel = np.array([celda_rel(r, tam[bloques_ord[b]], q) for r, b in zip(d.rank, d.bloque)],
                   dtype=np.int64)
    dd = osw.Datos()
    dd.__dict__.update(d.__dict__)
    dd.bloque = d.bloque * q + cel
    return dd


def p4_estrato(e, si, a, permitidos):
    con_pools = [r for r in e.filas_bot if r.pools]
    w = osw.pesos_semana(con_pools, e.entidad)
    sw, sin = {}, set()
    for b in e.bloques:
        if not cc.tiene_raw(a.raw, b):
            sin.add(b)
            continue
        sw.update(osw.swaps_por_tx(cc.leer_raw(a.raw, b, permitidos)[1]))
    txs = [(r.block, r.tx_index, e.entidad(r), r.pools, r.kind,
            sw.get(r.tx_hash, frozenset()) if r.kind == "success" else frozenset())
           for r in con_pools if r.block not in sin]
    tam = defaultdict(int)
    for r in e.filas:
        tam[r.block] = max(tam[r.block], r.tx_index + 1)
    ord_ = sorted({t[0] for t in txs if t[4] == "success" or t[4] in cc.FALLIDOS})
    sal = []
    for esq, q in (("bloque", 1), ("quintil", 5), ("decil", 10)):
        rng = np.random.default_rng([SEMILLA, si, q])
        d = osw.preparar(txs, w)
        total = osw.analizar(con_nulo_en_celdas(d, ord_, tam, q), k=K_P4, b=B_P4, rng=rng)
        celdas = {"revert": [], "probe": []}
        if q > 1:
            for c in range(q):
                dc = osw.preparar(
                    txs, w, celda=lambda f, c=c: 0 if celda_rel(f[1], tam[f[0]], q) == c else None)
                rc = osw.analizar(con_nulo_en_celdas(dc, ord_, tam, q), k=K_P4, b=0,
                                  rng=np.random.default_rng([SEMILLA, si, q, c]))
                for t in celdas:
                    celdas[t].append((rc[t]["n"], rc[t]["exceso"], rc[t]["p"]))
        sal.append((esq, q, total, celdas))
    return sal, len(sin)


def imprimir_p4(nombre, sal):
    for esq, q, total, celdas in sal:
        for t in ("revert", "probe"):
            x = total[t]
            ex = "n/a" if x["exceso"] is None else f"{x['exceso']:+.4f}"
            pv = "n/a" if x["p"] is None else f"{x['p']:.3f}"
            cel = ""
            if q > 1:
                cel = " | " + " ".join("n/a" if c[1] is None else f"{c[1]:+.3f}" for c in celdas[t])
                cel += " | n " + " ".join(str(c[0]) for c in celdas[t])
            print(f"{nombre:4} {esq:7} {t:7} {x['n']:>7} {ex:>8} {_ic(x['ic']):>17} {pv:>6}{cel}")


# ---------------------------------------------------------------- informe

def main():
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--rows", default="artefactos/exploratorio/rows.csv")
    p.add_argument("--blocks", default="datos/muestra_bloques_v1.csv")
    p.add_argument("--raw", default="artefactos/rpc/muestra_v1")
    p.add_argument("--split", choices=["exploratory", "confirmatory"], default="exploratory")
    p.add_argument("--E", default=None, help="archivo de E (default segun split)")
    p.add_argument("--solo-p4", action="store_true")
    p.add_argument("--sin-p4", action="store_true")
    a = p.parse_args()
    if a.E is None:
        a.E = ("artefactos/exploratorio/contend_v3_metricas.txt" if a.split == "exploratory"
               else "artefactos/confirmatorio/contend_primario.txt")
    sec = "weighted" if a.split == "exploratory" else None
    obs = leer_E(a.E, sec)
    permitidos = {b for bs in cc.cargar_muestra(a.blocks, a.split).values() for b in bs}
    print(f"# split={a.split} semilla={SEMILLA} B={B_BOOT} K_P4={K_P4} B_P4={B_P4}")
    print(f"# E y null: {a.E} seccion={sec or '(unica, Weighted)'}")
    t0 = time.time()
    estr = cc.estratos(a.rows, a.blocks, a.split)
    acums, preds = [], {}
    if not a.solo_p4:
        print("## P1: E_nulo exacta (media por intento) vs null de contend")
        print(f"{'est':4} {'n_int':>7} {'E_nulo_ex':>9} {'null':>7} {'dif':>8} {'se_MC':>7} "
              f"{'z':>6} {'E_exacta':>8} {'E_contend':>9}")
        p2 = []
        for si, e in enumerate(estr):
            res, v = analizar_estrato(e)
            tot = v.sum(0)
            bo = boot_horas(v, np.random.default_rng([SEMILLA, si]))
            pb = 1 - razones(bo, [C_INV], [C_M])
            preds[e.nombre] = (1 - tot[C_INV] / tot[C_M], pb,
                               tot[C_KSUM] / tot[C_NPOOL], tot[C_NK2] / tot[C_NK1])
            o = obs[e.nombre]
            en = tot[C_ENULL] / tot[C_M]
            se = np.hypot((o[6] - o[5]) / 3.92 / np.sqrt(o[7]), 0.0005 / np.sqrt(3))
            print(f"{e.nombre:4} {int(tot[C_M]):>7} {en:>9.4f} {o[4]:>7.4f} {en - o[4]:>+8.4f} "
                  f"{se:>7.4f} {(en - o[4]) / se:>+6.1f} {tot[C_S0] / tot[C_M] - en:>+8.4f} "
                  f"{o[0]:>+9.4f}")
            p2.append((e.nombre, tot, bo))
        print("## P2: e_f exacto vs (1-1/n)(s_f - C_-b,f), solo n>1")
        print(f"{'est':4} {'n_int>1':>8} {'mean e_f':>8} {'mean cota':>9} {'viol':>6} "
              f"{'IC95%':>15} {'cond':>6} {'viol|cond':>9}")
        for nombre, tot, bo in p2:
            vb = pct(razones(bo, [C_VIOL], [C_N1]))
            print(f"{nombre:4} {int(tot[C_N1]):>8} {tot[C_EF] / tot[C_N1]:>+8.4f} "
                  f"{tot[C_RHS] / tot[C_N1]:>+9.4f} {tot[C_VIOL] / tot[C_N1]:>6.3f} "
                  f"{_ic(vb):>15} {tot[C_NCOND] / tot[C_N1]:>6.3f} "
                  f"{tot[C_VCOND] / tot[C_NCOND] if tot[C_NCOND] else float('nan'):>9.3f}")
        print(f"# P1/P2 {time.time() - t0:.1f}s")
        if a.split == "confirmatory":
            p1b(a, preds, obs)
        else:
            print("## redundancia por pool (exploratorio): est k_medio frac(k>=2|k>=1)")
            for n_, (pr, _, k, k2) in preds.items():
                print(f"{n_:4} {k:>6.3f} {k2:>6.3f}")
    if a.sin_p4:
        return
    print("## P4: exceso de orden con nulo dentro de celdas de posicion (K=%d); columnas "
          "'celdas' = exceso de los intentos de cada celda" % K_P4)
    print(f"{'est':4} {'celdas':7} {'tipo':7} {'n_int':>7} {'exceso':>8} {'IC95%':>17} {'p':>6}")
    sin_tot = set()
    for si, e in enumerate(estr):
        t1 = time.time()
        sal, sin = p4_estrato(e, si, a, permitidos)
        sin_tot.add(sin)
        imprimir_p4(e.nombre, sal)
        print(f"# {e.nombre} P4 {time.time() - t1:.1f}s", flush=True)
    print(f"# bloques sin crudo por estrato: {sorted(sin_tot)}; total {time.time() - t0:.1f}s")


def p1b(a, preds_conf, obs_conf):
    """P1b: cociente predicho y observado exploratorio/confirmatorio (exploratorio con las
    rutas por defecto; solo usa 1-E[1/n] y m_{u,p}, no toca la evidencia reservada)."""
    rows = "artefactos/exploratorio/rows.csv"
    obs_e = leer_E("artefactos/exploratorio/contend_v3_metricas.txt", "weighted")
    print("## P1b: cociente expl/conf y redundancia por pool")
    print(f"{'est':4} {'pred':>6} {'IC95% pred':>15} {'obs':>6} {'IC95% obs (delta)':>19} "
          f"{'dentro':>7} {'k_expl':>6} {'k_conf':>6} {'k2_e':>5} {'k2_c':>5}")
    for si, e in enumerate(cc.estratos(rows, a.blocks, "exploratory")):
        _, v = analizar_estrato(e)
        tot = v.sum(0)
        bo = boot_horas(v, np.random.default_rng([SEMILLA, 100 + si]))
        pe = 1 - razones(bo, [C_INV], [C_M])
        pc = preds_conf[e.nombre][1]
        n = min(len(pe), len(pc))
        pred, ro, ic = cociente_p1(1 - tot[C_INV] / tot[C_M], preds_conf[e.nombre][0],
                                   obs_e[e.nombre], obs_conf[e.nombre])
        pic = pct(pe[:n] / pc[:n])
        print(f"{e.nombre:4} {pred:>6.3f} {_ic(pic):>15} {ro:>6.3f} {_ic(ic):>19} "
              f"{'si' if ic[0] <= pred <= ic[1] else 'NO':>7} {tot[C_KSUM] / tot[C_NPOOL]:>6.3f} "
              f"{preds_conf[e.nombre][2]:>6.3f} {tot[C_NK2] / tot[C_NK1]:>5.3f} "
              f"{preds_conf[e.nombre][3]:>5.3f}")


def cociente_p1(pred_a, pred_b, obs_a, obs_b):
    """Cociente predicho y observado con IC delta del observado.

    obs_x = (E, lo, hi, ...); IC delta suponiendo splits independientes."""
    ra = obs_a[0] / obs_b[0]
    sa, sb = (obs_a[2] - obs_a[1]) / 3.92, (obs_b[2] - obs_b[1]) / 3.92
    se = abs(ra) * np.sqrt((sa / obs_a[0]) ** 2 + (sb / obs_b[0]) ** 2)
    return pred_a / pred_b, ra, (ra - 1.96 * se, ra + 1.96 * se)


if __name__ == "__main__":
    main()
