"""(g) Orden previo de un Swap exitoso ajeno, con nulo de orden dentro del bloque
(V2 de la auditoria; hermano de `eventos.py`, fundamento `contencion-nulos-secundarios.md`
4.2).

Para cada intento fallido f (revert o probe de una entidad bot, con pools) de un bloque con
crudo: S_prev(f) = pools de P(f) en los que OTRA entidad bot tuvo un Swap exitoso antes en
el mismo bloque (tx exitosa con un log Swap emitido por ese pool en sus receipts y
tx_index menor que el de f). Metrica s_prev(f) = sum_{p en S_prev(f)} w_p / sum_{p en P(f)}
w_p, con los pesos w_p = 1/n_p de `contend` (n_p sobre todos los bloques de la semana);
la media por estrato es la media simple sobre los intentos fallidos del tipo.

Nulo: dentro de cada bloque se permuta uniformemente el tx_index entre las tx de bots con
pools (exitos y fallos), conservando identidades y pools. Solo importa el orden relativo,
asi que se permutan unicamente las tx que participan en algun (f, p, exito candidato).
K=1000, semilla fija 20261017 (default_rng([semilla, indice_de_estrato])), p de una cola
(1 + #{nulo >= obs}) / (K + 1), exceso = obs - media(nulo). IC 95% del exceso: bootstrap de
horas (B=1000, K'=50 de las K permutaciones), como en `eventos.py`.

Tipos: revert, probe, todos. Bloques sin crudo (receipts) se omiten por completo, tambien
de los denominadores, y se cuentan al final. Limite: el orden en Base lo fija la llegada
al Flashblock y la prioridad de fee, no la causalidad.
"""
import argparse
import os
import sys
from collections import defaultdict

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402

SEMILLA = 20261017
K_PERM = 1000
B_BOOT = 1000
K_BOOT = 50
TIPOS = ("revert", "probe", "todos")
_COLS = {"revert": [0], "probe": [1], "todos": [0, 1]}


def swaps_por_tx(receipts):
    """tx_hash -> frozenset de pools con un log Swap en una tx exitosa (status 0x1)."""
    out = {}
    for rc in receipts:
        if str(rc.get("status", "")).lower() not in ("0x1", "0x01", "1"):
            continue
        s = set()
        for log in rc.get("logs", []):
            topics = log.get("topics") or []
            if topics and topics[0].lower() in cc.SWAP_TOPICS:
                s.add(log["address"].lower())
        if s:
            out[rc["transactionHash"].lower()] = frozenset(s)
    return out


class Datos:
    """Items (f, p) con candidatos, en forma plana.

    item_f: indice local de la tx fallida de cada item; item_w: w_p / W_f; item_col: 0 revert,
    1 probe; item_hora; cand: indices locales de candidatos concatenados; starts: inicio de
    cada item en cand; rank: tx_index observado de cada tx local; bloque: bloque de cada tx
    local; den: (H, 2) numero de intentos fallidos con W_f > 0 por hora y tipo.
    """


