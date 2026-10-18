"""(b) Auto-redundancia: calldata repetida entre intentos fallidos de una misma entidad.

Para cada (entidad, bloque) con >=2 intentos fallidos (con pools), fraccion de intentos
cuyo calldata raiz (traza) es identico a otro intento fallido del grupo; y fraccion cuyo
selector de 4 bytes + primeros 64 bytes coinciden con los de otro intento.
"""
import os
import sys
from collections import Counter, defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402

PREFIJO = 2 + 8 + 128  # '0x' + selector + 64 bytes, en caracteres hex


def main():
    a = cc.argumentos(__doc__)
    permitidos = {b for bs in cc.cargar_muestra(a.blocks, a.split).values() for b in bs}
    print(f"{'estrato':7} {'grupos>=2':>9} {'intentos':>8} {'exacto':>8} {'share':>7} "
          f"{'prefijo':>8} {'share':>7}")
    sin_crudo = set()
    for e in cc.estratos(a.rows, a.blocks, a.split):
        grupos = defaultdict(list)
        for r in e.filas_bot:
            if r.fallida and r.pools:
                grupos[(e.entidad(r), r.block)].append(r.tx_hash)
        por_bloque = defaultdict(list)
        for (ent, b), hs in grupos.items():
            if len(hs) >= 2:
                por_bloque[b].append(hs)
        n = ex = pre = ng = 0
        for b, lista in por_bloque.items():
            # Bloques sin crudo (descarga fallida) se omiten y se cuentan al final.
            if not cc.tiene_raw(a.raw, b):
                sin_crudo.add(b)
                continue
            inputs = cc.inputs_raiz(cc.leer_raw(a.raw, b, permitidos)[0])
            for hs in lista:
                ng += 1
                datos = [inputs[h].lower() for h in hs]
                c_ex = Counter(datos)
                c_pre = Counter(d[:PREFIJO] for d in datos)
                n += len(datos)
                ex += sum(c_ex[d] > 1 for d in datos)
                pre += sum(c_pre[d[:PREFIJO]] > 1 for d in datos)
        print(f"{e.nombre:7} {ng:>9} {n:>8} {ex:>8} {cc.fmt_pct(ex, n):>7} "
              f"{pre:>8} {cc.fmt_pct(pre, n):>7}")
    print(f"bloques sin crudo: {len(sin_crudo)} ({', '.join(map(str, sorted(sin_crudo)))})")


main()
