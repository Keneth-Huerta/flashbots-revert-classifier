"""(f) Deriva dentro de la hora: curva C(Delta) por distancia y contraste con el vecino
(fundamento `contencion-nulos-secundarios.md` 4.3).

Para un intento fallido f de la entidad v en el bloque b y un bloque b' del mismo estrato
(semana), s(f, b') = sum_{p en S_b'(f)} w_p / sum_{p en P(f)} w_p, con S_b'(f) los pools de
P(f) que alguna entidad distinta de v toco (cualquier tx de bot con pools) en b'. Los pesos
w_p = 1/n_p (n_p = paquetes entidad-bloque de la semana que tocan p), el universo de bots y
las entidades son los de `comun_contencion` (= `contend`). Para b' = b, s es exactamente la
s(f) de `contend`, de modo que C(0) = CR_w,obs.

C(Delta) = media de s(f, b') sobre los pares (f, b') con |ts(b) - ts(b')| en el tramo.
Tramos (segundos, cerrados a la izquierda): 0 (mismo bloque), (0,2) min [extra, no entra en
E_vec], 2-5, 5-10, 10-20, 20-40, 40-60 min, 1-6 h, 6-24 h, >=1 dia. `b'` recorre todos los
bloques sorteados del estrato, tengan o no actividad (un bloque vacio aporta s = 0).
`n_pares` cuenta pares ordenados de bloques (b, b') con b con algun intento fallido.

Estadisticos:
  E_vec  = C(0) - C(primer tramo listado con >= 200 pares), libre de deriva en la medida en
           que el tramo vecino sea cercano.
  E_hora = C(0) - C(mismo bloque excluido, misma hora): contraste que aproxima el E del nulo
           primario (que permuta paquetes entre los bloques de la misma hora).
IC 95%: bootstrap de horas (B=500, semilla 20261017) remuestreando la hora del bloque b del
intento; las horas de b' no se remuestrean (cota de sensibilidad exploratoria).
Chequeo de coherencia: se imprime el E de `contend_primario.txt` junto a E_hora.
"""
import argparse
import os
import re
import sys
from collections import Counter, defaultdict

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402

SEMILLA = 20261017
B_BOOT = 500
MIN_PARES = 200
# (etiqueta, lo, hi) en segundos; el tramo 0 es "mismo bloque"
TRAMOS = [
    ("0", None, None),
    ("0-2m", 0, 120),
    ("2-5m", 120, 300),
    ("5-10m", 300, 600),
    ("10-20m", 600, 1200),
    ("20-40m", 1200, 2400),
    ("40-60m", 2400, 3600),
    ("1-6h", 3600, 21600),
    ("6-24h", 21600, 86400),
    (">1d", 86400, float("inf")),
]
NT = len(TRAMOS)
IDX_HORA = NT  # columna extra: misma hora, otros bloques
EXTRA = 1  # indice de "0-2m", fuera del E_vec


def indice_tramo(dt):
    """Tramo de una distancia en segundos (entero 0..NT-1). dt = 0 -> mismo bloque."""
    if dt == 0:
        return 0
    for i, (_, lo, hi) in enumerate(TRAMOS[1:], start=1):
        if lo <= dt < hi:
            return i
    raise ValueError(dt)


