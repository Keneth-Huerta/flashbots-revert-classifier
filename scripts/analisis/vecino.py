"""Verificacion cruzada de `mev-revert-classifier neighbor` (P-2; `descomposicion-exceso.md`
Prop. 3, `contencion-nulos-secundarios.md` 4.3, ADR 004).

Lee un `rows.csv` y una muestra con columnas `role` (anchor | n1 | n15 | n150) y `anchor`
(la salida de `sample --neighbors`). Para un intento fallido f de la entidad v en un bloque
ancla b, s(f, b') = sum_{p en S_b'(f)} w_p / sum_{p en P(f)} w_p, con S_b'(f) los pools de P(f)
que toco alguna tx de bot de una entidad distinta de v en b'. Universo de bots, entidades y
pesos w_p = 1/n_p (n_p = paquetes entidad-bloque que tocan p) se calculan SOLO sobre los
bloques ancla, con `comun_contencion` (= `contend`); los vecinos solo aportan S_b'.

C(0) = media de s(f, b) sobre los intentos de todas las anclas; C(k) = media de s(f, b + k)
sobre los intentos de las anclas con vecino de offset k (n1 = 1, n15 = 15, n150 = 150);
U_k = C0|k - C(k) con C0|k = media de s(f, b) sobre esas mismas anclas. Un bloque vecino sin
filas cuenta como vacio (s = 0). Una muestra sin columnas de rol (salida de `sample` normal)
da solo C(0). Solo estimadores puntuales: sin bootstrap. Salida: una linea por estrato y
offset con `.10f`, para compararla con la de Rust.
"""
import argparse
import csv
import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402

# Distancia de reloj de pared de cada rol (segundos). En bloques: segundos // BLOCK_TIME de
# la cadena (Base y OP +1/+15/+150; Unichain +2/+30/+300).
SEGUNDOS = {"n1": 2, "n15": 30, "n150": 300}
OFFSETS = {k: v // cc.BLOCK_TIME for k, v in SEGUNDOS.items()}  # Base


def offsets_de(cadena):
    dt = cc.CADENAS[cadena][1]
    if any(v % dt for v in SEGUNDOS.values()):
        raise ValueError(f"la distancia no es multiplo del bloque de {cadena}")
    return {k: v // dt for k, v in SEGUNDOS.items()}


def leer_diseno(path, split, offsets=None):
    """{estrato: (anclas, {(ancla, rol): bloque})} del split pedido (por ancla)."""
    offsets = OFFSETS if offsets is None else offsets
    quiero = {"exploratory": "true", "confirmatory": "false"}.get(split)
    anclas, vecinos = defaultdict(list), defaultdict(dict)
    with open(path, newline="") as f:
        for r in csv.DictReader(f):
            if quiero is not None and r["exploratory"].strip().lower() != quiero:
                continue
            rol = r.get("role") or "anchor"
            if rol == "anchor":
                anclas[r["stratum"]].append(int(r["block"]))
            else:
                a = int(r["anchor"])
                if int(r["block"]) != a + offsets[rol]:
                    raise ValueError(f"bloque {r['block']} no es ancla {a} + {offsets[rol]}")
                vecinos[r["stratum"]][(a, rol)] = int(r["block"])
    return {k: (sorted(set(v)), vecinos[k]) for k, v in sorted(anclas.items())}


def s_de(pools, v, indice, w):
    """s(f, b') con `indice` = {pool: {entidades que lo tocan en b'}}; None si pesos suman 0."""
    total = sum(w.get(p, 0.0) for p in pools)
    if total <= 0:
        return None
    return sum(w.get(p, 0.0) for p in pools if indice.get(p, set()) - {v}) / total


def indice_bloque(filas, entidad_de):
    idx = defaultdict(set)
    for r in filas:
        if r.to in entidad_de:
            for p in r.pools:
                idx[p].add(entidad_de[r.to])
    return idx


def estrato(nombre, anclas, vecinos, filas_todas, filtro=None):
    e = cc.Estrato(nombre, anclas, filas_todas, filtro)
    paquetes = defaultdict(set)
    for r in e.filas_bot:
        if r.pools:
            paquetes[(r.block, e.entidad(r))].update(r.pools)
    n = defaultdict(int)
    for ps in paquetes.values():
        for p in ps:
            n[p] += 1
    w = {p: 1.0 / v for p, v in n.items()}
    por_bloque = defaultdict(list)
    for r in filas_todas:
        por_bloque[r.block].append(r)
    res = {"intentos": 0, "s0": 0.0}
    acum = {rol: {"anclas": 0, "pares": 0, "s0": 0.0, "sk": 0.0} for rol in OFFSETS}
    for b in anclas:
        idx0 = indice_bloque([r for r in por_bloque[b]], e.entidad_de)
        fallidos = [(e.entidad_de[r.to], r.pools) for r in por_bloque[b]
                    if r.fallida and r.pools and r.to in e.entidad_de]
        s0 = [s_de(ps, v, idx0, w) for v, ps in fallidos]
        for s in s0:
            if s is not None:
                res["s0"] += s
                res["intentos"] += 1
        for rol in OFFSETS:
            nb = vecinos.get((b, rol))
            if nb is None:
                continue
            acum[rol]["anclas"] += 1
            idxk = indice_bloque(por_bloque.get(nb, []), e.entidad_de)
            for (v, ps), a in zip(fallidos, s0):
                sk = s_de(ps, v, idxk, w)
                if a is None or sk is None:
                    continue
                acum[rol]["pares"] += 1
                acum[rol]["s0"] += a
                acum[rol]["sk"] += sk
    return res, acum


def main():
    op = cc.opciones_cadena()  # --chain, --exclude-system[=BOOL], --system-list
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--rows", required=True)
    p.add_argument("--blocks", required=True)
    p.add_argument("--split", choices=["exploratory", "confirmatory", "all"], default="exploratory")
    a = p.parse_args()
    diseno = leer_diseno(a.blocks, a.split, offsets_de(op["cadena"]))
    todos = set()
    for anclas, vecinos in diseno.values():
        todos.update(anclas)
        todos.update(vecinos.values())
    filas = cc.leer_filas(a.rows, todos)
    tot = {"intentos": 0, "s0": 0.0}
    tot_k = {rol: {"anclas": 0, "pares": 0, "s0": 0.0, "sk": 0.0} for rol in OFFSETS}
    for nombre, (anclas, vecinos) in diseno.items():
        res, acum = estrato(nombre, anclas, vecinos, filas, op["filtro"])
        print(linea(nombre, res, acum))
        tot["intentos"] += res["intentos"]
        tot["s0"] += res["s0"]
        for rol in OFFSETS:
            for k in tot_k[rol]:
                tot_k[rol][k] += acum[rol][k]
    print(linea("POOL", tot, tot_k))


def linea(nombre, res, acum):
    c0 = res["s0"] / res["intentos"] if res["intentos"] else float("nan")
    out = [f"{nombre} intentos={res['intentos']} C0={c0:.10f}"]
    for rol, d in acum.items():
        if d["anclas"] == 0:
            continue
        n = d["pares"]
        m0 = d["s0"] / n if n else float("nan")
        ck = d["sk"] / n if n else float("nan")
        out.append(f"{nombre} {rol} anclas={d['anclas']} pares={n} C0k={m0:.10f} "
                   f"Ck={ck:.10f} U={m0 - ck:.10f}")
    return "\n".join(out)


if __name__ == "__main__":
    main()
