"""Sorteo de las 48 semanas frescas de AF-04 (ADR 006 §1.3, decision D-A).

Semanas calendario que inician en domingo UTC. Elegibles: 2024, de 2024-03-17 a 2024-12-29
(posteriores a Dencun, 2024-03-13; 42 semanas); 2026, de 2026-01-04 a 2026-10-04 (40 semanas).
Por ano se toman las 24 con menor sha256("mev-af04-v1:<ano>:<inicio_de_semana>"), ordenadas
por fecha y numeradas F24-01 a F24-24 y F26-01 a F26-24. Imprime la lista y los argumentos
--stratum para `sample --neighbors` (las anclas son las filas con role=anchor; ver
el anexo privado, decision 1). Uso: python3 sorteo_semanas_af04.py
"""
import datetime as dt
import hashlib

RANGOS = {2024: (dt.date(2024, 3, 17), dt.date(2024, 12, 29)),
          2026: (dt.date(2026, 1, 4), dt.date(2026, 10, 4))}
N = 24


def elegibles(ano):
    ini, fin = RANGOS[ano]
    assert ini.weekday() == 6 and fin.weekday() == 6  # domingos
    return [ini + dt.timedelta(days=7 * i) for i in range((fin - ini).days // 7 + 1)]


def sorteo(ano):
    el = elegibles(ano)
    h = sorted(el, key=lambda s: hashlib.sha256(f"mev-af04-v1:{ano}:{s.isoformat()}".encode()).hexdigest())[:N]
    return len(el), sorted(h)


if __name__ == "__main__":
    args = []
    for ano in sorted(RANGOS):
        n, h = sorteo(ano)
        pref = f"F{ano % 100}"
        print(f"{ano}: {n} elegibles")
        for i, s in enumerate(h):
            print(f"  {pref}-{i + 1:02d} {s.isoformat()}")
            args.append(f"--stratum {pref}-{i + 1:02d}={s.isoformat()}")
    print(" ".join(args))
