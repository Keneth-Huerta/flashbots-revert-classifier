"""(c) Contencion por ventana: CR binario dentro de rebanadas de tx_index vs. dentro del bloque.

Cada bloque se parte en 10 rebanadas de igual numero de tx (todas las tx del bloque, por
orden de tx_index) como proxy de Flashblock. CR = fallidas con algun pool compartido con
el paquete de otra entidad en la misma ventana / fallidas con pools. Solo observado.
Las entidades y fallidas son las mismas en ambas ventanas; el paquete de una entidad en
una ventana une los pools de todas sus tx (cualquier tipo) en esa ventana.
"""
import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402

K = 10


def cr_ventanas(e, clave):
    """(contendidas, fallidas) con ventana = clave(fila)."""
    toca = defaultdict(lambda: defaultdict(set))  # ventana -> pool -> entidades
    for r in e.filas_bot:
        for p in r.pools:
            toca[clave(r)][p].add(e.entidad(r))
    hit = tot = 0
    for r in e.filas_bot:
        if r.fallida and r.pools:
            tot += 1
            hit += any(len(toca[clave(r)][p]) >= 2 for p in r.pools)
    return hit, tot


def main():
    a = cc.argumentos(__doc__)
    print(f"{'estrato':7} {'fallidas':>8} {'CR_bloque':>9} {'CR_rebanada':>11} {'diferencia':>10}")
    for e in cc.estratos(a.rows, a.blocks, a.split):
        orden = defaultdict(list)
        for r in e.filas:
            orden[r.block].append(r.tx_index)
        rebanada = {}
        for b, idx in orden.items():
            idx.sort()
            for pos, t in enumerate(idx):
                rebanada[(b, t)] = pos * K // len(idx)
        hb, n = cr_ventanas(e, lambda r: r.block)
        hs, n2 = cr_ventanas(e, lambda r: (r.block, rebanada[(r.block, r.tx_index)]))
        assert n == n2
        print(f"{e.nombre:7} {n:>8} {hb / n:>9.4f} {hs / n:>11.4f} {(hs - hb) / n:>10.4f}")


main()
