"""Utilidades compartidas de los analisis exploratorios de contencion.

Reimplementa, sin cambios de criterio, el universo de bots y las entidades de
`src/contend.rs` (fundamento `contencion-entre-bots.md` 4.1). Lee solo bloques del
split pedido: `leer_raw` rechaza cualquier bloque fuera de ese conjunto.
"""
import csv
import gzip
import json
import os
import sys
from collections import defaultdict
from typing import NamedTuple

GENESIS_TS = 1686789347
BLOCK_TIME = 2
# Reloj de bloque por cadena (espejo de `src/chain.rs`): (genesis_ts, segundos por bloque).
CADENAS = {
    "base": (1686789347, 2),
    "unichain": (1730748359, 1),
    "op": (1475598777, 2),
}
LISTA_SISTEMA = "datos/tx_sistema.csv"
FALLIDOS = ("probe", "revert")
SWAP_TOPICS = frozenset(
    [
        "0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822",
        "0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67",
        "0x40e9cecb9f5f1f1c5b9c97dec2917b7ee92e57ba5563708daca94dd84ad7112f",
        "0xb3e2773606abfd36b5bd91394b3a54d1398336c65005baf7bf7a05efeffaf75b",
        "0x19b47279256b2a23a1665c810c8d55a1758940ee09377d4f8d26497a3577dc83",
    ]
)
MIN_FALLIDOS = 5
MIN_SHARE_FALLIDOS = 0.5


class Fila(NamedTuple):
    block: int
    tx_index: int
    tx_hash: str
    from_: str
    to: str
    kind: str
    cyclic: bool
    pools: tuple

    @property
    def fallida(self):
        return self.kind in FALLIDOS


def block_timestamp(block, cadena="base"):
    g, dt = CADENAS[cadena]
    return g + dt * block


def cargar_filtro_sistema(path, cadena):
    """(remitentes, destinos) de la cadena en la lista de tx de sistema y de builder
    (`datos/tx_sistema.csv`; ADR 005 D2). Falla ante cadena, rol o direccion invalidos."""
    remitentes, destinos = set(), set()
    with open(path, newline="") as f:
        lineas = (l for l in f if not l.startswith("#"))
        for i, r in enumerate(csv.DictReader(lineas), start=2):
            if r["cadena"].strip() not in CADENAS:
                raise ValueError(f"lista de sistema, linea {i}: cadena desconocida {r['cadena']!r}")
            a = r["direccion"].strip().lower()
            if len(a) != 42 or not a.startswith("0x") or any(c not in "0123456789abcdef" for c in a[2:]):
                raise ValueError(f"lista de sistema, linea {i}: direccion mal formada {a!r}")
            rol = r["rol"].strip()
            if rol not in ("remitente", "destino"):
                raise ValueError(f"lista de sistema, linea {i}: rol desconocido {rol!r}")
            if r["cadena"].strip() == cadena:
                (remitentes if rol == "remitente" else destinos).add(a)
    return remitentes, destinos


def es_sistema(fila, filtro):
    """True si la fila es una tx de sistema o de builder segun `filtro` ((rem, dest) o None)."""
    return filtro is not None and (fila.from_.lower() in filtro[0] or fila.to.lower() in filtro[1])


def opciones_cadena():
    """Extrae de sys.argv `--chain C`, `--exclude-system[=BOOL]`, `--system-list P` y
    `--max-sender-degree N` (mismas opciones que `race`, `contend` y `neighbor` de Rust) y
    devuelve dict(cadena, filtro, max_grado). `--exclude-system` por defecto: apagado en
    base, encendido en unichain y op (ADR 005 D2)."""
    argv, cadena, excluir, lista, grado = sys.argv, "base", None, LISTA_SISTEMA, None
    out = [argv[0]]
    i = 1
    while i < len(argv):
        a = argv[i]
        if a == "--chain":
            cadena = argv[i + 1]
            i += 2
        elif a == "--system-list":
            lista = argv[i + 1]
            i += 2
        elif a == "--max-sender-degree":
            grado = int(argv[i + 1])
            i += 2
        elif a.startswith("--exclude-system"):
            if "=" in a:
                excluir = a.split("=", 1)[1].lower() == "true"
                i += 1
            elif i + 1 < len(argv) and argv[i + 1].lower() in ("true", "false"):
                excluir = argv[i + 1].lower() == "true"
                i += 2
            else:
                excluir = True
                i += 1
        else:
            out.append(a)
            i += 1
    if cadena not in CADENAS:
        raise SystemExit(f"cadena desconocida: {cadena}")
    sys.argv[:] = out
    if excluir is None:
        excluir = cadena != "base"
    filtro = cargar_filtro_sistema(lista, cadena) if excluir else None
    return {"cadena": cadena, "filtro": filtro, "max_grado": grado}


def cargar_muestra(path, split="exploratory"):
    """Devuelve {estrato: [bloques]} del split ('exploratory' o 'confirmatory')."""
    if split not in ("exploratory", "confirmatory"):
        raise ValueError(f"split invalido: {split}")
    quiero = "true" if split == "exploratory" else "false"
    out = defaultdict(list)
    with open(path, newline="") as f:
        for r in csv.DictReader(f):
            if r["exploratory"].strip().lower() == quiero:
                out[r["stratum"]].append(int(r["block"]))
    return {k: sorted(v) for k, v in sorted(out.items())}


