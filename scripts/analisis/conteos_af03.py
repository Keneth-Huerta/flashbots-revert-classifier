#!/usr/bin/env python3
"""Conteos descriptivos de AF-03 (no deciden): excluidos por regla, reverts en anclas,
contratos dominantes y cobertura de pools de fallidos. Uso: conteos_af03.py <unichain|op>
Solo lee filas y listas locales."""
import csv, sys, collections
c = sys.argv[1]
D = "artefactos/af03/"
def load_blocks(p):
    with open(p) as f:
        return {int(r["block"]) for r in csv.DictReader(f)}
anc = load_blocks(f"datos/muestra_bloques_af03_{c}_anclas.csv")
allb = load_blocks(f"datos/muestra_bloques_af03_{c}.csv")
sysrem, sysdest = set(), set()
with open("datos/tx_sistema.csv") as f:
    for r in csv.DictReader(f):
        if r["cadena"] == c:
            (sysrem if r["rol"] == "remitente" else sysdest).add(r["direccion"].lower())
def run(rows_path, label):
    tot = collections.Counter(); excl = collections.Counter()
    k_all = collections.Counter(); k_anc = collections.Counter()
    fall_anc = 0; fall_pool = 0; fall_nopool = 0
    rev_to = collections.Counter(); rev_to_all = collections.Counter()
    rev_anc = 0; rev_anc_pool = 0
    nrows = nex = 0
    with open(rows_path) as f:
        for r in csv.DictReader(f):
            nrows += 1
            b = int(r["block"]); a = b in anc
            isx = r["from"].lower() in sysrem or r["to"].lower() in sysdest
            if isx:
                nex += 1
                excl["from="+r["from"].lower() if r["from"].lower() in sysrem else "to="+r["to"].lower()] += 1
                continue
            k = r["kind"]; k_all[k] += 1
            if a: k_anc[k] += 1
            if a and k in ("probe", "revert"):
                fall_anc += 1
                if r["pools"]: fall_pool += 1
                else: fall_nopool += 1
            if k == "revert":
                rev_to_all[r["to"].lower()] += 1
                if a:
                    rev_to[r["to"].lower()] += 1
                    rev_anc += 1
                    if r["pools"]: rev_anc_pool += 1
    print(f"## {c} {label}: filas {nrows}, excluidas por regla (primera coincidencia) {nex}")
    print("excluidas por entrada:", dict(excl))
    print("kinds (todos los bloques, tras exclusion):", dict(k_all))
    print("kinds (solo anclas, tras exclusion):", dict(k_anc))
    print(f"reverts en anclas tras exclusion: {rev_anc} (con pools: {rev_anc_pool})")
    print(f"fallidos (probe+revert) en anclas tras exclusion: {fall_anc}; con pools {fall_pool} ({fall_pool/max(fall_anc,1):.4f}); sin pools {fall_nopool}")
    print("top 8 destinos de reverts en anclas:", rev_to.most_common(8))
    print("top 8 destinos de reverts en todos los bloques:", rev_to_all.most_common(8), "total", sum(rev_to_all.values()))
rows = D + (f"filas_{c}.csv")
run(rows, "con v4")
run(D + f"noV4/filas_{c}_sinv4.csv", "sin v4")
