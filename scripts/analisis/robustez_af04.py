"""Verificacion cruzada de AF-04 (ADR 006 §1.4, fundamento `carrera-placebos-seleccion.md`).

Reimplementa en Python, con las mismas filas y los mismos receipts, las estimaciones puntuales
y los conteos de las variantes de `race` (exposiciones T1 a T5), la rejilla de faltantes, la
descomposicion de Simpson (terminos exactos) y DerSimonian-Laird. Los intervalos NO se
calculan aqui (generador distinto): solo puntos y conteos, que deben coincidir con Rust.
Reproduccion NO independiente (mismo autor, mismas filas).

  python3 -I scripts/analisis/robustez_af04.py --rows R --blocks B --raw DIR [--split all]
      [--x-def prior_other|later_other|donor_pool|prior_same|user_prior|any_prior]
      [--x-also none|prior_other|user_prior|prior_same|later_other]
      [--p-rule trace|contract|contract_subset|trace_on_contract_subset]
      [--p-size-strata] [--bot-share F] [--donor-seed N] [--donor-subset]
      [--v4-pool-manager ADDR] [--missing-grid] [--decompose] [--comparar SALIDA_RUST]
"""
import argparse
import gzip
import hashlib
import json
import math
import os
import re
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comun_contencion as cc  # noqa: E402

TAU = 0.02
BINS = 10


# ------------------------------------------------------------------ receipts

def swaps_completos(receipts, pool_manager=None):
    """Lista de dicts {idx, hash, from, pools} de tx exitosas con log Swap."""
    out = []
    for pos, rc in enumerate(receipts):
        if str(rc.get("status", "")).lower() not in ("0x1", "0x01", "1"):
            continue
        pools = set()
        for log in rc.get("logs", []):
            topics = log.get("topics") or []
            if topics and topics[0].lower() in cc.SWAP_TOPICS:
                a = log["address"].lower()
                if pool_manager and a == pool_manager:
                    if len(topics) > 1:
                        pools.add("v4:" + topics[1].lower())
                else:
                    pools.add(a)
        if not pools or "transactionHash" not in rc:
            continue
        ti = rc.get("transactionIndex")
        idx = int(ti, 16) if isinstance(ti, str) else pos
        out.append({"idx": idx, "hash": rc["transactionHash"].lower(),
                    "from": str(rc.get("from", "")).lower(), "pools": pools})
    return out


def _gana(wins, pools, f):
    return any(f(i, e) for p in pools for i, e in wins.get(p, ()))


# ------------------------------------------------------------------ construccion

