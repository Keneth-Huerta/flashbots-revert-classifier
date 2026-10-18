#!/usr/bin/env python3
"""Descriptivos del piloto de cadenas frente a Base (EXPLORATORIO, EXPL)."""
import csv, gzip, json, os
from collections import Counter, defaultdict

BASE = "artefactos/exploratorio/rows.csv"
REDES = ["unichain-mainnet", "opt-mainnet"]
RAW = "artefactos/rpc/piloto_cadenas"
OUT = "artefactos/piloto_cadenas"
# selectores que el extractor ya cubre (src/extract.rs) + balanceOf
CUBIERTOS = {"0x0902f1ac", "0x3850c7bd", "0xe76c01e4", "0x1a686502", "0x022c0d9f", "0x128acb08",
             "0x1e2eaeaf", "0x35fd631a", "0xdbd035ff", "0x0dfe1681", "0xd21220a7", "0xf30dba93",
             "0x5339c296", "0x514ea4bf", "0x70a08231"}
CONOCIDOS = {
    "0xa9059cbb": "ERC20 transfer", "0x23b872dd": "ERC20 transferFrom", "0x095ea7b3": "ERC20 approve",
    "0xd0e30db0": "WETH deposit", "0x2e1a7d4d": "WETH withdraw", "0x18160ddd": "totalSupply()",
    "0x313ce567": "decimals()", "0xdd62ed3e": "allowance", "0x0902f1ac": "getReserves (cubierto)",
    "0x022c0d9f": "swap V2 (cubierto)", "0x128acb08": "swap V3 (cubierto)",
    "0x3850c7bd": "slot0 (cubierto)", "0x70a08231": "balanceOf (cubierto)",
    "0x0dfe1681": "token0 (cubierto)", "0xd21220a7": "token1 (cubierto)",
    "0xddca3f43": "fee() (pool V3)", "0x1a686502": "liquidity (cubierto)",
    "0x5c60da1b": "implementation()", "0x4d2301cc": "getEthBalance (Multicall)",
    "0x252dba42": "aggregate (Multicall3)", "0x5ae401dc": "multicall(deadline) Uni router",
    "0x3593564c": "UniversalRouter execute", "0x472b43f3": "swapExactTokensForTokens (V2 router)",
    "0x7ff36ab5": "swapExactETHForTokens", "0x38ed1739": "swapExactTokensForTokens (V2 router)",
    "0x04e45aaf": "exactInputSingle (SwapRouter02)", "0x414bf389": "exactInputSingle (SwapRouter)",
    "0x0b4c7e4d": "UniV4 PoolManager modifyLiquidities?", "0x48c89491": "UniV4 unlock(bytes)",
    "0x91dd7346": "UniV4 unlockCallback(bytes)", "0xf3cd914c": "UniV4 swap(PoolKey,..)",
    "0x2e6a5268": "UniV4 ?", "0xa5841194": "UniV4 sync/settle?", "0x11da60b4": "UniV4 settle()",
    "0x0b0d9c09": "UniV4 take(currency,to,amt)", "0xf135baaa": "UniV4 settleFor?",
    "0xd9caed12": "withdraw(address,address,uint256) (aggr?)", "0xab9c4b5d": "Aave flashLoan",
    "0x5cffe9de": "ERC3156 flashLoan", "0xa1d48336": "Velodrome/Solidly getAmountOut(uint,address)",
    "0xf140a35a": "Aerodrome/Velodrome getReserves-variant?", "0x5a76f25e": "Solidly stable()?",
    "0x1f00ca74": "Uniswap V2 getAmountsIn", "0xd06ca61f": "V2 getAmountsOut",
    "0x0902f1ac ": "", "0x5e0d443f": "Curve get_dy_underlying?", "0x5b41b908": "Curve get_dy(int128,int128,uint256)?",
    "0x3df02124": "Curve exchange(int128,int128,uint256,uint256)", "0xa6417ed6": "Curve exchange_underlying",
    "0x4515cef3": "Curve ?", "0x52bbbe29": "Balancer V2 swap (Vault)", "0x945bcec9": "Balancer V2 batchSwap",
    "0x0f5287b0": "Uni V3 pool-like ?", "0xfa461e33": "uniswapV3SwapCallback", "0x10d1e85c": "uniswapV2Call",
    "0x84800812": "Solidly getAmountOut?", "0x3e32a2f6": "?",
}