def leer_filas(path, bloques=None):
    """Lee rows.csv; si se da `bloques`, conserva solo filas de esos bloques."""
    permitidos = set(bloques) if bloques is not None else None
    filas = []
    with open(path, newline="") as f:
        for r in csv.DictReader(f):
            b = int(r["block"])
            if permitidos is not None and b not in permitidos:
                continue
            pools = tuple(r["pools"].split(";")) if r["pools"] else ()
            filas.append(
                Fila(b, int(r["tx_index"]), r["tx_hash"].lower(), r["from"], r["to"],
                     r["kind"], r["cyclic"].strip().lower() == "true", pools)
            )
    return filas


def universo_bots(filas):
    """Contratos con >=5 fallidas con pools y fallidas >= 0.5 * todas sus tx."""
    todas, fallidas, con_pools = defaultdict(int), defaultdict(int), defaultdict(int)
    for r in filas:
        todas[r.to] += 1
        if r.fallida:
            fallidas[r.to] += 1
            if r.pools:
                con_pools[r.to] += 1
    return {
        t for t in todas
        if con_pools[t] >= MIN_FALLIDOS and fallidas[t] >= MIN_SHARE_FALLIDOS * todas[t]
    }


def entidades(filas, bots, max_grado=None):
    """contrato -> representante (menor direccion) de la componente conexa por `from`.
    Con `max_grado`, los remitentes que llaman a al menos ese numero de contratos bot
    distintos se ignoran (como `contend --max-sender-degree`)."""
    nombres = sorted(bots)
    ignorados = set()
    if max_grado is not None:
        llamados = defaultdict(set)
        for r in filas:
            if r.to in bots:
                llamados[r.from_].add(r.to)
        ignorados = {f for f, cs in llamados.items() if len(cs) >= max_grado}
    padre = {n: n for n in nombres}

    def buscar(x):
        while padre[x] != x:
            padre[x] = padre[padre[x]]
            x = padre[x]
        return x

    primero = {}
    for r in filas:
        if r.to not in padre or r.from_ in ignorados:
            continue
        if r.from_ not in primero:
            primero[r.from_] = r.to
        else:
            a, b = buscar(r.to), buscar(primero[r.from_])
            if a != b:
                lo, hi = (a, b) if a < b else (b, a)
                padre[hi] = lo
    return {n: buscar(n) for n in nombres}


class Estrato:
    """Filas, bots y entidades de un estrato."""

    def __init__(self, nombre, bloques, filas_todas, filtro=None, max_grado=None):
        self.nombre = nombre
        self.bloques = list(bloques)
        s = set(bloques)
        # `filas` conserva todas las tx del bloque (largo del bloque, bloques con filas);
        # bots, entidades y `filas_bot` salen de las que no son de sistema.
        self.filas = [r for r in filas_todas if r.block in s]
        utiles = [r for r in self.filas if not es_sistema(r, filtro)]
        self.bots = universo_bots(utiles)
        self.entidad_de = entidades(utiles, self.bots, max_grado)
        self.filas_bot = [r for r in utiles if r.to in self.entidad_de]

    def entidad(self, fila):
        return self.entidad_de[fila.to]

    @property
    def n_entidades(self):
        """Entidades con al menos una tx con pools (columna `bots` de `contend`)."""
        return len({self.entidad(r) for r in self.filas_bot if r.pools})


def estratos(rows_path, blocks_path, split="exploratory", filtro=None, max_grado=None):
    muestra = cargar_muestra(blocks_path, split)
    todos = {b for bs in muestra.values() for b in bs}
    filas = leer_filas(rows_path, todos)
    return [Estrato(n, bs, filas, filtro, max_grado) for n, bs in muestra.items()]


def _leer_gz(raw_dir, block, sufijo, permitidos):
    if block not in permitidos:
        raise PermissionError(f"bloque {block} fuera del split pedido")
    with gzip.open(os.path.join(raw_dir, f"{block}.{sufijo}.json.gz"), "rt") as f:
        return json.load(f)["result"]


def leer_raw(raw_dir, block, permitidos):
    """(trazas, receipts) del bloque; `permitidos` es el conjunto de bloques del split."""
    return (_leer_gz(raw_dir, block, "trace", permitidos),
            _leer_gz(raw_dir, block, "receipts", permitidos))


def tiene_raw(raw_dir, block):
    """True si el bloque tiene sus dos capturas crudas (trazas y receipts) en disco."""
    return all(
        os.path.isfile(os.path.join(raw_dir, f"{block}.{sufijo}.json.gz"))
        for sufijo in ("trace", "receipts")
    )


def inputs_raiz(trazas):
    """tx_hash -> calldata de la llamada raiz."""
    return {t["txHash"].lower(): t["result"].get("input", "") for t in trazas}


def pools_con_swap(receipts):
    """Direcciones que emitieron un evento Swap en el bloque."""
    out = set()
    for rc in receipts:
        for log in rc.get("logs", []):
            topics = log.get("topics") or []
            if topics and topics[0].lower() in SWAP_TOPICS:
                out.add(log["address"].lower())
    return out


def fmt_pct(num, den):
    return f"{num / den:.4f}" if den else "n/a"


def argumentos(descripcion):
    import argparse

    p = argparse.ArgumentParser(description=descripcion)
    p.add_argument("--rows", default="artefactos/exploratorio/rows.csv")
    p.add_argument("--blocks", default="datos/muestra_bloques_v1.csv")
    p.add_argument("--raw", default="artefactos/rpc/muestra_v1")
    p.add_argument("--split", choices=["exploratory", "confirmatory"], default="exploratory")
    return p.parse_args()
