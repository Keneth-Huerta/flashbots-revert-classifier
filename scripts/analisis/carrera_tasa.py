"""Tasa de revert segun swap previo ajeno, condicionando en entidad y posicion
(`descomposicion-exceso.md` 4.4; EXPL-20, P4, prueba que discrimina).

Comparar la posicion de los reverts con y sin S_prev es mecanico: cuanto mas tarde esta una
tx, mas probable que alguien haya hecho swap antes. Aqui se condiciona. Universo: tx de
entidades bot con pools no vacios (universo y entidades de `comun_contencion`), de bloques con
crudo. X(t) = 1 si OTRA entidad tuvo un swap exitoso antes en el mismo bloque en algun pool
de P(t) (mismo criterio que `orden_swap.py`, via `swaps_por_tx`). Resultado y = 1 si la tx es
revert. Dos variantes: "sin_probes" (los probes salen del universo) y "con_probes" (los
probes quedan con y = 0).

Estratos de celda: (entidad, decil de q), con q = tx_index / (max_tx_index_del_bloque + 1),
dentro de cada estrato-semana. Estimador: diferencia de riesgos de Cochran-Mantel-Haenszel,
sum_c w_c d_c / sum_c w_c con d_c = tasa(X=1) - tasa(X=0) y w_c = n1 n0 / (n1 + n0), sobre
celdas con ambos grupos. IC 95% por bootstrap de horas (B=1000); p de una cola (1 + #{perm >=
obs}) / (K + 1) permutando X dentro de cada celda (K=1000). Semilla 20261017
(default_rng([semilla, indice_de_estrato, variante])). Tambien se reporta la misma diferencia
con celdas solo de decil (sin entidad) para ver cuanto mueve la composicion.
Prediccion bajo carrera por estado: diferencia MH > 0. Descriptivo: la llegada y el fee
tambien fijan el orden, y X mira solo swaps exitosos (puede haber confusion por pool).
"""
import os
import sys
from collections import defaultdict

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402
import orden_swap as osw  # noqa: E402

SEMILLA = 20261017
B_BOOT = 1000
K_PERM = 1000
N_DECIL = 10
VARIANTES = ("sin_probes", "con_probes")


def x_por_bloque(lista):
    """lista: tx de UN bloque ordenadas por tx_index, (tx_index, entidad, pools, kind,
    swappools). Devuelve X (0/1) por tx: swap exitoso previo de OTRA entidad en algun pool."""
    exitos = defaultdict(list)
    for t in lista:
        if t[3] == "success":
            for p in t[4]:
                exitos[p].append((t[0], t[1]))
    return [int(any(i < idx and e != ent for p in pools for i, e in exitos.get(p, ())))
            for idx, ent, pools, _, _ in lista]


