"""(d) Estudio de eventos: orden dentro del bloque, con nulo de orden (fundamento
`contencion-nulos-secundarios.md` 4.2).

Para cada exito s (tx exitosa de bot con pools) de la entidad u en el bloque b, O(s) son
los intentos fallidos de OTRAS entidades en b que comparten algun pool con s. Estadistico
D = (# de f en O(s) con tx_index > idx(s)) / (# total en O(s)), sumado sobre exitos, por
estrato (semana), grupo del exito (ciclicos / no_ciclicos) y tipo de fallo (revert, probe,
todos).

Nulo: dentro de cada bloque, permutacion uniforme del tx_index entre las tx de bots con
pools (exitos y fallos juntos), conservando identidades y pools. Solo importa el orden
relativo, asi que se permutan unicamente las tx que forman algun par (s, f); su orden
relativo bajo una permutacion uniforme del bloque entero es uniforme. K=1000 permutaciones,
semilla fija 20261017 (cada estrato usa default_rng([semilla, indice_de_estrato])), p de una
cola (1 + #{D_nulo >= D_obs}) / (K + 1), exceso = D_obs - media(D_nulo).

IC 95% del exceso: bootstrap de horas completas (B=1000 replicas; en cada una se remuestrean
las horas con reemplazo, se recalcula D_obs y la media del nulo con K'=50 de las K
permutaciones guardadas, elegidas al azar), percentiles 2.5 y 97.5, como en `contend`.
"""
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
GRUPOS = ("ciclicos", "no_ciclicos")
TIPOS = ("revert", "probe", "todos")


class Datos:
    """Pares (exito, fallo) de un estrato, listos para vectorizar.

    ps, pf: indices (en el arreglo local de tx) del exito y del fallo de cada par.
    rank: rango observado (por tx_index) de cada tx dentro de su bloque.
    bloque: indice de bloque (0..) de cada tx local; celda: columna 0..3 de cada par
    (grupo*2 + tipo con revert=0, probe=1); hora: indice de hora de cada par.
    """

    def __init__(self, ps, pf, rank, bloque, celda, hora, n_horas):
        self.ps, self.pf, self.rank, self.bloque = ps, pf, rank, bloque
        self.celda, self.hora, self.n_horas = celda, hora, n_horas
        self.num_celdas = 4 * n_horas
        self.pares_celda = self.hora * 4 + self.celda
        self.den = np.bincount(self.pares_celda, minlength=self.num_celdas).astype(float)
        self.n_exitos = None  # (2,): exitos por grupo con >=1 par? se fija en preparar


