"""(h) Conciliacion rehecha con Anchuri, Felten y Mamageishvili (arXiv 2610.03922) (V6).

Atribucion de UN pool por revert: el ultimo pool de P(f) que la traza llama con un selector
de swap (V2 `swap` 0x022c0d9f o V3/CL `swap` 0x128acb08), en orden cronologico de la
traza, antes del revert (nivel "swap"). Si no hay tal llamada, el pool de mayor peso w_p
(= 1/n_p de `contend`) de P(f), con desempate por direccion (nivel "maxw"). Se reportan
ambos niveles. Los probes no se atribuyen ni cuentan.

Unidad = contrato (campo `to`, no entidad). Para cada par (pool, bloque) con algun revert
atribuido se cuentan los contratos DISTINTOS que lo intentaron: los que tienen un revert
atribuido a ese pool en ese bloque mas los que tuvieron una tx exitosa con un log Swap
emitido por ese pool en ese bloque (receipts). Se reporta la fraccion de pares con un solo
contrato (share_1), y la misma cuenta sin exitos ("solo reverts") como contraste. Variante
"entidad": igual pero la unidad es la entidad de `comun_contencion` (union por `from`).
Solo cuentan txs de contratos del universo de bots del estrato; bloques sin crudo se omiten.

En que difiere aun de Anchuri et al. (no se afirma equivalencia):
  - universo: nuestros bots son los contratos con >=5 fallidas con pools y >=50% de sus tx
    fallidas, por semana-estrato; el criterio de ellos para decidir que es un bot de MEV
    puede ser otro;
  - muestra: 4 semanas-estrato de horas muestreadas (bloques sorteados), no la cadena
    completa, de modo que un pool-bloque aqui solo ve a los contratos del universo;
  - atribucion de pool: heuristica de trazas con dos niveles; un revert que nunca llego a
    llamar un swap queda atribuido por peso, no por evidencia;
  - "intento": aqui solo reverts atribuidos y exitos con Swap; no se incluyen probes ni
    tx no bot, y Uniswap V4/Balancer (PoolManager/Vault) quedan fuera del selector;
  - el mapa de entidades es el nuestro (union por remitente) y puede diferir del suyo.
"""
import argparse
import os
import sys
from collections import defaultdict
from multiprocessing import Pool

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402
import orden_swap as osw  # noqa: E402

SEL_SWAP = frozenset({"0x022c0d9f", "0x128acb08"})


def ultimo_swap(raiz, pools):
    """Ultimo pool de `pools` llamado con selector de swap en la traza (preorden), o None.

    raiz: nodo `result` de la traza de la tx (la llamada raiz no cuenta)."""
    pools = {p.lower() for p in pools}
    ultimo = None
    pila = list(reversed(raiz.get("calls") or []))
    while pila:
        n = pila.pop()
        to = (n.get("to") or "").lower()
        if to in pools and (n.get("input") or "")[:10].lower() in SEL_SWAP:
            ultimo = to
        pila.extend(reversed(n.get("calls") or []))
    return ultimo


def atribuir(pools, w, ultimo):
    """(pool, nivel): el ultimo pool con swap si existe, si no el de mayor w."""
    if ultimo is not None and ultimo in pools:
        return ultimo, "swap"
    return max(sorted(pools), key=lambda p: w.get(p, 0.0)), "maxw"


def contar(intentos):
    """intentos: iterable de (pool, bloque, unidad, es_exito). Devuelve
    (n_pares, n_uno, n_uno_solo_reverts) sobre pares con algun revert."""
    todos, solo_rev, hay_rev = defaultdict(set), defaultdict(set), set()
    for p, b, u, exito in intentos:
        todos[(p, b)].add(u)
        if not exito:
            solo_rev[(p, b)].add(u)
            hay_rev.add((p, b))
    n = len(hay_rev)
    return (n, sum(len(todos[k]) == 1 for k in hay_rev),
            sum(len(solo_rev[k]) == 1 for k in hay_rev))


def _bloque(args):
    """Por bloque: {tx_hash: ultimo_swap_o_None} de reverts pedidos y swaps por tx."""
    raw, b, permitidos, reverts = args
    trazas, receipts = cc.leer_raw(raw, b, permitidos)
    sw = osw.swaps_por_tx(receipts)
    ult = {}
    for t in trazas:
        h = t["txHash"].lower()
        if h in reverts:
            ult[h] = ultimo_swap(t["result"], reverts[h])
    return b, ult, sw


def main():
    a = cc.argumentos(__doc__)
    permitidos = {b for bs in cc.cargar_muestra(a.blocks, a.split).values() for b in bs}
    print(f"split={a.split}")
    print(f"{'estrato':7} {'unidad':8} {'pares':>8} {'1 unidad':>9} {'share_1':>8} "
          f"{'solo_rev':>9} | reverts {'swap':>6} {'maxw':>6}")
    sin_crudo = set()
    with Pool() as pool:
        for e in cc.estratos(a.rows, a.blocks, a.split):
            con_pools = [r for r in e.filas_bot if r.pools]
            w = osw.pesos_semana(con_pools, e.entidad)
            hay = {b for b in e.bloques if cc.tiene_raw(a.raw, b)}
            sin_crudo |= set(e.bloques) - hay
            por_b = defaultdict(dict)
            for r in con_pools:
                if r.kind == "revert" and r.block in hay:
                    por_b[r.block][r.tx_hash] = r.pools
            tareas = [(a.raw, b, permitidos, por_b.get(b, {})) for b in sorted(hay)]
            ult, sw = {}, {}
            for _, u, s in pool.imap_unordered(_bloque, tareas, chunksize=4):
                ult.update(u)
                sw.update(s)
            niveles = {"swap": 0, "maxw": 0}
            att = []  # (pool, bloque, contrato, entidad)
            for r in con_pools:
                if r.block in hay and r.kind == "revert":
                    p, nivel = atribuir(r.pools, w, ult.get(r.tx_hash))
                    niveles[nivel] += 1
                    att.append((p, r.block, r.to, e.entidad(r), False))
            for r in con_pools:
                if r.block in hay and r.kind == "success":
                    for p in sw.get(r.tx_hash, ()):
                        att.append((p, r.block, r.to, e.entidad(r), True))
            for nombre, k in (("contrato", 2), ("entidad", 3)):
                n, uno, uno_rev = contar((x[0], x[1], x[k], x[4]) for x in att)
                extra = (f"| {niveles['swap']:>14} {niveles['maxw']:>6}" if k == 2 else "")
                print(f"{e.nombre:7} {nombre:8} {n:>8} {uno:>9} {cc.fmt_pct(uno, n):>8} "
                      f"{cc.fmt_pct(uno_rev, n):>9} {extra}")
    print(f"bloques sin crudo: {len(sin_crudo)} ({', '.join(map(str, sorted(sin_crudo)))})")


if __name__ == "__main__":
    main()
