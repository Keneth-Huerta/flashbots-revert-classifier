"""Compara bots/entidades por estrato de comun_contencion con la salida de `contend`.

Ejecuta el binario ya compilado (sin recompilar) y exige igualdad exacta de `bots`.
Uso: python3 -I scripts/analisis/check_contencion.py [--split exploratory]
"""
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402

BIN = "target/release/mev-revert-classifier"


def main():
    a = cc.argumentos(__doc__)
    out = subprocess.run(
        [BIN, "contend", "--rows", a.rows, "--blocks", a.blocks, "--split", a.split,
         "--permutations", "1", "--bootstrap", "1"],
        capture_output=True, text=True,
    )
    texto = out.stdout
    if out.returncode != 0:  # por si las opciones de velocidad no existen
        out = subprocess.run(
            [BIN, "contend", "--rows", a.rows, "--blocks", a.blocks, "--split", a.split],
            capture_output=True, text=True, check=True)
        texto = out.stdout
    rust = {}
    for linea in texto.splitlines():
        m = re.match(r"(S\d)\s+(\d+)\s+(\d+)\s+(\d+)\s", linea)
        if m:
            rust[m[1]] = tuple(int(m[i]) for i in (2, 3, 4))
    ok = True
    print(f"{'estrato':8} {'bloques':>7} {'contratos':>9} {'bots(ent)':>9} {'rust_bots':>9} "
          f"{'fallidas_bots':>13} {'fallidas_pools':>14} {'rust_failed':>11}  ok")
    for e in cc.estratos(a.rows, a.blocks, a.split):
        fb = sum(r.fallida for r in e.filas_bot)
        fp = sum(r.fallida and bool(r.pools) for r in e.filas_bot)
        rb, rbots, rf = rust[e.nombre]
        coincide = e.n_entidades == rbots and len(e.bloques) == rb and rf in (fb, fp)
        ok &= coincide
        print(f"{e.nombre:8} {len(e.bloques):>7} {len(e.bots):>9} {e.n_entidades:>9} {rbots:>9} "
              f"{fb:>13} {fp:>14} {rf:>11}  {'SI' if coincide else 'NO'}")
    print("COINCIDE EXACTAMENTE" if ok else "DIFERENCIAS")
    sys.exit(0 if ok else 1)


main()