def construir(estrato, receipts, tam, bins=BINS, bot_share_filas=None, donor_seed=1, pm=None):
    """Devuelve (txs, vacias): txs = dicts por tx del universo (P no vacio) en el orden de
    Rust (bloque, tx_index); vacias = tx de bots con P vacio. `receipts`: bloque -> lista."""
    filas = estrato.filas_bot
    bots = set(estrato.entidad_de)
    ent_de = estrato.entidad_de
    # clave de entidad: contrato bot con mas filas (empate: menor direccion)
    cuenta = defaultdict(int)
    for r in filas:
        cuenta[r.to] += 1
    mejor = {}
    for c, n in cuenta.items():
        l = ent_de[c]
        if l not in mejor or n > mejor[l][0] or (n == mejor[l][0] and c < mejor[l][1]):
            mejor[l] = (n, c)
    clave = {l: c for l, (n, c) in mejor.items()}
    remitentes = defaultdict(set)
    for r in filas:
        remitentes[ent_de[r.to]].add(r.from_)
    bot_hashes = {r.tx_hash.lower() for r in filas}
    por_bloque = defaultdict(list)
    for r in filas:
        if r.block in receipts:
            por_bloque[r.block].append(r)
    sw_bloque, conteo = {}, defaultdict(int)
    for b in sorted(por_bloque):
        por_bloque[b].sort(key=lambda r: r.tx_index)
        sw = swaps_completos(receipts[b], pm)
        sw_bloque[b] = sw
        por_hash = {s["hash"]: s for s in sw}
        for r in por_bloque[b]:
            s = por_hash.get(r.tx_hash.lower())
            if r.kind == "success" and s:
                for p in s["pools"]:
                    conteo[(r.to, p)] += 1
    # reglas de pools por contrato-semana: >= 10% de las incidencias, a lo mas 5, empate por direccion
    total = defaultdict(int)
    for (c, p), n in conteo.items():
        total[c] += n
    pc = defaultdict(list)
    for (c, p), n in conteo.items():
        if n >= 0.10 * total[c]:
            pc[c].append((-n, p))
    pc = {c: [p for _, p in sorted(v)[:5]] for c, v in pc.items()}
    txs, vacias = [], []
    for b in sorted(por_bloque):
        lista = por_bloque[b]
        sw = sw_bloque[b]
        por_hash = {s["hash"]: s for s in sw}
        lab = {}
        for r in lista:
            lab.setdefault(ent_de[r.to], len(lab))
        ent = lambda r: lab[ent_de[r.to]]  # noqa: E731
        wins_list, wins_full, wins_default = defaultdict(list), defaultdict(list), defaultdict(list)
        for r in lista:
            s = por_hash.get(r.tx_hash.lower())
            if r.kind == "success" and s:
                for p in s["pools"]:
                    wins_full[p].append((r.tx_index, ent(r)))
                    if r.pools:
                        wins_list[p].append((r.tx_index, ent(r)))
        wins_user = defaultdict(list)
        for s in sw:
            if s["hash"] not in bot_hashes:
                for p in s["pools"]:
                    wins_user[p].append((s["idx"], s["from"]))
        uni = [r for r in lista if r.pools]
        for r in lista:
            e, i = ent(r), r.tx_index
            lab_e = ent_de[r.to]
            xc = None
            if r.to in pc:
                xc = int(_gana(wins_full, pc[r.to], lambda j, ej: j < i and ej != e))
            if not r.pools:
                vacias.append({"ent": lab_e, "key": clave[lab_e], "block": b, "idx": i,
                               "y": int(r.kind == "revert"), "probe": int(r.kind == "probe"),
                               "x_c": xc})
                continue
            pools = list(r.pools)
            x_later = int(_gana(wins_list, pools, lambda j, ej: j > i and ej != e))
            x_same = int(_gana(wins_list, pools, lambda j, ej: j < i and ej == e))
            x_user = int(any(j < i and f not in remitentes[lab_e]
                             for p in pools for j, f in wins_user.get(p, ())))
            mio = set(pools)
            mejor_d = None
            for d in uni:
                if d.kind == "probe" or ent(d) == e or d.tx_hash == r.tx_hash:
                    continue
                dp = list(d.pools)
                if mio & set(dp):
                    continue
                gap = abs(len(dp) - len(pools))
                k = hashlib.sha256(f"{donor_seed}:{b}:{r.tx_hash.lower()}:{d.tx_hash.lower()}".encode()).digest()
                if mejor_d is None or (gap, k) < mejor_d[:2]:
                    mejor_d = (gap, k, d)
            x_donor = None
            if mejor_d:
                x_donor = int(_gana(wins_list, list(mejor_d[2].pools), lambda j, ej: j < i and ej != e))
            q = i / tam[b] * bins
            txs.append({"ent": lab_e, "key": clave[lab_e], "bin": min(int(q), bins - 1), "block": b,
                        "y": int(r.kind == "revert"), "probe": int(r.kind == "probe"),
                        "x": int(_gana(wins_list, pools, lambda j, ej: j < i and ej != e)),
                        "x_later": x_later, "x_same": x_same, "x_user": x_user,
                        "x_donor": x_donor, "psize": min(max(len(pools), 1), 3), "x_c": xc})
    for v in vacias:
        v["bin"] = min(int(v["idx"] / tam[v["block"]] * bins), bins - 1)
    return txs, vacias


# ------------------------------------------------------------------ variantes y MH

def aplicar(txs, vacias, x_def="prior_other", also="none", p_rule="trace", p_size=False,
            donor_subset=False):
    """Lista de (celda_ent, bin, x, y, probe) tras la variante (ent recodificada como tupla)."""
    def expo(t):
        return {"prior_other": t["x"], "later_other": t["x_later"], "donor_pool": t["x_donor"],
                "prior_same": t["x_same"], "user_prior": t["x_user"],
                "any_prior": int(t["x"] or t["x_user"])}[x_def]

    def z_of(t):
        return {"none": 0, "prior_other": t["x"], "user_prior": t["x_user"],
                "prior_same": t["x_same"], "later_other": t["x_later"]}[also]

    out, universo = [], 0
    for t in txs:
        universo += 1 - t["probe"]
        if donor_subset and t["x_donor"] is None:
            continue
        if p_rule == "trace":
            x, z = expo(t), z_of(t)
        elif p_rule in ("contract", "contract_subset"):
            x, z = t["x_c"], 0
        else:
            x, z = (t["x"] if t["x_c"] is not None else None), 0
        if x is None:
            continue
        g = t["psize"] - 1 if p_size else 3
        out.append(((t["ent"], g, z), t["bin"], x, t["y"], t["probe"]))
    if p_rule == "contract":
        for v in vacias:
            if v["x_c"] is not None:
                out.append(((v["ent"], 0 if p_size else 3, 0), v["bin"], v["x_c"], v["y"], v["probe"]))
    return out