def preparar(txs, w, celda=None):
    """txs: iterable de (block, tx_index, entidad, pools, kind, swappools) de bots con pools;
    w: {pool: peso}. swappools: pools con Swap exitoso (vacio salvo kind 'success').
    `celda` (opcional): funcion de la tx fallida a una etiqueta que sustituye a la hora como
    unidad de agrupacion (None omite el intento) (`descomposicion.py`); `Datos.etiquetas` da etiqueta -> indice."""
    por_bloque = defaultdict(list)
    for t in txs:
        if t[4] == "success" or t[4] in cc.FALLIDOS:
            por_bloque[t[0]].append(t)
    horas = {}
    loc = {}
    rank, bloque = [], []
    item_f, item_w, item_col, item_hora, cand, starts = [], [], [], [], [], []
    den_l = defaultdict(lambda: [0, 0])
    for bi, b in enumerate(sorted(por_bloque)):
        h = None if celda else horas.setdefault(cc.block_timestamp(b) // 3600, len(horas))
        lista = sorted(por_bloque[b], key=lambda t: t[1])
        exitos_pool = defaultdict(list)
        for i, t in enumerate(lista):
            if t[4] == "success":
                for p in t[5]:
                    exitos_pool[p].append(i)

        def local(i):
            if (b, i) not in loc:
                loc[(b, i)] = len(rank)
                rank.append(lista[i][1])
                bloque.append(bi)
            return loc[(b, i)]

        for j, f in enumerate(lista):
            if f[4] not in cc.FALLIDOS:
                continue
            total = sum(w.get(p, 0.0) for p in f[3])
            if total <= 0:
                continue
            col = 0 if f[4] == "revert" else 1
            hf = h
            if celda:
                etiqueta = celda(f)
                if etiqueta is None:
                    continue
                hf = horas.setdefault(etiqueta, len(horas))
            den_l[hf][col] += 1
            for p in f[3]:
                cs = [i for i in exitos_pool.get(p, ()) if lista[i][2] != f[2]]
                if not cs:
                    continue
                item_f.append(local(j))
                item_w.append(w.get(p, 0.0) / total)
                item_col.append(col)
                item_hora.append(hf)
                starts.append(len(cand))
                cand.extend(local(i) for i in cs)
    d = Datos()
    H = len(horas)
    d.n_horas = H
    d.etiquetas = horas
    d.item_f = np.array(item_f, dtype=np.int64)
    d.item_w = np.array(item_w)
    d.cell = np.array(item_hora, dtype=np.int64) * 2 + np.array(item_col, dtype=np.int64)
    d.cand = np.array(cand, dtype=np.int64)
    d.starts = np.array(starts, dtype=np.int64)
    d.rank = np.array(rank, dtype=np.int64)
    d.bloque = np.array(bloque, dtype=np.int64)
    d.den = np.zeros((H, 2))
    for h, (r, p) in den_l.items():
        d.den[h] = (r, p)
    return d


def _numeradores(d, rank):
    """(H*2,) suma de pesos de items con algun candidato de rango menor que el de f."""
    if len(d.starts) == 0:
        return np.zeros(2 * d.n_horas)
    minimo = np.minimum.reduceat(rank[d.cand], d.starts)
    hit = minimo < rank[d.item_f]
    return np.bincount(d.cell, weights=d.item_w * hit, minlength=2 * d.n_horas)


def _rango_nulo(d, rng):
    clave = d.bloque + rng.random(len(d.bloque))
    orden = np.argsort(clave, kind="stable")
    nuevo = np.empty(len(orden), dtype=np.int64)
    nuevo[orden] = np.arange(len(orden))
    return nuevo


def analizar(d, k=K_PERM, b=B_BOOT, kb=K_BOOT, rng=None):
    """{tipo: dict(n, obs, nulo, exceso, ic, p)}; b=0 omite el bootstrap."""
    rng = rng if rng is not None else np.random.default_rng(SEMILLA)
    H = d.n_horas
    vacio = dict(n=0, obs=None, nulo=None, exceso=None, ic=None, p=None)
    if H == 0:
        return {t: dict(vacio) for t in TIPOS}
    obs = _numeradores(d, d.rank).reshape(H, 2)
    nulo = np.empty((k, H, 2))
    for i in range(k):
        nulo[i] = _numeradores(d, _rango_nulo(d, rng)).reshape(H, 2)
    if b:
        pesos = rng.multinomial(H, np.full(H, 1.0 / H), size=b).astype(float)
        elegir = np.stack([rng.choice(k, size=min(kb, k), replace=False) for _ in range(b)])
    out = {}
    for t in TIPOS:
        cols = _COLS[t]
        o, dn = obs[:, cols].sum(1), d.den[:, cols].sum(1)
        nu = nulo[:, :, cols].sum(2)
        N = dn.sum()
        if N == 0:
            out[t] = dict(vacio)
            continue
        s_obs = o.sum() / N
        s_nul = nu.sum(1) / N
        p = (1 + int((s_nul >= s_obs - 1e-12).sum())) / (k + 1)
        ic = None
        if b:
            den_b = pesos @ dn
            ok = den_b > 0
            obs_b = np.where(ok, (pesos @ o) / np.where(ok, den_b, 1), np.nan)
            nu_sel = np.take_along_axis(pesos @ nu.T, elegir, axis=1).mean(1)
            nul_b = np.where(ok, nu_sel / np.where(ok, den_b, 1), np.nan)
            ex = (obs_b - nul_b)[ok]
            ic = (float(np.percentile(ex, 2.5)), float(np.percentile(ex, 97.5)))
        out[t] = dict(n=int(N), obs=float(s_obs), nulo=float(s_nul.mean()),
                      exceso=float(s_obs - s_nul.mean()), ic=ic, p=float(p))
    return out


def pesos_semana(filas_bot_con_pools, entidad):
    """w_p = 1/n_p con n_p = paquetes entidad-bloque que tocan p (como `contend`)."""
    paquetes = defaultdict(set)
    for r in filas_bot_con_pools:
        paquetes[(r.block, entidad(r))].update(r.pools)
    n = defaultdict(int)
    for ps in paquetes.values():
        for p in ps:
            n[p] += 1
    return {p: 1.0 / v for p, v in n.items()}


def _f(x, f):
    return "n/a" if x is None else format(x, f)


def main():
    a = cc.argumentos(__doc__)
    permitidos = {b for bs in cc.cargar_muestra(a.blocks, a.split).values() for b in bs}
    print(f"# semilla={SEMILLA} K={K_PERM} B={B_BOOT} K'={K_BOOT} split={a.split}")
    print(f"{'estrato':7} {'tipo':7} {'n_int':>8} {'s_obs':>7} {'s_nulo':>7} {'exceso':>8} "
          f"{'IC95%':>19} {'p':>7}")
    sin_crudo = set()
    for si, e in enumerate(cc.estratos(a.rows, a.blocks, a.split)):
        con_pools = [r for r in e.filas_bot if r.pools]
        w = pesos_semana(con_pools, e.entidad)
        sw = {}
        for b in e.bloques:
            if not cc.tiene_raw(a.raw, b):
                sin_crudo.add(b)
                continue
            sw.update(swaps_por_tx(cc.leer_raw(a.raw, b, permitidos)[1]))
        txs = [(r.block, r.tx_index, e.entidad(r), r.pools, r.kind,
                sw.get(r.tx_hash, frozenset()) if r.kind == "success" else frozenset())
               for r in con_pools if r.block not in sin_crudo]
        d = preparar(txs, w)
        res = analizar(d, rng=np.random.default_rng([SEMILLA, si]))
        for t in TIPOS:
            r = res[t]
            ic = "n/a" if r["ic"] is None else f"[{r['ic'][0]:+.3f},{r['ic'][1]:+.3f}]"
            print(f"{e.nombre:7} {t:7} {r['n']:>8} {_f(r['obs'], '.4f'):>7} "
                  f"{_f(r['nulo'], '.4f'):>7} {_f(r['exceso'], '+.4f'):>8} {ic:>19} "
                  f"{_f(r['p'], '.4f'):>7}")
    print(f"bloques sin crudo: {len(sin_crudo)} ({', '.join(map(str, sorted(sin_crudo)))})")


if __name__ == "__main__":
    main()