def leer(ruta):
    with open(ruta) as f:
        return list(csv.DictReader(f))


def desc(rows):
    bl = {r["block"] for r in rows}
    n = len(rows); nb = len(bl)
    k = Counter(r["kind"] for r in rows)
    return dict(bloques=nb, tx=n, tx_bloque=n / nb, revert_bloque=k["revert"] / nb, probe_bloque=k["probe"] / nb,
                p_revert=k["revert"] / n, p_probe=k["probe"] / n, p_fallido=(k["revert"] + k["probe"]) / n)


def cobertura(rows, minimo):
    fall = [r for r in rows if r["kind"] in ("revert", "probe")]
    cnt = Counter(r["to"] for r in fall)
    ok = [r for r in fall if cnt[r["to"]] >= minimo]
    if not ok:
        return None, 0, len(cnt)
    return sum(1 for r in ok if r["pools"]) / len(ok), len(ok), sum(1 for c in cnt.values() if c >= minimo)


def selectores_sin_pools(red, rows):
    sin = {r["tx_hash"]: r for r in rows if r["kind"] in ("revert", "probe") and not r["pools"]}
    bloques = {int(r["block"]) for r in sin.values()}
    cont, ejemplo = Counter(), {}
    for b in sorted(bloques):
        with gzip.open(f"{RAW}/{red}/{b}.trace.json.gz", "rt") as f:
            res = json.load(f)["result"]
        for it in res:
            h = it["txHash"].lower()
            if h not in sin:
                continue
            bot = (it["result"].get("to") or "").lower()
            pila = [it["result"]]
            while pila:
                nd = pila.pop()
                for c in nd.get("calls") or []:
                    if (c.get("from") or "").lower() == bot:
                        s = (c.get("input") or "")[:10].lower()
                        cont[s] += 1
                        ejemplo.setdefault(s, (c.get("to") or "").lower())
                    pila.append(c)
    return cont, ejemplo


def main():
    base = leer(BASE)
    tabla = {"Base(S4)": base}
    for r in REDES:
        tabla[r] = leer(f"{OUT}/{r}/rows.csv")
    print("== descriptivos ==")
    print(f"{'red':18}{'bloq':>6}{'tx':>8}{'tx/b':>8}{'rev/b':>8}{'probe/b':>9}{'%rev':>7}{'%probe':>8}{'%fall':>7}")
    for n, rows in tabla.items():
        d = desc(rows)
        print(f"{n:18}{d['bloques']:6}{d['tx']:8}{d['tx_bloque']:8.1f}{d['revert_bloque']:8.2f}{d['probe_bloque']:9.2f}"
              f"{100*d['p_revert']:7.1f}{100*d['p_probe']:8.1f}{100*d['p_fallido']:7.1f}")
    print("\n== cobertura de pools en fallidos (revert+probe) por contrato 'to' con >= m fallos ==")
    for n, rows in tabla.items():
        for m in (5, 2):
            c, nt, nc = cobertura(rows, m)
            print(f"{n:18} m>={m}: contratos={nc:4} intentos={nt:6} cobertura={'NA' if c is None else f'{100*c:.1f}%'}")
        fall = [r for r in rows if r["kind"] in ("revert", "probe")]
        print(f"{n:18} todos los fallidos: {len(fall)} con pools={100*sum(1 for r in fall if r['pools'])/max(1,len(fall)):.1f}%")
    for red in REDES:
        rows = tabla[red]
        fall = [r for r in rows if r["kind"] in ("revert", "probe")]
        print(f"\n== {red}: top 15 'to' de fallidos (n, %con pools) ==")
        por = defaultdict(list)
        for r in fall:
            por[r["to"]].append(r)
        for to, rs in sorted(por.items(), key=lambda kv: -len(kv[1]))[:15]:
            print(f"{to} {len(rs):4} {100*sum(1 for r in rs if r['pools'])/len(rs):5.0f}%")
        cont, ej = selectores_sin_pools(red, rows)
        print(f"-- top 20 selectores de llamadas desde el bot, fallidos SIN pools ({sum(cont.values())} llamadas)")
        for s, c in cont.most_common(20):
            nota = "CUBIERTO" if s in CUBIERTOS else ""
            nom = CONOCIDOS.get(s, "DESCONOCIDO")
            print(f"{s} {c:5} {nom} {nota} ej_callee={ej[s]}")


if __name__ == "__main__":
    main()
