"""Descarga de AF-02 semana por semana (preregistro 2026-10-18, seccion Ejecucion).

Por cada semana de `datos/muestra_bloques_af02.csv`, en orden:
  1. muestra de la semana en `artefactos/af02/muestras/<W>.csv` (subconjunto exacto de filas);
  2. `fetch --split confirmatory --seal <sello>` con crudo en `artefactos/rpc_af02/` y filas en
     `artefactos/af02/filas/<W>.csv` (+ manifiesto);
  3. trazas de la semana empaquetadas en `artefactos/af02_trazas/<W>.tar` y borradas;
  4. `dvc add` + `dvc push` del `.tar`, luego borrado local del `.tar` y de su objeto en cache.
Los receipts se quedan en disco (los lee `race`). Reanudable: una semana con filas y `.tar.dvc`
se salta. No calcula ninguna estadistica: solo imprime cobertura y errores de `fetch`.
La llave se lee de `.env` al entorno del subproceso; nunca se imprime (salida enmascarada).
Uso: python3 scripts/ingesta/descarga_af02.py [--semanas W01,W02] [--sin-dvc]
"""
import argparse
import csv
import os
import re
import subprocess
import sys
import tarfile
import time

RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
MUESTRA = "datos/muestra_bloques_af02.csv"
SELLO = "docs/04_investigacion_y_labs/2026-10-18-sello-af02.sha256"
CRUDO = "artefactos/rpc_af02"
BIN = "target/release/mev-revert-classifier"
DVC = os.environ.get("DVC", "/home/valge/Documents/repos/jarvis-micro-ssm/venv/bin/dvc")
LARGO = re.compile(r"[A-Za-z0-9_-]{32,}")


def entorno():
    env = dict(os.environ)
    with open(os.path.join(RAIZ, ".env")) as f:
        for linea in f:
            linea = linea.strip()
            if linea and not linea.startswith("#") and "=" in linea:
                k, v = linea.split("=", 1)
                env[k.strip()] = v.strip().strip('"').strip("'")
    return env


def correr(cmd, env=None):
    for _ in range(720):  # dvc: espera hasta 12 h si otro proceso tiene el lock del repo
        p = subprocess.run(cmd, cwd=RAIZ, env=env, capture_output=True, text=True)
        salida = LARGO.sub("<masked>", p.stdout + p.stderr)
        if p.returncode == 0 or cmd[0] != DVC or "lock" not in salida.lower():
            break
        time.sleep(60)
    if p.returncode != 0:
        sys.exit(f"fallo ({p.returncode}): {' '.join(cmd[:2])}\n{salida[-2000:]}")
    return salida


def borrar_cache(dvc_file):
    md5 = None
    for linea in open(dvc_file):
        m = re.match(r"\s*-?\s*md5:\s*([0-9a-f]{32})", linea)
        if m:
            md5 = m.group(1)
    if md5:
        obj = os.path.join(RAIZ, ".dvc/cache/files/md5", md5[:2], md5[2:])
        if os.path.exists(obj):
            os.remove(obj)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--semanas", default="")
    ap.add_argument("--sin-dvc", action="store_true")
    a = ap.parse_args()
    filas = list(csv.DictReader(open(os.path.join(RAIZ, MUESTRA))))
    semanas = sorted({r["stratum"] for r in filas})
    if a.semanas:
        semanas = [w for w in semanas if w in a.semanas.split(",")]
    for d in ("artefactos/af02/muestras", "artefactos/af02/filas", "artefactos/af02_trazas", CRUDO):
        os.makedirs(os.path.join(RAIZ, d), exist_ok=True)
    env = entorno()
    for w in semanas:
        out = f"artefactos/af02/filas/{w}.csv"
        tar = f"artefactos/af02_trazas/{w}.tar"
        if os.path.exists(os.path.join(RAIZ, out)) and os.path.exists(os.path.join(RAIZ, tar + ".dvc")):
            print(f"{w}: ya descargada, se salta", flush=True)
            continue
        sub = [r for r in filas if r["stratum"] == w]
        m = f"artefactos/af02/muestras/{w}.csv"
        with open(os.path.join(RAIZ, m), "w", newline="") as f:
            cw = csv.DictWriter(f, fieldnames=list(filas[0].keys()), lineterminator="\n")
            cw.writeheader()
            cw.writerows(sub)
        log = correr([BIN, "fetch", "--blocks", m, "--split", "confirmatory", "--seal", SELLO,
                      "--raw-dir", CRUDO, "--out", out], env)
        print(f"{w}: " + log.strip().splitlines()[-1], flush=True)
        trazas = [f"{r['block']}.trace.json.gz" for r in sub]
        with tarfile.open(os.path.join(RAIZ, tar), "w") as t:
            for n in trazas:
                p = os.path.join(RAIZ, CRUDO, n)
                if os.path.exists(p):
                    t.add(p, arcname=n)
        for n in trazas:
            p = os.path.join(RAIZ, CRUDO, n)
            if os.path.exists(p):
                os.remove(p)
        if a.sin_dvc:
            continue
        correr([DVC, "add", tar])
        correr([DVC, "push", tar + ".dvc"])
        os.remove(os.path.join(RAIZ, tar))
        borrar_cache(os.path.join(RAIZ, tar + ".dvc"))
        print(f"{w}: trazas en DVC ({len(trazas)} bloques)", flush=True)


if __name__ == "__main__":
    main()
