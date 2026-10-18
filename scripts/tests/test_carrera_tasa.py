"""Pruebas de carrera_tasa.py con datos sinteticos. Uso: python3 -I -m unittest discover scripts/tests"""
import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analisis"))
import carrera_tasa as ct  # noqa: E402


def tx(idx, ent, kind, pools=("p",), sw=None):
    return (idx, ent, pools, kind, frozenset(pools if sw is None else sw) if kind == "success" else frozenset())


class XPrevio(unittest.TestCase):
    def test_x(self):
        lista = [tx(0, "X", "revert"), tx(1, "Y", "success"), tx(2, "X", "revert"),
                 tx(3, "Y", "revert"), tx(4, "Z", "probe", pools=("q",))]
        self.assertEqual(ct.x_por_bloque(lista), [0, 0, 1, 0, 0])

    def test_exito_sin_swap(self):
        self.assertEqual(ct.x_por_bloque([tx(0, "Y", "success", sw=()), tx(1, "X", "revert")]), [0, 0])


class Construir(unittest.TestCase):
    def test_probes(self):
        b = 30000000
        txs = [(b, 0, "Y", ("p",), "success", frozenset({"p"})), (b, 1, "X", ("p",), "probe", frozenset()),
               (b, 2, "X", ("p",), "revert", frozenset())]
        for con, n in ((False, 2), (True, 3)):
            ent, dec, hora, x, y, H = ct.construir(txs, {b: 4}, con)
            self.assertEqual((len(x), H), (n, 1))
        self.assertEqual(list(y), [0, 0, 1])
        self.assertEqual(list(x), [0, 1, 1])
        self.assertEqual(list(dec), [0, 2, 5])


class Robustez(unittest.TestCase):
    def test_bins_y_bloque(self):
        b = 30000000
        txs = [(b, 0, "Y", ("p",), "success", frozenset({"p"})), (b, 1, "X", ("p",), "revert", frozenset()),
               (b, 7, "X", ("p",), "revert", frozenset())]
        ent, dec, hora, x, y, H, blq = ct.construir(txs, {b: 8}, False, 4, True)
        self.assertEqual(list(dec), [0, 0, 3])
        self.assertEqual(list(blq), [b, b, b])

    def test_poda_no_cambia_ic(self):
        rng = np.random.default_rng(5)
        celda, hora, x, y, H = sintetico(rng, 1500, 0.1)
        celda2 = np.concatenate([celda, np.arange(100, 160)])  # celdas de un solo grupo
        hora2 = np.concatenate([hora, np.zeros(60, dtype=int)])
        x2 = np.concatenate([x, np.ones(60, dtype=int)])
        y2 = np.concatenate([y, np.ones(60, dtype=int)])
        a = ct.mh_boot(celda, hora, x, y, H, np.random.default_rng(1), b=100)
        c = ct.mh_boot(celda2, hora2, x2, y2, H, np.random.default_rng(1), b=100)
        self.assertEqual(a, c)


class MH(unittest.TestCase):
    def test_exacto(self):
        # celda 0: tasa1=1/2, tasa0=0/2 (d=.5, w=1); celda 1: tasa1=1, tasa0=.5 (d=.5, w=1)
        celda = np.array([0, 0, 0, 0, 1, 1, 1, 1])
        x = np.array([1, 1, 0, 0, 1, 1, 0, 0])
        y = np.array([1, 0, 0, 0, 1, 1, 1, 0])
        est, k = ct.mh_stat(celda, x, y)
        self.assertAlmostEqual(est, 0.5)
        self.assertEqual(k, 2)

    def test_celda_sin_ambos_grupos_no_cuenta(self):
        celda = np.array([0, 0, 1, 1])
        x = np.array([1, 0, 1, 1])
        y = np.array([1, 0, 0, 1])
        est, k = ct.mh_stat(celda, x, y)
        self.assertEqual(k, 1)
        self.assertAlmostEqual(est, 1.0)

    def test_sin_celdas_validas(self):
        est, k = ct.mh_stat(np.array([0, 1]), np.array([1, 0]), np.array([1, 0]))
        self.assertTrue(np.isnan(est))
        self.assertEqual(k, 0)

    def test_paradoja_de_simpson(self):
        # X se concentra en la celda de tasa alta: cruda > 0, condicionada = 0
        celda = np.array([0] * 10 + [1] * 10)
        x = np.array([1] * 8 + [0] * 2 + [1] * 2 + [0] * 8)
        y = np.array([1] * 10 + [0] * 10)
        est, _ = ct.mh_stat(celda, x, y)
        self.assertAlmostEqual(est, 0.0)
        self.assertGreater(ct.mh_stat(np.zeros(20, dtype=int), x, y)[0], 0.5)


def sintetico(rng, n, efecto, ncel=20, H=40):
    celda = rng.integers(0, ncel, n)
    hora = rng.integers(0, H, n)
    x = (rng.random(n) < 0.3 + 0.4 * (celda / ncel)).astype(np.int64)
    base = 0.1 + 0.5 * (celda / ncel)
    y = (rng.random(n) < base + efecto * x).astype(np.int64)
    return celda, hora, x, y, H


class Calibracion(unittest.TestCase):
    def test_nulo_p_uniforme_y_cobertura(self):
        rng = np.random.default_rng(7)
        ps, cubre = [], 0
        R = 60
        for _ in range(R):
            celda, hora, x, y, H = sintetico(rng, 1500, 0.0)
            p, _ = ct.mh_perm_p(celda, x, y, rng, k=199)
            ps.append(p)
            lo, hi = ct.mh_boot(celda, hora, x, y, H, rng, b=200)
            cubre += lo <= 0 <= hi
        ps = np.array(ps)
        self.assertLess((ps <= 0.05).mean(), 0.15)   # nominal 0.05
        self.assertGreater(cubre / R, 0.85)           # nominal 0.95

    def test_poder_efecto_sembrado(self):
        rng = np.random.default_rng(8)
        celda, hora, x, y, H = sintetico(rng, 4000, 0.10)
        p, obs = ct.mh_perm_p(celda, x, y, rng, k=199)
        self.assertLess(p, 0.02)
        self.assertAlmostEqual(obs, 0.10, delta=0.04)
        lo, hi = ct.mh_boot(celda, hora, x, y, H, rng, b=200)
        self.assertGreater(lo, 0.0)

    def test_efecto_negativo_p_alto(self):
        rng = np.random.default_rng(9)
        celda, hora, x, y, H = sintetico(rng, 4000, -0.10)
        p, obs = ct.mh_perm_p(celda, x, y, rng, k=99)
        self.assertGreater(p, 0.9)


class Analiza(unittest.TestCase):
    def test_claves_y_reproducible(self):
        rng = np.random.default_rng(3)
        celda, hora, x, y, H = sintetico(rng, 800, 0.1)
        ent, dec = celda // 10, celda % 10
        r1 = ct.analiza(ent, dec, hora, x, y, H, np.random.default_rng(1), b=50, k=49)
        r2 = ct.analiza(ent, dec, hora, x, y, H, np.random.default_rng(1), b=50, k=49)
        self.assertEqual(r1, r2)
        self.assertEqual(r1["n"], 800)
        self.assertIn("cond", r1)
        self.assertIn("dec", r1)


if __name__ == "__main__":
    unittest.main()
