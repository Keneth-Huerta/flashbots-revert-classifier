"""Pruebas de eventos.py con datos sinteticos. Uso: python3 -I -m unittest discover scripts/tests"""
import os
import random
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analisis"))
import eventos as ev  # noqa: E402


def sintetico(semilla, bloques=30, despues=False):
    """Cada bloque: 1 exito de A y 4 fallos de B/C con el pool p1. Si `despues`, los fallos
    van siempre despues del exito; si no, el orden es aleatorio."""
    r = random.Random(semilla)
    txs = []
    for b in range(1, bloques + 1):
        n = 5
        idx = list(range(n))
        if despues:
            e, idx_f = 0, [1, 2, 3, 4]
        else:
            r.shuffle(idx)
            e, idx_f = idx[0], idx[1:]
        txs.append((b, e, "A", ("p1",), "success", b % 2 == 0))
        for k, i in enumerate(idx_f):
            txs.append((b, i, "BC"[k % 2], ("p1",), "revert" if k % 2 == 0 else "probe", False))
    return txs


class Eventos(unittest.TestCase):
    def test_estadistico_a_mano(self):
        # exito en idx 1; fallo ajeno (idx 0) antes, fallo ajeno (idx 2) despues,
        # fallo de la misma entidad (idx 3) excluido, fallo sin pool comun (idx 4) excluido.
        txs = [(1, 1, "A", ("p",), "success", True),
               (1, 0, "B", ("p", "q"), "revert", False),
               (1, 2, "C", ("p",), "revert", False),
               (1, 3, "A", ("p",), "revert", False),
               (1, 4, "D", ("z",), "revert", False)]
        d = ev.preparar(txs)
        self.assertEqual(len(d.ps), 2)
        self.assertEqual(ev._numeradores(d, d.rank).sum(), 1)
        self.assertEqual(d.n_exitos, [1, 0])

    def test_nulo_conserva_permutacion_por_bloque(self):
        d = ev.preparar(sintetico(1, bloques=5))
        rng = np.random.default_rng(3)
        for _ in range(20):
            nuevo = ev._rango_nulo(d, rng)
            for b in set(d.bloque):
                self.assertEqual(sorted(nuevo[d.bloque == b] - nuevo[d.bloque == b].min()),
                                 list(range(int((d.bloque == b).sum()))))

    def test_calibracion(self):
        menores = 0
        semillas = 300
        for s in range(semillas):
            d = ev.preparar(sintetico(s))
            r = ev.analizar(d, k=99, b=0, rng=np.random.default_rng(1000 + s))
            menores += r[("ciclicos", "todos")]["p"] < 0.05
        fr = menores / semillas
        self.assertTrue(0.02 <= fr <= 0.08, fr)

    def test_poder(self):
        for s in range(5):
            d = ev.preparar(sintetico(s, despues=True))
            r = ev.analizar(d, k=199, b=0, rng=np.random.default_rng(s))
            for g in ev.GRUPOS:
                self.assertLess(r[(g, "todos")]["p"], 0.01)
            self.assertGreater(r[("ciclicos", "todos")]["exceso"], 0.3)

    def test_ic_bootstrap_cubre_cero_bajo_nulo(self):
        d = ev.preparar(sintetico(7, bloques=60))
        r = ev.analizar(d, k=100, b=200, kb=20, rng=np.random.default_rng(5))
        lo, hi = r[("no_ciclicos", "todos")]["ic"]
        self.assertLess(lo, hi)

    def test_sin_pares(self):
        d = ev.preparar([(1, 0, "A", ("p",), "success", True)])
        r = ev.analizar(d, k=5, b=0)
        self.assertIsNone(r[("ciclicos", "todos")]["p"])


if __name__ == "__main__":
    unittest.main()