def construir(bloques, txs, gancho=None):
    """Acumula, por hora del bloque b y tramo, sum de s y numero de pares.

    bloques: bloques sorteados del estrato. txs: iterable de (bloque, entidad, pools, fallida)
    de las tx de bots con pools. Devuelve dict(num, den, n_pares, n_int, horas).
    num/den: (H, NT+1) con la ultima columna = misma hora otros bloques; den cuenta pares
    (intento, b') y num suma s. n_pares: pares de bloques (b, b') con b con intentos.
    `gancho` (opcional, para `descomposicion.py`) se llama por cada grupo de intentos iguales
    como gancho(bi, h, m, s, a, mh, ent, pools, M): s = puntuacion contra cada bloque b', a = suma de
    contribuciones aisladas por entidad (aditiva, sin truncar), mh = mascara de los otros
    bloques de la hora, ent/pools = entidad y pools del intento, M = (B, |pools|) otras
    entidades que tocan cada pool en cada bloque. Tambien se devuelve `hora_ts` (hora absoluta de cada fila de num).
    """
    bl = sorted(set(bloques))
    B = len(bl)
    bidx = {b: i for i, b in enumerate(bl)}
    ts = np.array([cc.block_timestamp(b) for b in bl], dtype=np.int64)
    hora_ts = ts // 3600
    horas = sorted(set(hora_ts.tolist()))
    hmap = {h: i for i, h in enumerate(horas)}
    hb = np.array([hmap[h] for h in hora_ts], dtype=np.int64)
    H = len(horas)

    # paquetes entidad-bloque
    paquete = defaultdict(set)
    grupos = Counter()
    for b, ent, pools, fallida in txs:
        if not pools or b not in bidx:
            continue
        paquete[(b, ent)].update(pools)
        if fallida:
            grupos[(bidx[b], ent, tuple(pools))] += 1
    pid = {}
    for ps in paquete.values():
        for p in ps:
            pid.setdefault(p, len(pid))
    npool = len(pid)
    cnt = np.zeros((B, max(npool, 1)), dtype=np.int16)
    n_p = np.zeros(max(npool, 1))
    propio = defaultdict(list)  # (entidad, pool) -> indices de bloque donde la entidad lo toca
    for (b, ent), ps in paquete.items():
        for p in ps:
            cnt[bidx[b], pid[p]] += 1
            n_p[pid[p]] += 1
            propio[(ent, pid[p])].append(bidx[b])
    w = np.divide(1.0, n_p, out=np.zeros_like(n_p), where=n_p > 0)

    dt = np.abs(ts[:, None] - ts[None, :])
    tr = np.zeros((B, B), dtype=np.int8)
    for i in range(1, NT):
        _, lo, hi = TRAMOS[i]
        tr[(dt > 0) & (dt >= lo) & (dt < hi)] = i
    misma_hora = (hb[:, None] == hb[None, :]) & (dt > 0)
    conteo_tr = np.stack([np.bincount(tr[i], minlength=NT) for i in range(B)]) if B else np.zeros((0, NT))
    conteo_h = misma_hora.sum(1)

    num = np.zeros((H, NT + 1))
    den = np.zeros((H, NT + 1))
    n_pares = np.zeros(NT + 1)
    n_int = 0
    con_intento = np.zeros(B, dtype=bool)
    for (bi, ent, pools), m in grupos.items():
        ids = np.array([pid[p] for p in pools], dtype=np.int64)
        M = cnt[:, ids].astype(np.int32)
        for k, j in enumerate(ids):
            M[propio[(ent, int(j))], k] -= 1
        wp = w[ids]
        total = wp.sum()
        if total <= 0:
            continue
        s = ((M > 0) @ wp) / total
        if gancho is not None:
            gancho(bi, hb[bi], m, s, (M @ wp) / total, misma_hora[bi], ent, pools, M)
        h = hb[bi]
        suma = np.bincount(tr[bi], weights=s, minlength=NT)
        num[h, :NT] += m * suma
        den[h, :NT] += m * conteo_tr[bi]
        num[h, IDX_HORA] += m * float(s[misma_hora[bi]].sum())
        den[h, IDX_HORA] += m * conteo_h[bi]
        n_int += m
        con_intento[bi] = True
    for bi in np.flatnonzero(con_intento):
        n_pares[:NT] += conteo_tr[bi]
        n_pares[IDX_HORA] += conteo_h[bi]
    return dict(num=num, den=den, n_pares=n_pares, n_int=n_int, horas=H, hora_ts=horas)


def curva(res):
    num, den = res["num"].sum(0), res["den"].sum(0)
    return np.divide(num, den, out=np.full(num.shape, np.nan), where=den > 0)


def primer_tramo(n_pares, minimo=MIN_PARES):
    """Primer tramo listado (>= 2-5m) con al menos `minimo` pares, o None."""
    for i in range(2, NT):
        if n_pares[i] >= minimo:
            return i
    return None


