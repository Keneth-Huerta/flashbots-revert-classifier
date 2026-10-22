"""Pruebas de robustez_af04.py con datos sinteticos. Uso: python3 -I -m unittest discover scripts/tests"""
import math
import os
import random
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analisis"))
import robustez_af04 as rb  # noqa: E402


def items(seed, n=2000, ents=4):
    r = random.Random(seed)
    out = []
    for _ in range(n):
        e = r.randrange(ents)
        b = r.randrange(10)
        x = int(r.random() < 0.1 + 0.8 * e / ents)
        y = int(r.random() < min(0.95, 0.2 + 0.08 * e + 0.03 * b + 0.04 * x))
        out.append(((e, 3, 0), b, x, y, 0))
    return out


class Descomposicion(unittest.TestCase):
    def test_identidad_exacta(self):
        for seed in range(8):
            d = rb.descomponer([items(seed), items(seed + 100)])
            self.assertLess(d["resid"], 1e-12)
            self.assertAlmostEqual(d["gap"], d["dec"] - d["mh"], places=15)

    def test_simpson_sintetico(self):
        r = random.Random(1)
        it = []
        for _ in range(40000):
            e = r.randrange(2)
            x = int(r.random() < (0.1 if e == 0 else 0.9))
            y = int(r.random() < (0.8 if e == 0 else 0.2) + 0.05 * x)
            it.append(((e, 3, 0), 0, x, y, 0))
        d = rb.descomponer([it])
        self.assertGreater(d["mh"], 0.03)
        self.assertLess(d["dec"], 0)
        self.assertLess(d["comp"], 0)


class DerSimonianLaird(unittest.TestCase):
    def test_ejemplo_a_mano(self):
        h = rb.dersimonian_laird([(0.0, 1.0), (1.0, 1.0), (4.0, 1.0)], [0, 1, 2])
        fe = 5 / 3
        q = fe ** 2 + (1 - fe) ** 2 + (4 - fe) ** 2
        self.assertAlmostEqual(h["fe"], fe)
        self.assertAlmostEqual(h["q"], q)
        self.assertAlmostEqual(h["tau2"], (q - 2) / 2)
        self.assertAlmostEqual(h["slope"], 2.0)
        self.assertAlmostEqual(h["spearman"], 1.0)

    def test_homogeneo(self):
        h = rb.dersimonian_laird([(0.07, 4e-4), (0.07, 9e-4), (0.07, 1e-4)], [0, 1, 2])
        self.assertEqual((h["tau2"], h["i2"]), (0.0, 0.0))
        self.assertIsNone(rb.dersimonian_laird([(0.1, 0.0), (0.2, 0.0)], [0, 1]))


class Rejilla(unittest.TestCase):
    def test_sin_vacias_es_constante(self):
        it = [items(3)]
        pts = rb.rejilla(it, [[]])
        base, _ = rb.estimar([[t for t in it[0] if not t[4]]], "cond")
        self.assertTrue(all(abs(v - base) < 1e-12 for v in pts.values()))

    def test_verdad_dentro_del_rango(self):
        r = random.Random(5)
        todo = items(5, 3000)
        vis, vac = [], []
        for ent, b, x, y, p in todo:
            if r.random() < 0.15:
                vac.append({"ent": ent[0], "bin": b, "y": y, "probe": 0})
            else:
                vis.append((ent, b, x, y, p))
        pts = rb.rejilla([vis], [vac])
        verdad, _ = rb.estimar([todo], "cond")
        fin = [v for v in pts.values() if not math.isnan(v)]
        self.assertLessEqual(min(fin) - 1e-9, verdad)
        self.assertLessEqual(verdad, max(fin) + 1e-9)


if __name__ == "__main__":
    unittest.main()
