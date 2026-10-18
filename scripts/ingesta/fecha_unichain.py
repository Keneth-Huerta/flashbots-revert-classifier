"""Fecha del lanzamiento con trafico de usuarios de Unichain mainnet (ADR 005 §1.5), solo cabeceras.

Criterio escrito ANTES de mirar (2026-10-18):
  - Una sonda es el bloque de Unichain con timestamp 12:00:00 UTC de un dia (bloque = ts - genesis,
    reloj de 1 s de `src/chain.rs`; se comprueba que el timestamp de la cabecera coincide).
  - Una sonda "tiene trafico" si su cabecera lista al menos 10 transacciones. Un bloque sin
    usuarios lleva solo txs de sistema (deposito de atributos L1 y las del builder).
  - Dia de lanzamiento L = primer dia de la ventana tal que ese dia y los 6 siguientes tienen
    sonda con trafico.
  - Semanas elegibles de AF-03 en Unichain: las que inician en domingo UTC estrictamente despues de L.
Solo `eth_getBlockByNumber(n, false)` (cabecera, sin transacciones). No lee receipts, trazas ni
transacciones. Una sonda por dia, del 2024-11-05 (el 04 12:00 UTC es previo al genesis) al 2025-03-01. La llave se lee de `.env` y no se
imprime. Uso: python3 scripts/ingesta/fecha_unichain.py
"""
import datetime as dt
import json
import os
import time
import urllib.request

RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GENESIS = 1_730_748_359
UMBRAL, RACHA = 10, 7
INI, FIN = dt.date(2024, 11, 5), dt.date(2025, 3, 1)
SALIDA = os.path.join(RAIZ, "artefactos/exploratorio/fecha_unichain.json")


def url():
    for linea in open(os.path.join(RAIZ, ".env")):
        if linea.startswith("ALCHEMY_API_KEY="):
            k = linea.split("=", 1)[1].strip().strip('"').strip("'")
            return f"https://unichain-mainnet.g.alchemy.com/v2/{k}"
    raise SystemExit("falta ALCHEMY_API_KEY")


U = url()
LLAMADAS = 0


def cabecera(n):
    global LLAMADAS
    for intento in range(6):
        try:
            LLAMADAS += 1
            req = urllib.request.Request(U, json.dumps({"jsonrpc": "2.0", "id": 1, "method": "eth_getBlockByNumber",
                                                        "params": [hex(n), False]}).encode(),
                                         {"content-type": "application/json"})
            r = json.loads(urllib.request.urlopen(req, timeout=30).read())
            return r["result"]
        except Exception:  # noqa: BLE001 - reintento sin imprimir la URL
            time.sleep(2 ** intento * 0.5)
    raise SystemExit("RPC fallo")


def main():
    sondas, d = [], INI
    while d <= FIN:
        ts = int(dt.datetime(d.year, d.month, d.day, 12, tzinfo=dt.timezone.utc).timestamp())
        n = ts - GENESIS
        h = cabecera(n)
        if h is None:
            sondas.append({"dia": d.isoformat(), "bloque": n, "txs": None, "ts_ok": None})
        else:
            sondas.append({"dia": d.isoformat(), "bloque": n, "txs": len(h["transactions"]),
                           "ts_ok": int(h["timestamp"], 16) == ts})
        d += dt.timedelta(days=1)
    ok = [s["txs"] is not None and s["txs"] >= UMBRAL for s in sondas]
    L = next((sondas[i]["dia"] for i in range(len(ok) - RACHA + 1) if all(ok[i:i + RACHA])), None)
    os.makedirs(os.path.dirname(SALIDA), exist_ok=True)
    json.dump({"llamadas": LLAMADAS, "L": L, "sondas": sondas}, open(SALIDA, "w"), indent=1)
    print("llamadas", LLAMADAS, "L", L)
    for s in sondas:
        print(s["dia"], s["bloque"], s["txs"], s["ts_ok"])


main()