def preparar(txs):
    """txs: iterable de (block, tx_index, entidad, pools, kind, cyclic) de bots con pools.

    Solo kind 'success' y fallidas (revert/probe) entran. Devuelve Datos.
    """
    por_bloque = defaultdict(list)
    for t in txs:
        if t[4] == "success" or t[4] in cc.FALLIDOS:
            por_bloque[t[0]].append(t)
    ps, pf, rank, bloque, celda, hora_b = [], [], [], [], [], []
    n_exitos = [0, 0]
    n_local = 0
    horas = {}
    for bi, b in enumerate(sorted(por_bloque)):
        lista = sorted(por_bloque[b], key=lambda t: t[1])
        por_pool = defaultdict(list)
        for i, t in enumerate(lista):
            if t[4] in cc.FALLIDOS:
                for p in t[3]:
                    por_pool[p].append(i)
        pares = []
        for i, t in enumerate(lista):
            if t[4] != "success":
                continue
            fs = set()
            for p in t[3]:
                fs.update(por_pool.get(p, ()))
            fs = [j for j in fs if lista[j][2] != t[2]]
            if fs:
                n_exitos[0 if t[5] else 1] += 1
            for j in fs:
                pares.append((i, j))
        usados = sorted({i for pr in pares for i in pr})
        if not usados:
            continue
        loc = {i: n_local + k for k, i in enumerate(usados)}
        h = horas.setdefault(cc.block_timestamp(b) // 3600, len(horas))
        for i in usados:
            rank.append(lista[i][1])
            bloque.append(bi)
        for i, j in pares:
            ps.append(loc[i])
            pf.append(loc[j])
            g = 0 if lista[i][5] else 1
            celda.append(g * 2 + (0 if lista[j][4] == "revert" else 1))
            hora_b.append(h)
        n_local += len(usados)
    # rango observado = posicion por tx_index dentro del bloque (solo importa el orden)
    rank = np.array(rank, dtype=np.int64)
    bloque = np.array(bloque, dtype=np.int64)
    d = Datos(np.array(ps, dtype=np.int64), np.array(pf, dtype=np.int64), rank, bloque,
              np.array(celda, dtype=np.int64), np.array(hora_b, dtype=np.int64), len(horas))
    d.n_exitos = n_exitos
    return d


def _numeradores(d, rank):
    mayor = rank[d.pf] > rank[d.ps]
    return np.bincount(d.pares_celda, weights=mayor, minlength=d.num_celdas)


def _rango_nulo(d, rng):
    """Rango permutado uniformemente dentro de cada bloque (clave = bloque + U(0,1))."""
    clave = d.bloque + rng.random(len(d.bloque))
    orden = np.argsort(clave, kind="stable")
    # como las tx estan agrupadas por bloque, la posicion global en `orden` basta como rango
    nuevo = np.empty(len(orden), dtype=np.int64)
    nuevo[orden] = np.arange(len(orden))
    return nuevo


def _columnas(tipo, grupo):
    g = GRUPOS.index(grupo)
    return {"revert": [g * 2], "probe": [g * 2 + 1], "todos": [g * 2, g * 2 + 1]}[tipo]


def analizar(d, k=K_PERM, b=B_BOOT, kb=K_BOOT, rng=None):
    """Devuelve {(grupo, tipo): dict(n_pares, D_obs, D_nulo, exceso, ic, p)}.

    b=0 omite el bootstrap (ic=None).
    """
    rng = rng if rng is not None else np.random.default_rng(SEMILLA)
    H = d.n_horas
    out = {}
    if len(d.ps) == 0:
        return {(g, t): dict(n_pares=0, D_obs=None, D_nulo=None, exceso=None, ic=None, p=None)
                for g in GRUPOS for t in TIPOS}
    obs = _numeradores(d, d.rank).reshape(H, 4)
    nulo = np.empty((k, H, 4))
    for i in range(k):
        nulo[i] = _numeradores(d, _rango_nulo(d, rng)).reshape(H, 4)
    den = d.den.reshape(H, 4)
    if b:
        pesos = rng.multinomial(H, np.full(H, 1.0 / H), size=b).astype(float)  # (b, H)
        elegir = np.stack([rng.choice(k, size=min(kb, k), replace=False) for _ in range(b)])
    for g in GRUPOS:
        for t in TIPOS:
            cols = _columnas(t, g)
            o, dn = obs[:, cols].sum(1), den[:, cols].sum(1)
            nu = nulo[:, :, cols].sum(2)  # (k, H)
            N = dn.sum()
            if N == 0:
                out[(g, t)] = dict(n_pares=0, D_obs=None, D_nulo=None, exceso=None, ic=None, p=None)
                continue
            d_obs = o.sum() / N
            d_nul = nu.sum(1) / N
            p = (1 + int((d_nul >= d_obs - 1e-12).sum())) / (k + 1)
            ic = None
            if b:
                den_b = pesos @ dn  # (b,)
                ok = den_b > 0
                obs_b = np.where(ok, (pesos @ o) / np.where(ok, den_b, 1), np.nan)
                nu_b = pesos @ nu.T  # (b, k)
                nu_sel = np.take_along_axis(nu_b, elegir, axis=1).mean(1)
                nul_b = np.where(ok, nu_sel / np.where(ok, den_b, 1), np.nan)
                ex = (obs_b - nul_b)[ok]
                ic = (float(np.percentile(ex, 2.5)), float(np.percentile(ex, 97.5)))
            out[(g, t)] = dict(n_pares=int(N), D_obs=float(d_obs), D_nulo=float(d_nul.mean()),
                               exceso=float(d_obs - d_nul.mean()), ic=ic, p=float(p))
    return out


def _fmt(x, f):
    return "n/a" if x is None else format(x, f)


def main():
    a = cc.argumentos(__doc__)
    print(f"# semilla={SEMILLA} K={K_PERM} B={B_BOOT} K'={K_BOOT} split={a.split}")
    print(f"{'estrato':7} {'grupo':12} {'tipo':7} {'n_exitos':>9} {'n_pares':>9} {'D_obs':>7} "
          f"{'D_nulo':>7} {'exceso':>8} {'IC95%':>19} {'p':>7}")
    for si, e in enumerate(cc.estratos(a.rows, a.blocks, a.split)):
        txs = [(r.block, r.tx_index, e.entidad(r), r.pools, r.kind, r.cyclic)
               for r in e.filas_bot if r.pools]
        d = preparar(txs)
        res = analizar(d, rng=np.random.default_rng([SEMILLA, si]))
        for g in GRUPOS:
            # n_exitos: exitos del grupo con >=1 fallo ajeno en O(s) (de cualquier tipo)
            ne = d.n_exitos[GRUPOS.index(g)]
            for t in TIPOS:
                r = res[(g, t)]
                ic = "n/a" if r["ic"] is None else f"[{r['ic'][0]:+.3f},{r['ic'][1]:+.3f}]"
                print(f"{e.nombre:7} {g:12} {t:7} {ne:>9} {r['n_pares']:>9} "
                      f"{_fmt(r['D_obs'], '.4f'):>7} {_fmt(r['D_nulo'], '.4f'):>7} "
                      f"{_fmt(r['exceso'], '+.4f'):>8} {ic:>19} {_fmt(r['p'], '.4f'):>7}")


if __name__ == "__main__":
    main()
