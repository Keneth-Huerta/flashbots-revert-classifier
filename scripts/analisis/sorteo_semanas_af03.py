"""Sorteo de las 12 semanas de AF-03 por cadena (ADR 005 §1.5).

Semanas de 2025 que inician en domingo UTC (2025-01-05 a 2025-12-28). Elegibles: OP, las 52;
Unichain, las que inician estrictamente despues de L (dia de lanzamiento, `fecha_unichain.py`).
Se toman las 12 con menor sha256("mev-af03-v1:<cadena>:<inicio_de_semana>"), ordenadas por fecha.
Imprime los argumentos --stratum para `sample`. Uso: python3 sorteo_semanas_af03.py [L_unichain]
"""
import datetime as dt
import hashlib
import sys

L = dt.date.fromisoformat(sys.argv[1]) if len(sys.argv) > 1 else dt.date(2025, 2, 12)
semanas = [dt.date(2025, 1, 5) + dt.timedelta(days=7 * i) for i in range(52)]
for cadena, pref in (("unichain", "U"), ("op", "O")):
    el = [s for s in semanas if cadena != "unichain" or s > L]
    h = sorted(el, key=lambda s: hashlib.sha256(f"mev-af03-v1:{cadena}:{s.isoformat()}".encode()).hexdigest())[:12]
    h.sort()
    print(cadena, len(el), "elegibles")
    print(" ".join(f"--stratum {pref}{i + 1:02d}={s.isoformat()}" for i, s in enumerate(h)))
