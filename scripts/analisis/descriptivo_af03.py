"""Descriptivo de AF-03 (no decide): por semana, cuantos MH > 0, y por que casi no hay celdas con
ambos grupos. Reusa universo, entidades y X de `carrera_tasa.py` (misma regla). Uso:
descriptivo_af03.py <unichain|op>. Solo lee archivos locales."""
import os
import re
import sys
from collections import Counter, defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402
import orden_swap as osw  # noqa: E402
import carrera_tasa as ct  # noqa: E402

c = sys.argv[1]
D = "artefactos/af03/"
pos = neg = cero = na = 0
semanas = []
for ln in open(D + f"race_primario_{c}.txt"):
    m = re.match(r"^([UO]\d\d)\s+sin_probes .*MH\(ent,dec\)=(\S+)", ln)
    if m:
        v = m.group(2)
        if v == "n/a":
            na += 1
        else:
            x = float(v)
            semanas.append((m.group(1), x))
            pos += x > 0
            neg += x < 0
            cero += x == 0
print(f"{c}: semanas con MH medible {len(semanas)}; >0: {pos}; <0: {neg}; =0: {cero}; n/a: {na}")
print("  ", semanas)

sys.argv = ["x", "--rows", D + f"filas_{c}.csv", "--raw", f"artefactos/rpc_af03/{c}",
            "--blocks", f"datos/muestra_bloques_af03_{c}_anclas.csv", "--split", "confirmatory",
            "--chain", c]
op = cc.opciones_cadena()
a = cc.argumentos("x")
permitidos = {b for bs in cc.cargar_muestra(a.blocks, a.split).values() for b in bs}
tot = Counter()
ent_n = Counter()
for si, e in enumerate(cc.estratos(a.rows, a.blocks, a.split, op["filtro"], op["max_grado"])):
    tam = defaultdict(int)
    for r in e.filas:
        tam[r.block] = max(tam[r.block], r.tx_index + 1)
    con_pools = [r for r in e.filas_bot if r.pools]
    sw = {}
    for b in e.bloques:
        if os.path.isfile(os.path.join(a.raw, f"{b}.receipts.json.gz")):
            sw.update(osw.swaps_por_tx(cc._leer_gz(a.raw, b, "receipts", permitidos)))
    txs = [(r.block, r.tx_index, e.entidad(r), r.pools, r.kind,
            sw.get(r.tx_hash, frozenset()) if r.kind == "success" else frozenset()) for r in con_pools]
    pb = defaultdict(list)
    for t in txs:
        if t[4] in ("success", "revert"):
            pb[t[0]].append(t)
    ents_semana = set()
    for b, lista in pb.items():
        lista.sort(key=lambda t: t[1])
        xs = ct.x_por_bloque([(t[1], t[2], t[3], t[4], t[5]) for t in lista])
        ents = {t[2] for t in lista}
        multi = len(ents) >= 2
        for t, x in zip(lista, xs):
            tot["n"] += 1
            tot["revert"] += t[4] == "revert"
            tot["x1"] += x
            tot["x1_revert"] += x and t[4] == "revert"
            tot["en_bloque_multientidad"] += multi
            ent_n[t[2]] += 1
            ents_semana.add(t[2])
        tot["bloques"] += 1
        tot["bloques_multientidad"] += multi
    tot["entidades"] += len(ents_semana)
tt = tot["n"]
print(f"  universo sin_probes (bot, pools, success|revert): n={tt}; reverts={tot['revert']} ({tot['revert']/tt:.4f}); X=1: {tot['x1']} ({tot['x1']/tt:.4f}); X=1 y revert: {tot['x1_revert']}")
print(f"  bloques ancla con tx del universo: {tot['bloques']}; con >=2 entidades: {tot['bloques_multientidad']} ({tot['bloques_multientidad']/max(tot['bloques'],1):.4f}); tx en bloques con >=2 entidades: {tot['en_bloque_multientidad']} ({tot['en_bloque_multientidad']/tt:.4f})")
print(f"  entidades-semana: {tot['entidades']}")

# Concentracion por contrato destino (descriptivo): reverts del universo sin_probes por destino
if c == "unichain":
    rev_dest = Counter()
    for e in cc.estratos(a.rows, a.blocks, a.split, op["filtro"], op["max_grado"]):
        for r in e.filas_bot:
            if r.pools and r.kind == "revert":
                rev_dest[r.to.lower()] += 1
    tr = sum(rev_dest.values())
    print(f"  reverts del universo por destino (top 5, total {tr}):", [(k, v, round(v / tr, 4)) for k, v in rev_dest.most_common(5)])
