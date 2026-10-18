"""(a) Conciliacion con Anchuri, Felten y Mamageishvili (arXiv 2610.03922).

Sobre los pares (pool, bloque) tocados por entidades bot, fraccion con exactamente una
entidad. Variantes: todas las tx; solo pools que emitieron un Swap en ese bloque
(receipts); solo reverts; fallidas (probe+revert); y las combinaciones con Swap.
"""
import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402

FILTROS = {
    "todas": lambda r: True,
    "fallidas": lambda r: r.fallida,
    "revert": lambda r: r.kind == "revert",
}


def main():
    a = cc.argumentos(__doc__)
    permitidos = {b for bs in cc.cargar_muestra(a.blocks, a.split).values() for b in bs}
    print(f"{'estrato':7} {'variante':22} {'pares':>8} {'1 entidad':>9} {'>=2':>7} {'share_1':>8}")
    sin_crudo = set()
    for e in cc.estratos(a.rows, a.blocks, a.split):
        swaps = {}
        for b in e.bloques:
            # Bloques sin crudo (descarga fallida) se omiten del todo y se cuentan al final.
            if not cc.tiene_raw(a.raw, b):
                sin_crudo.add(b)
                continue
            swaps[b] = cc.pools_con_swap(cc.leer_raw(a.raw, b, permitidos)[1])
        for nombre, filtro in FILTROS.items():
            ents = defaultdict(set)
            for r in e.filas_bot:
                if r.block in swaps and filtro(r):
                    for p in r.pools:
                        ents[(p, r.block)].add(e.entidad(r))
            for con_swap in (False, True):
                pares = [v for (p, b), v in ents.items() if not con_swap or p in swaps[b]]
                uno = sum(len(v) == 1 for v in pares)
                et = nombre + ("+swap" if con_swap else "")
                print(f"{e.nombre:7} {et:22} {len(pares):>8} {uno:>9} {len(pares) - uno:>7} "
                      f"{cc.fmt_pct(uno, len(pares)):>8}")
    print(f"bloques sin crudo: {len(sin_crudo)} ({', '.join(map(str, sorted(sin_crudo)))})")


main()