def celdas(items, spec, con_probes=False):
    c = defaultdict(lambda: [0.0, 0.0, 0.0, 0.0])
    for ent, b, x, y, probe in items:
        if probe and not con_probes:
            continue
        k = (ent, b) if spec == "cond" else b
        a = 0 if x else 2
        c[k][a] += 1
        c[k][a + 1] += y
    return c


def mh(celdas_):
    num = den = 0.0
    usadas = 0
    for n1, r1, n0, r0 in celdas_.values():
        if n1 > 0 and n0 > 0:
            w = n1 * n0 / (n1 + n0)
            num += w * (r1 / n1 - r0 / n0)
            den += w
            usadas += 1
    return num, den, usadas


def estimar(estratos_items, spec):
    """(MH agrupado, celdas usadas totales) sobre estratos (celdas anidadas)."""
    N = D = 0.0
    K = 0
    for items in estratos_items:
        n, d, k = mh(celdas(items, spec))
        N, D, K = N + n, D + d, K + k
    return (N / D if D > 0 else float("nan")), K


# ------------------------------------------------------------------ rejilla

def rejilla(estratos_txs, estratos_vacias):
    """Estimaciones agrupadas en {0,.1,..,1}^2 (sin probes): fraccion q_r de las vacias que son
    reverts y q_s de las vacias exitos con X = 1, dentro de celdas (entidad, bin)."""
    pts = {}
    for qi in range(11):
        for si in range(11):
            qr, qs = qi / 10, si / 10
            N = D = 0.0
            for items, vac in zip(estratos_txs, estratos_vacias):
                c = defaultdict(lambda: [0.0] * 6)
                for ent, b, x, y, probe in items:
                    if probe:
                        continue
                    a = 0 if x else 2
                    c[(ent[0], b)][a] += 1
                    c[(ent[0], b)][a + 1] += y
                for v in vac:
                    if v["probe"]:
                        continue
                    c[(v["ent"], v["bin"])][4 if v["y"] else 5] += 1
                for k, (n1, r1, n0, r0, mr, ms) in c.items():
                    n1 += qr * mr + qs * ms
                    r1 += qr * mr
                    n0 += (1 - qr) * mr + (1 - qs) * ms
                    r0 += (1 - qr) * mr
                    if n1 > 0 and n0 > 0:
                        w = n1 * n0 / (n1 + n0)
                        N += w * (r1 / n1 - r0 / n0)
                        D += w
            pts[(qr, qs)] = N / D if D > 0 else float("nan")
    return pts


# ------------------------------------------------------------------ descomposicion

def _w(n1, n0):
    return n1 * n0 / (n1 + n0)


def descomponer(estratos_items):
    """Terminos exactos (dec, mh, comp, wgt, sup, gap, residuo) de la descomposicion de Kitagawa."""
    a = [0.0] * 8
    for items in estratos_items:
        c = celdas(items, "cond")
        dec = defaultdict(lambda: [0.0] * 4)
        sdec = defaultdict(list)
        for (ent, b), v in c.items():
            for k in range(4):
                dec[b][k] += v[k]
            if v[0] > 0 and v[2] > 0:
                w = _w(v[0], v[2])
                a[2] += w * (v[1] / v[0] - v[3] / v[2])
                a[3] += w
                sdec[b].append(v)
        for v in dec.values():
            if v[0] > 0 and v[2] > 0:
                w = _w(v[0], v[2])
                a[0] += w * (v[1] / v[0] - v[3] / v[2])
                a[1] += w
        for cs in sdec.values():
            n1 = sum(v[0] for v in cs)
            n0 = sum(v[2] for v in cs)
            r1 = sum(v[1] for v in cs)
            r0 = sum(v[3] for v in cs)
            w = _w(n1, n0)
            a[4] += w * (r1 / n1 - r0 / n0)
            a[5] += w
            within = comp = 0.0
            for v in cs:
                p1, p0 = v[0] / n1, v[2] / n0
                rr1, rr0 = v[1] / v[0], v[3] / v[2]
                within += (p1 + p0) / 2 * (rr1 - rr0)
                comp += (p1 - p0) * (rr1 + rr0) / 2
            a[6] += w * within
            a[7] += w * comp
    dec, mh_, ds = a[0] / a[1], a[2] / a[3], a[4] / a[5]
    w, comp = a[6] / a[5], a[7] / a[5]
    sup, wgt, gap = dec - ds, w - mh_, dec - mh_
    return {"dec": dec, "mh": mh_, "comp": comp, "wgt": wgt, "sup": sup, "gap": gap,
            "resid": abs(gap - (comp + wgt + sup))}


