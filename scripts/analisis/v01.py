"""(f) Heuristica de la v0.1.0: densidad por contrato y bloque (C_min = 3).

Agrupa los intentos fallidos (probe+revert) de contratos del universo de bots por
(bloque, contrato) y reporta la fraccion de intentos en grupos de tamano >= C_min.
Se da tambien por (bloque, entidad) como referencia. Mide repeticion de un bot,
no competencia entre bots.
"""
import os
import sys
from collections import Counter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402

C_MIN = 3


def main():
    a = cc.argumentos(__doc__)
    print(f"{'estrato':7} {'fallidas':>8} {'grupos':>7} {'en_grupos>=3':>12} {'share':>7} "
          f"{'share_entidad':>13}")
    for e in cc.estratos(a.rows, a.blocks, a.split):
        fal = [r for r in e.filas_bot if r.fallida]
        g = Counter((r.block, r.to) for r in fal)
        ge = Counter((r.block, e.entidad(r)) for r in fal)
        en = sum(n for n in g.values() if n >= C_MIN)
        ene = sum(n for n in ge.values() if n >= C_MIN)
        print(f"{e.nombre:7} {len(fal):>8} {len(g):>7} {en:>12} {cc.fmt_pct(en, len(fal)):>7} "
              f"{cc.fmt_pct(ene, len(fal)):>13}")


main()
