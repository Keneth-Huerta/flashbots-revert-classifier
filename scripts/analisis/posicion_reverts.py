"""Posicion en el bloque de los reverts: ¿confusor (diseno) o mediador (carrera)?
(`descomposicion-exceso.md` 4.4; P4).

La prueba de orden en reverts casi desaparece al permutar solo dentro de la misma posicion
relativa del bloque. Posicion relativa q = tx_index / (max_tx_index_del_bloque + 1), con el
tamano de bloque de todas las filas de rows.csv. Sobre las tx de bots con pools (universo y
entidades de `comun_contencion`) de bloques con crudo, por estrato:
  A  reverts con S_prev vacio (ningun pool propio con Swap exitoso previo de OTRA entidad
     en el bloque; mismo S_prev de `orden_swap.py`, via `swaps_por_tx`);
  B  reverts con S_prev no vacio;  C  probes;  D  exitos;  E  todas las tx.
Se reporta media y cuartiles de q, y la diferencia de medias A - E (tambien B - E) con IC 95%
por bootstrap de horas (B=1000, semilla 20261017) sobre sumas por hora.
Prediccion: si la posicion es DISENO (confusor), A tardia como B (A >> E); si es MEDIADOR
(carrera), A no tardia (A ~ E) y B tardia. Lo mismo para las 5 entidades con mas reverts de
cada estrato. Descriptivo: no decide por si solo (la llegada y el fee tambien fijan q).
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
GRUPOS = ("A", "B", "C", "D", "E")
TOP_ENT = 5


def grupos_de_bloque(lista):
    """lista: tx de bots con pools de UN bloque, (tx_index, entidad, pools, kind, swappools)
    ordenadas por tx_index. Devuelve [(entidad, grupos)] con grupos subconjunto de 'ABCDE'
    por tx (siempre 'E'; 'A'/'B' reverts segun S_prev; 'C' probe; 'D' exito)."""
    exitos = defaultdict(list)  # pool -> [(tx_index, entidad)]
    for t in lista:
        if t[3] == "success":
            for p in t[4]:
                exitos[p].append((t[0], t[1]))
    out = []
    for idx, ent, pools, kind, _ in lista:
        g = "E"
        if kind == "revert":
            previo = any(i < idx and e != ent for p in pools for i, e in exitos.get(p, ()))
            g += "B" if previo else "A"
        elif kind == "probe":
            g += "C"
        elif kind == "success":
            g += "D"
        out.append((ent, g))
    return out


def acumular(txs, tam):
    """txs: (block, tx_index, entidad, pools, kind, swappools). Devuelve
    {clave: {grupo: {hora: [n, suma_q, ...]}}} con clave None (estrato) o entidad, y la
    lista de q por grupo."""
    por_bloque = defaultdict(list)
    for t in txs:
        por_bloque[t[0]].append(t)
    hora = {}
    q = defaultdict(lambda: defaultdict(list))  # clave -> grupo -> [(hora, q)]
    for b in sorted(por_bloque):
        h = hora.setdefault(cc.block_timestamp(b) // 3600, len(hora))
        lista = sorted(por_bloque[b], key=lambda t: t[1])
        reducida = [(t[1], t[2], t[3], t[4], t[5]) for t in lista]
        for t, (ent, gs) in zip(lista, grupos_de_bloque(reducida)):
            v = t[1] / tam[b]
            for g in gs:
                q[None][g].append((h, v))
                q[ent][g].append((h, v))
    return q, len(hora)


def por_hora(pares, H):
    """(H, 2): n y suma de q por hora."""
    m = np.zeros((H, 2))
    for h, v in pares:
        m[h, 0] += 1
        m[h, 1] += v
    return m


def diff_boot(ma, me, rng, b=B_BOOT):
    """Diferencia de medias (A - E) e IC 95% por bootstrap de horas. Devuelve
    (dif, (lo, hi)) o (None, None) si falta algun grupo."""
    if ma[:, 0].sum() == 0 or me[:, 0].sum() == 0:
        return None, None
    H = len(ma)
    pesos = rng.multinomial(H, np.full(H, 1.0 / H), size=b).astype(float)
    na, sa, ne, se = pesos @ ma[:, 0], pesos @ ma[:, 1], pesos @ me[:, 0], pesos @ me[:, 1]
    ok = (na > 0) & (ne > 0)
    d = sa[ok] / na[ok] - se[ok] / ne[ok]
    dif = ma[:, 1].sum() / ma[:, 0].sum() - me[:, 1].sum() / me[:, 0].sum()
    return float(dif), (float(np.percentile(d, 2.5)), float(np.percentile(d, 97.5)))


def resumen(pares):
    v = np.array([x[1] for x in pares])
    if len(v) == 0:
        return None
    return len(v), float(v.mean()), [float(x) for x in np.percentile(v, [25, 50, 75])]


def _ic(x):
    return "n/a" if x is None else f"[{x[0]:+.3f},{x[1]:+.3f}]"


def _d(x):
    return "n/a" if x is None else f"{x:+.3f}"


def fila_diffs(q, H, rng):
    me = por_hora(q["E"], H)
    out = []
    for g in ("A", "B"):
        out.append(diff_boot(por_hora(q[g], H), me, rng))
    return out


def main():
    a = cc.argumentos(__doc__)
    permitidos = {b for bs in cc.cargar_muestra(a.blocks, a.split).values() for b in bs}
    print(f"# semilla={SEMILLA} B={B_BOOT} split={a.split}; q = tx_index/(max_tx_index+1)")
    print(f"{'est':4} {'grp':3} {'n':>8} {'media':>6} {'Q1':>6} {'med':>6} {'Q3':>6}")
    sin_crudo = set()
    detalle = []
    for si, e in enumerate(cc.estratos(a.rows, a.blocks, a.split)):
        tam = defaultdict(int)
        for r in e.filas:
            tam[r.block] = max(tam[r.block], r.tx_index + 1)
        con_pools = [r for r in e.filas_bot if r.pools]
        sw = {}
        for b in e.bloques:
            if not cc.tiene_raw(a.raw, b):
                sin_crudo.add(b)
                continue
            sw.update(osw.swaps_por_tx(cc.leer_raw(a.raw, b, permitidos)[1]))
        txs = [(r.block, r.tx_index, e.entidad(r), r.pools, r.kind,
                sw.get(r.tx_hash, frozenset()) if r.kind == "success" else frozenset())
               for r in con_pools if r.block not in sin_crudo]
        q, H = acumular(txs, tam)
        for g in GRUPOS:
            s = resumen(q[None][g])
            if s:
                print(f"{e.nombre:4} {g:3} {s[0]:>8} {s[1]:>6.3f} "
                      f"{s[2][0]:>6.3f} {s[2][1]:>6.3f} {s[2][2]:>6.3f}")
        rng = np.random.default_rng([SEMILLA, si])
        (da, ica), (db, icb) = fila_diffs(q[None], H, rng)
        detalle.append(f"{e.nombre:4} A-E {_d(da):>7} {_ic(ica):>17}   B-E {_d(db):>7} {_ic(icb):>17}")
        top = sorted((k for k in q if k is not None),
                     key=lambda k: -len(q[k]["A"]) - len(q[k]["B"]))[:TOP_ENT]
        for k in top:
            sa, sb, se_ = resumen(q[k]["A"]), resumen(q[k]["B"]), resumen(q[k]["E"])
            (dka, icka), _ = fila_diffs(q[k], H, np.random.default_rng([SEMILLA, si, 1]))
            fm = lambda s: "n/a" if s is None else f"{s[1]:.3f}"  # noqa: E731
            fn = lambda s: 0 if s is None else s[0]  # noqa: E731
            detalle.append(f"  ent {k[:10]} {e.nombre:4} nA={fn(sa):>5} nB={fn(sb):>5} nE={fn(se_):>6} "
                           f"mediaA={fm(sa)} mediaB={fm(sb)} mediaE={fm(se_)} A-E {_d(dka)} {_ic(icka)}")
    print("## diferencias de medias (IC 95% bootstrap de horas) y top-5 entidades por reverts")
    print("\n".join(detalle))
    print(f"bloques sin crudo: {len(sin_crudo)}")


if __name__ == "__main__":
    main()