# ------------------------------------------------------------------ heterogeneidad

def _rangos(v):
    idx = sorted(range(len(v)), key=lambda i: v[i])
    r = [0.0] * len(v)
    i = 0
    while i < len(idx):
        j = i
        while j + 1 < len(idx) and v[idx[j + 1]] == v[idx[i]]:
            j += 1
        for q in idx[i:j + 1]:
            r[q] = (i + j) / 2 + 1
        i = j + 1
    return r


def dersimonian_laird(est, indice):
    """est: [(estimacion, varianza)]; devuelve dict o None. Igual que `robust::dersimonian_laird`."""
    d = [(e, v, i) for (e, v), i in zip(est, indice) if v > 0]
    k = len(d)
    if k < 2:
        return None
    s1 = sum(1 / v for _, v, _ in d)
    s2 = sum(1 / v ** 2 for _, v, _ in d)
    fe = sum(e / v for e, v, _ in d) / s1
    q = sum((e - fe) ** 2 / v for e, v, _ in d)
    tau2 = max(0.0, (q - (k - 1)) / (s1 - s2 / s1))
    i2 = max(0.0, (q - (k - 1)) / q) if q > 0 else 0.0
    sw = sum(1 / (v + tau2) for _, v, _ in d)
    re = sum(e / (v + tau2) for e, v, _ in d) / sw
    w = [1 / v for _, v, _ in d]
    xs, ys = [i for _, _, i in d], [e for e, _, _ in d]
    sw1 = sum(w)
    mx = sum(x * wi for x, wi in zip(xs, w)) / sw1
    my = sum(y * wi for y, wi in zip(ys, w)) / sw1
    sxy = sum(wi * (x - mx) * (y - my) for x, y, wi in zip(xs, ys, w))
    sxx = sum(wi * (x - mx) ** 2 for x, wi in zip(xs, w))
    rx, ry = _rangos(xs), _rangos(ys)
    mrx, mry = sum(rx) / k, sum(ry) / k
    cov = sum((a - mrx) * (b - mry) for a, b in zip(rx, ry))
    va = sum((a - mrx) ** 2 for a in rx)
    vb = sum((b - mry) ** 2 for b in ry)
    return {"k": k, "fe": fe, "q": q, "tau2": tau2, "i2": i2, "re": re, "re_se": math.sqrt(1 / sw),
            "slope": sxy / sxx, "spearman": cov / math.sqrt(va * vb) if va * vb > 0 else float("nan")}


# ------------------------------------------------------------------ CLI

def _r(v):
    return "n/a" if math.isnan(v) else f"{v:.4f}"


