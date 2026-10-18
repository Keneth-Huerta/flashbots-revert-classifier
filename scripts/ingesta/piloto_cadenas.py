#!/usr/bin/env python3
"""Piloto EXPLORATORIO de cadenas (Unichain, OP Mainnet): baja traza y receipts.

Formato igual a src/fetch.rs: cuerpo JSON-RPC completo (texto crudo) en
<bloque>.trace.json.gz y <bloque>.receipts.json.gz. La llave se lee de
ALCHEMY_API_KEY y nunca se imprime (toda excepcion pasa por enmascarar).
"""
import csv, gzip, hashlib, json, os, random, sys, time, urllib.request, urllib.error
from datetime import datetime, timezone

REDES = ["unichain-mainnet", "opt-mainnet"]
SEMILLA = 20261017
N_BLOQUES = 30
DIAS = 7
RAIZ = os.path.join("artefactos", "rpc", "piloto_cadenas")


def enmascarar(texto, llave):
    """Quita la llave (y cualquier segmento tras /v2/) de un texto."""
    texto = str(texto)
    if llave:
        texto = texto.replace(llave, "***")
    out, i = [], 0
    while True:
        j = texto.find("/v2/", i)
        if j < 0:
            out.append(texto[i:]); break
        out.append(texto[i:j + 4] + "***")
        k = j + 4
        while k < len(texto) and not texto[k].isspace() and texto[k] not in "\"'<>)":
            k += 1
        i = k
    return "".join(out)


def sortear(rng_semilla, red, ultimo, por_dia, n=N_BLOQUES, dias=DIAS):
    """n bloques distintos, uniformes, en los ultimos `dias` dias. Determinista."""
    ventana = int(por_dia * dias)
    lo = max(1, ultimo - ventana)
    rng = random.Random(f"{rng_semilla}:{red}")
    return sorted(rng.sample(range(lo, ultimo - 100), n))  # margen 100 bloques por reorgs


class Cliente:
    def __init__(self, red, llave):
        self.url = f"https://{red}.g.alchemy.com/v2/{llave}"
        self.llave = llave
        self.llamadas = 0
        self.fallas = 0

    def call(self, metodo, params):
        cuerpo = json.dumps({"jsonrpc": "2.0", "id": 1, "method": metodo, "params": params}).encode()
        ultimo = ""
        for intento in range(6):
            if intento:
                time.sleep(0.5 * 2 ** intento)
            self.llamadas += 1
            try:
                req = urllib.request.Request(self.url, cuerpo, {"Content-Type": "application/json"})
                with urllib.request.urlopen(req, timeout=300) as r:
                    texto = r.read().decode()
                p = json.loads(texto)
                if "error" in p:
                    cod = p["error"].get("code")
                    if cod in (-32005, 429):
                        ultimo = f"{metodo}: rate limited"; continue
                    raise RuntimeError(f"{metodo}: RPC error {p['error']}")
                return texto, p["result"]
            except urllib.error.HTTPError as e:
                ultimo = f"{metodo}: HTTP {e.code}"
                if not (e.code == 429 or e.code >= 500):
                    raise RuntimeError(enmascarar(ultimo, self.llave)) from None
            except RuntimeError:
                raise
            except Exception as e:  # transporte: no eco de URL
                ultimo = f"{metodo}: {type(e).__name__}"
        self.fallas += 1
        raise RuntimeError(enmascarar(ultimo + " tras reintentos", self.llave))


def sha(b):
    return hashlib.sha256(b).hexdigest()


def main():
    llave = os.environ.get("ALCHEMY_API_KEY", "")
    if not llave:
        sys.exit("falta ALCHEMY_API_KEY en el entorno")
    resumen = {}
    for red in REDES:
        try:
            c = Cliente(red, llave)
            _, h = c.call("eth_blockNumber", [])
            ultimo = int(h, 16)
            _, b1 = c.call("eth_getBlockByNumber", [hex(ultimo), False])
            _, b0 = c.call("eth_getBlockByNumber", [hex(ultimo - 100000), False])
            seg = int(b1["timestamp"], 16) - int(b0["timestamp"], 16)
            por_dia = 100000 / seg * 86400
            bloques = sortear(SEMILLA, red, ultimo, por_dia)
            d = os.path.join(RAIZ, red); os.makedirs(d, exist_ok=True)
            filas, fallos = [], []
            for blk in bloques:
                tag = hex(blk)
                reg = {"block": blk}
                try:
                    for kind, m, p in (("trace", "debug_traceBlockByNumber", [tag, {"tracer": "callTracer"}]),
                                       ("receipts", "eth_getBlockReceipts", [tag])):
                        ruta = os.path.join(d, f"{blk}.{kind}.json.gz")
                        if os.path.exists(ruta):
                            with gzip.open(ruta, "rb") as f:
                                texto = f.read()
                        else:
                            t, _ = c.call(m, p); texto = t.encode()
                            tmp = ruta + ".tmp"
                            with gzip.open(tmp, "wb") as f:
                                f.write(texto)
                            os.replace(tmp, ruta)
                        reg[kind + "_sha256"] = sha(texto)
                        reg[kind + "_gz_bytes"] = os.path.getsize(ruta)
                    reg["ts_utc"] = datetime.now(timezone.utc).isoformat()
                    filas.append(reg)
                except Exception as e:
                    fallos.append((blk, enmascarar(e, llave)))
            with open(os.path.join(d, "manifest.csv"), "w", newline="") as f:
                w = csv.DictWriter(f, ["block", "trace_sha256", "trace_gz_bytes", "receipts_sha256", "receipts_gz_bytes", "ts_utc"])
                w.writeheader(); w.writerows(filas)
            # CSV compatible con datos/muestra_bloques_v1.csv
            with open(os.path.join(d, "bloques.csv"), "w", newline="") as f:
                w = csv.writer(f); w.writerow(["stratum", "block", "hour_start_ts", "exploratory"])
                for r in filas:
                    w.writerow(["P", r["block"], 0, "true"])
            resumen[red] = dict(ultimo=ultimo, por_dia=round(por_dia), bloques_ok=len(filas),
                                fallos=fallos, llamadas=c.llamadas)
        except Exception as e:
            resumen[red] = {"error": enmascarar(e, llave)}
    print(json.dumps(resumen, indent=1))


if __name__ == "__main__":
    main()