def bootstrap(res, ref, b=B_BOOT, rng=None):
    """IC 95% (percentiles) de E_vec (si ref no es None) y E_hora por bootstrap de horas."""
    rng = rng if rng is not None else np.random.default_rng(SEMILLA)
    H = res["horas"]
    pesos = rng.multinomial(H, np.full(H, 1.0 / H), size=b).astype(float)
    num, den = pesos @ res["num"], pesos @ res["den"]
    with np.errstate(invalid="ignore", divide="ignore"):
        c = num / den
    out = {}

    def ic(x):
        x = x[~np.isnan(x)]
        return (float(np.percentile(x, 2.5)), float(np.percentile(x, 97.5))) if len(x) else None

    out["E_hora"] = ic(c[:, 0] - c[:, IDX_HORA])
    out["E_vec"] = ic(c[:, 0] - c[:, ref]) if ref is not None else None
    out["C"] = [ic(c[:, i]) for i in range(NT + 1)]
    return out


def leer_primario(path):
    """{estrato: E} de la tabla de `contend` (columna E antes de su IC)."""
    out = {}
    if not os.path.isfile(path):
        return out
    with open(path) as fh:
        lineas = fh.readlines()
    for linea in lineas:
        m = re.match(r"^(S\d+)\s+\d+\s+\d+\s+\d+\s+[\d.]+\s+([\d.]+)\s+\[[^\]]*\]\s+([\d.]+)"
                     r"\s+\[[^\]]*\]\s+([-+]?[\d.]+)\s+\[", linea)
        if m:
            out[m.group(1)] = float(m.group(4))
    return out


def _f(x, fmt=".3f"):
    return "n/a" if x is None or (isinstance(x, float) and np.isnan(x)) else format(x, fmt)


def _ic(x):
    return "n/a" if x is None else f"[{x[0]:+.3f},{x[1]:+.3f}]"


def main():
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--rows", default="artefactos/exploratorio/rows.csv")
    p.add_argument("--blocks", default="datos/muestra_bloques_v1.csv")
    p.add_argument("--raw", default="artefactos/rpc/muestra_v1")
    p.add_argument("--split", choices=["exploratory", "confirmatory"], default="exploratory")
    p.add_argument("--primario", default="artefactos/confirmatorio/contend_primario.txt")
    a = p.parse_args()
    primario = leer_primario(a.primario)
    print(f"# semilla={SEMILLA} B={B_BOOT} split={a.split} min_pares={MIN_PARES} "
          "(--raw no se usa: solo rows y bloques)")
    for si, e in enumerate(cc.estratos(a.rows, a.blocks, a.split)):
        txs = [(r.block, e.entidad(r), r.pools, r.fallida) for r in e.filas_bot if r.pools]
        res = construir(e.bloques, txs)
        c = curva(res)
        ref = primer_tramo(res["n_pares"])
        bo = bootstrap(res, ref, rng=np.random.default_rng([SEMILLA, si]))
        print(f"## {e.nombre} intentos={res['n_int']} horas={res['horas']} "
              f"C(0)=CR_w={_f(c[0], '.4f')}")
        print(f"{'tramo':8} {'n_pares':>9} {'C':>7} {'IC95% C':>17}")
        for i in range(NT):
            print(f"{TRAMOS[i][0]:8} {int(res['n_pares'][i]):>9} {_f(c[i]):>7} {_ic(bo['C'][i]):>17}")
        print(f"{'mismaHora':8} {int(res['n_pares'][IDX_HORA]):>9} {_f(c[IDX_HORA]):>7} "
              f"{_ic(bo['C'][IDX_HORA]):>17}")
        ev = "n/a" if ref is None else f"{c[0] - c[ref]:+.4f}"
        eh = c[0] - c[IDX_HORA]
        print(f"E_vec = C(0)-C({'n/a' if ref is None else TRAMOS[ref][0]}) = {ev} {_ic(bo['E_vec'])}")
        print(f"E_hora = C(0)-C(mismaHora) = {eh:+.4f} {_ic(bo['E_hora'])} | "
              f"E contend_primario = {_f(primario.get(e.nombre), '+.4f')}")


if __name__ == "__main__":
    main()