def _f(v):
    return "n/a" if v is None or (isinstance(v, float) and math.isnan(v)) else f"{v:+.4f}"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--rows", required=True)
    ap.add_argument("--blocks", required=True)
    ap.add_argument("--raw", required=True)
    ap.add_argument("--split", default="all")
    ap.add_argument("--x-def", default="prior_other")
    ap.add_argument("--x-also", default="none")
    ap.add_argument("--p-rule", default="trace")
    ap.add_argument("--p-size-strata", action="store_true")
    ap.add_argument("--bot-share", type=float, default=None)
    ap.add_argument("--donor-seed", type=int, default=1)
    ap.add_argument("--donor-subset", action="store_true")
    ap.add_argument("--v4-pool-manager", default=None)
    ap.add_argument("--max-sender-degree", type=int, default=None)
    ap.add_argument("--missing-grid", action="store_true")
    ap.add_argument("--decompose", action="store_true")
    ap.add_argument("--comparar", default=None, help="salida de `race` para comparar")
    a = ap.parse_args()
    if a.bot_share is not None:
        cc.MIN_SHARE_FALLIDOS = a.bot_share
    permitidos = {b for bs in cc.cargar_muestra(a.blocks, a.split).values() for b in bs}
    todos_items, todos_vac, nombres, lineas = [], [], [], []
    for e in cc.estratos(a.rows, a.blocks, a.split, None, a.max_sender_degree):
        tam = defaultdict(int)
        for r in e.filas:
            tam[r.block] = max(tam[r.block], r.tx_index + 1)
        recibos = {b: cc._leer_gz(a.raw, b, "receipts", permitidos) for b in e.bloques
                   if os.path.isfile(os.path.join(a.raw, f"{b}.receipts.json.gz"))}
        txs, vac = construir(e, recibos, tam, BINS, None, a.donor_seed, a.v4_pool_manager)
        items = aplicar(txs, vac, a.x_def, a.x_also, a.p_rule, a.p_size_strata, a.donor_subset)
        nombres.append(e.nombre)
        todos_items.append(items)
        todos_vac.append([v for v in vac])
        uni = [t for t in items if not t[4]]
        n1 = sum(t[2] for t in uni)
        t0 = [t[3] for t in uni if not t[2]]
        t1 = [t[3] for t in uni if t[2]]
        mhv, kc = estimar([items], "cond")
        dv, kd = estimar([items], "dec")
        lineas.append((e.nombre, len(uni), n1, kc, sum(t0) / len(t0) if t0 else float("nan"),
                       sum(t1) / len(t1) if t1 else float("nan"), mhv, kd, dv))
    sin = [[t for t in it if not t[4]] for it in todos_items]
    mhv, kc = estimar(sin, "cond")
    dv, kd = estimar(sin, "dec")
    n = sum(len(s) for s in sin)
    n1 = sum(t[2] for s in sin for t in s)
    t0 = [t[3] for s in sin for t in s if not t[2]]
    t1 = [t[3] for s in sin for t in s if t[2]]
    lineas.append(("POOL", n, n1, kc, sum(t0) / len(t0) if t0 else float("nan"),
                   sum(t1) / len(t1) if t1 else float("nan"), mhv, kd, dv))
    for nm, n, n1, kc, r0, r1, m, kd, d in lineas:
        if n:
            print(f"{nm:<4} PY n={n:>7} nX1={n1:>6} celdas={kc:>4} tasa0={_r(r0)} tasa1={_r(r1)} "
                  f"MH(ent,dec)={_f(m)} | solo-decil({kd})={_f(d)}")
    if a.missing_grid:
        pts = rejilla(todos_items, todos_vac)
        fin = {k: v for k, v in pts.items() if not math.isnan(v)}
        print("py grid min=%s max=%s" % (_f(min(fin.values())), _f(max(fin.values()))))
        for qi in range(11):
            print("py grid q_R=%.1f " % (qi / 10) + " ".join(_f(pts[(qi / 10, si / 10)]) for si in range(11)))
    if a.decompose:
        d = descomponer(sin)
        print("py decompose: " + " ".join(f"{k}={_f(v) if k != 'resid' else format(v, '.3e')}"
                                          for k, v in d.items()))
    if a.comparar:
        mal = 0
        pat = re.compile(r"^(\S+)\s+\S+\s+n=\s*(\d+) nX1=\s*(\d+) celdas=\s*(\d+) tasa0=(\S+) tasa1=(\S+) "
                         r"MH\(ent,dec\)=(\S+) .*solo-decil\((\d+)\)=(\S+)")
        esperado = {}
        for l in open(a.comparar):
            m = pat.match(l)
            if m:
                esperado[m.group(1)] = m.groups()[1:]
        for nm, n, n1, kc, r0, r1, mhv_, kd, d in lineas:
            if not n:
                continue
            py = (str(n), str(n1), str(kc), _r(r0), _r(r1), _f(mhv_), str(kd), _f(d))
            if esperado.get(nm) != py:
                mal += 1
                print("DISCREPANCIA", nm, "rust", esperado.get(nm), "python", py)
        print("PARIDAD OK" if not mal else f"PARIDAD FALLA ({mal})")
        sys.exit(1 if mal else 0)


if __name__ == "__main__":
    main()