def construir(txs, tam, con_probes, nbins=N_DECIL, devolver_bloque=False, cadena="base"):
    """txs: (block, tx_index, entidad, pools, kind, swappools). Devuelve arrays
    (ent_id, bin, hora, X, y) y H (y el bloque por tx si `devolver_bloque`). Solo kind in
    {success, revert, probe}; bin = floor(q * nbins)."""
    por_bloque = defaultdict(list)
    for t in txs:
        if t[4] in ("success", "revert", "probe"):
            por_bloque[t[0]].append(t)
    ents, hora_id = {}, {}
    ent, dec, hor, xs, ys, blq = [], [], [], [], [], []
    for b in sorted(por_bloque):
        h = hora_id.setdefault(cc.block_timestamp(b, cadena) // 3600, len(hora_id))
        lista = sorted(por_bloque[b], key=lambda t: t[1])
        x = x_por_bloque([(t[1], t[2], t[3], t[4], t[5]) for t in lista])
        for t, xv in zip(lista, x):
            if t[4] == "probe" and not con_probes:
                continue
            ent.append(ents.setdefault(t[2], len(ents)))
            dec.append(min(int(t[1] / tam[b] * nbins), nbins - 1))
            blq.append(b)
            hor.append(h)
            xs.append(xv)
            ys.append(1 if t[4] == "revert" else 0)
    a = lambda v: np.array(v, dtype=np.int64)  # noqa: E731
    if devolver_bloque:
        return a(ent), a(dec), a(hor), a(xs), a(ys), len(hora_id), a(blq)
    return a(ent), a(dec), a(hor), a(xs), a(ys), len(hora_id)


def _mh(n1, r1, n0, r0):
    """MH risk difference sobre ultimo eje (celdas). Devuelve (est, n_celdas_usadas)."""
    ok = (n1 > 0) & (n0 > 0)
    tot = np.where(ok, n1 + n0, 1)
    w = np.where(ok, n1 * n0 / tot, 0.0)
    d = np.where(ok, r1 / np.maximum(n1, 1) - r0 / np.maximum(n0, 1), 0.0)
    sw = w.sum(axis=-1)
    with np.errstate(invalid="ignore", divide="ignore"):
        est = (w * d).sum(axis=-1) / sw
    return np.where(sw > 0, est, np.nan), ok.sum(axis=-1)


def _conteos(celda, x, y, nc):
    n1 = np.bincount(celda, weights=x, minlength=nc)
    r1 = np.bincount(celda, weights=x * y, minlength=nc)
    n = np.bincount(celda, minlength=nc).astype(float)
    r = np.bincount(celda, weights=y, minlength=nc)
    return n1, r1, n - n1, r - r1


def mh_stat(celda, x, y):
    """(estimador MH, celdas usadas) con celda = id entero de celda por tx."""
    nc = int(celda.max()) + 1 if len(celda) else 0
    e, k = _mh(*_conteos(celda, x, y, nc))
    return float(e), int(k)


def mh_boot(celda, hora, x, y, H, rng, b=B_BOOT):
    """IC 95% por bootstrap de horas. Devuelve (lo, hi) o (nan, nan)."""
    # celdas con un solo grupo no aportan nunca: se podan (mismo resultado, menos memoria)
    nc0 = int(celda.max()) + 1
    n1 = np.bincount(celda, weights=x, minlength=nc0)
    n0 = np.bincount(celda, minlength=nc0) - n1
    nuevo = np.cumsum((n1 > 0) & (n0 > 0)) - 1
    m = ((n1 > 0) & (n0 > 0))[celda]
    if not m.any():
        return float("nan"), float("nan")
    celda, hora, x, y = nuevo[celda[m]], hora[m], x[m], y[m]
    nc = int(celda.max()) + 1
    T = np.zeros((nc, H, 4))
    for k, v in enumerate((x, x * y, 1 - x, (1 - x) * y)):
        T[:, :, k] = np.bincount(celda * H + hora, weights=v, minlength=nc * H).reshape(nc, H)
    pesos = rng.multinomial(H, np.full(H, 1.0 / H), size=b).astype(float)
    es = []
    for i in range(0, b, 100):
        S = np.einsum("bh,chk->bck", pesos[i:i + 100], T)
        es.append(_mh(S[..., 0], S[..., 1], S[..., 2], S[..., 3])[0])
    e = np.concatenate(es)
    e = e[~np.isnan(e)]
    if len(e) == 0:
        return float("nan"), float("nan")
    return float(np.percentile(e, 2.5)), float(np.percentile(e, 97.5))


def mh_perm_p(celda, x, y, rng, k=K_PERM):
    """p de una cola permutando X dentro de cada celda. Devuelve (p, obs)."""
    obs, _ = mh_stat(celda, x, y)
    if np.isnan(obs):
        return float("nan"), obs
    orden = np.argsort(celda, kind="stable")
    c, xs, ys = celda[orden], x[orden], y[orden]
    ge = 0
    for _ in range(k):
        perm = np.lexsort((rng.random(len(c)), c))
        s, _ = mh_stat(c, xs[perm], ys)
        ge += s >= obs - 1e-12
    return (1 + ge) / (k + 1), obs


def analiza(ent, dec, hora, x, y, H, rng, b=B_BOOT, k=K_PERM, nbins=N_DECIL, blq=None):
    """Dict con n, tasas crudas y, para celdas (entidad, decil) y (decil), diferencia MH,
    IC, p y celdas usadas."""
    out = {"n": len(x), "n1": int(x.sum()),
           "t0": float(y[x == 0].mean()) if (x == 0).any() else float("nan"),
           "t1": float(y[x == 1].mean()) if (x == 1).any() else float("nan")}
    if blq is None:  # celda = (entidad, bin)
        cond = ent * nbins + dec
    else:  # celda = (entidad, bloque): elimina todo lo propio del bloque
        cond = ent * (int(blq.max()) + 1) + blq
    for nombre, celda in (("cond", cond), ("dec", dec)):
        _, celda = np.unique(celda, return_inverse=True)
        p, obs = mh_perm_p(celda, x, y, rng, k)
        _, ku = mh_stat(celda, x, y)
        lo, hi = mh_boot(celda, hora, x, y, H, rng, b) if ku else (float("nan"),) * 2
        out[nombre] = {"d": obs, "ic": (lo, hi), "p": p, "celdas": ku}
    return out


def _f(v, fmt="+.4f"):
    return "n/a" if v is None or np.isnan(v) else format(v, fmt)


def linea(nombre, var, r):
    c, d = r["cond"], r["dec"]
    return (f"{nombre:4} {var:10} n={r['n']:>7} nX1={r['n1']:>6} celdas={c['celdas']:>4} "
            f"tasa0={_f(r['t0'], '.4f')} tasa1={_f(r['t1'], '.4f')} "
            f"MH(ent,dec)={_f(c['d'])} [{_f(c['ic'][0])},{_f(c['ic'][1])}] p={_f(c['p'], '.3f')} | "
            f"solo-decil({d['celdas']})={_f(d['d'])} [{_f(d['ic'][0])},{_f(d['ic'][1])}] "
            f"p={_f(d['p'], '.3f')}")


def main():
    nbins, bloque = N_DECIL, False
    op = cc.opciones_cadena()  # --chain, --exclude-system, --system-list, --max-sender-degree
    if "--bins" in sys.argv:  # opciones propias; el resto lo parsea `comun_contencion`
        i = sys.argv.index("--bins")
        nbins = int(sys.argv[i + 1])
        del sys.argv[i:i + 2]
    if "--bloque" in sys.argv:
        bloque = True
        sys.argv.remove("--bloque")
    solo_sin = "--sin-probes" in sys.argv
    if solo_sin:
        sys.argv.remove("--sin-probes")
    a = cc.argumentos(__doc__ + "\nOpciones extra: --bins N (default 10), --bloque (celda = "
                      "entidad x bloque), --sin-probes (solo esa variante), --chain base|unichain|op, "
                      "--exclude-system[=BOOL] (default: no en base, si en unichain y op), "
                      "--system-list RUTA, --max-sender-degree N.")
    modo = "entidad x bloque" if bloque else f"entidad x bin (N={nbins})"
    permitidos = {b for bs in cc.cargar_muestra(a.blocks, a.split).values() for b in bs}
    print(f"# semilla={SEMILLA} B={B_BOOT} K={K_PERM} split={a.split}; p de una cola (MH > 0); celdas {modo}")
    sin_crudo = set()
    for si, e in enumerate(cc.estratos(a.rows, a.blocks, a.split, op["filtro"], op["max_grado"])):
        tam = defaultdict(int)
        for r in e.filas:
            tam[r.block] = max(tam[r.block], r.tx_index + 1)
        con_pools = [r for r in e.filas_bot if r.pools]
        sw = {}
        for b in e.bloques:
            if not os.path.isfile(os.path.join(a.raw, f"{b}.receipts.json.gz")):  # solo receipts (desviacion post-sello AF-02)
                sin_crudo.add(b)
                continue
            sw.update(osw.swaps_por_tx(cc._leer_gz(a.raw, b, "receipts", permitidos)))
        txs = [(r.block, r.tx_index, e.entidad(r), r.pools, r.kind,
                sw.get(r.tx_hash, frozenset()) if r.kind == "success" else frozenset())
               for r in con_pools if r.block not in sin_crudo]
        for vi, var in enumerate(VARIANTES):
            if solo_sin and var != "sin_probes":
                continue
            ent, dec, hora, x, y, H, blq = construir(txs, tam, var == "con_probes", nbins, True, op["cadena"])
            if len(x) == 0:
                continue
            rng = np.random.default_rng([SEMILLA, si, vi])
            print(linea(e.nombre, var, analiza(ent, dec, hora, x, y, H, rng, nbins=nbins,
                                               blq=blq if bloque else None)), flush=True)
    print(f"bloques sin crudo: {len(sin_crudo)}")


if __name__ == "__main__":
    main()
