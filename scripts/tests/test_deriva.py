"""Pruebas de deriva.py con datos sinteticos. Uso: python3 -I -m unittest discover scripts/tests"""
import os
import random
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analisis"))
import comun_contencion as cc  # noqa: E402
import deriva as dv  # noqa: E402


def bloque_inicio_hora():
    """Primer bloque cuyo timestamp cae en el segundo 1 de una hora (para que b0, b0+1 y
    b0+300 queden en la misma hora)."""
    b = 1
    while cc.block_timestamp(b) % 3600 != 1:
        b += 1
    return b


class Tramos(unittest.TestCase):
    def test_bordes(self):
        self.assertEqual(dv.indice_tramo(0), 0)
        self.assertEqual(dv.indice_tramo(2), 1)
        self.assertEqual(dv.indice_tramo(119), 1)
        self.assertEqual(dv.indice_tramo(120), 2)
        self.assertEqual(dv.indice_tramo(600), 4)
        self.assertEqual(dv.indice_tramo(3599), 6)
        self.assertEqual(dv.indice_tramo(3600), 7)
        self.assertEqual(dv.indice_tramo(86400), 9)


class ACuenta(unittest.TestCase):
    def test_curva_a_mano(self):
        b0 = bloque_inicio_hora()
        txs = [(b0, "A", ("p1", "p2"), True),
               (b0, "B", ("p1",), False),
               (b0 + 1, "B", ("p2",), False),
               (b0 + 300, "B", ("p1", "p2"), False)]
        # n_p1 = 3 (A@b0, B@b0, B@b0+300), n_p2 = 3 -> w = 1/3 cada uno
        res = dv.construir([b0, b0 + 1, b0 + 300], txs)
        c = dv.curva(res)
        self.assertAlmostEqual(c[0], 0.5)  # solo B toca p1 en b0
        self.assertAlmostEqual(c[1], 0.5)  # b0+1, 2 s: B toca p2
        self.assertAlmostEqual(c[4], 1.0)  # b0+300, 600 s = 10-20m: B toca p1 y p2
        self.assertTrue(np.isnan(c[2]))
        self.assertAlmostEqual(c[dv.IDX_HORA], 0.75)  # (0.5 + 1.0) / 2
        self.assertEqual(res["n_int"], 1)
        self.assertEqual(list(res["n_pares"][[0, 1, 4, dv.IDX_HORA]]), [1, 1, 1, 2])

    def test_entidad_propia_no_cuenta(self):
        b0 = bloque_inicio_hora()
        txs = [(b0, "A", ("p1",), True), (b0 + 300, "A", ("p1",), False)]
        c = dv.curva(dv.construir([b0, b0 + 300], txs))
        self.assertAlmostEqual(c[0], 0.0)
        self.assertAlmostEqual(c[4], 0.0)

    def test_c0_es_contend(self):
        # b0: A falla en (p1,p2); B toca p1; n_p1=2, n_p2=1 -> w = 1/2, 1 ; s = 0.5/1.5
        b0 = bloque_inicio_hora()
        txs = [(b0, "A", ("p1", "p2"), True), (b0, "B", ("p1",), False)]
        c = dv.curva(dv.construir([b0], txs))
        self.assertAlmostEqual(c[0], 0.5 / 1.5)

    def test_contra_fuerza_bruta(self):
        r = random.Random(7)
        bloques = sorted(r.sample(range(1000, 400000, 7), 14))
        pools = [f"p{i}" for i in range(6)]
        txs = []
        for b in bloques:
            for ent in "ABC":
                if r.random() < 0.7:
                    ps = tuple(r.sample(pools, r.randint(1, 3)))
                    txs.append((b, ent, ps, r.random() < 0.6))
        c = dv.curva(dv.construir(bloques, txs))
        paq = {}
        for b, ent, ps, _ in txs:
            paq.setdefault((b, ent), set()).update(ps)
        n_p = {}
        for ps in paq.values():
            for p in ps:
                n_p[p] = n_p.get(p, 0) + 1
        suma, cuenta = np.zeros(dv.NT), np.zeros(dv.NT)
        for b, ent, ps, f in txs:
            if not f:
                continue
            tot = sum(1 / n_p[p] for p in ps)
            for b2 in bloques:
                comp = sum(1 / n_p[p] for p in ps
                           if any(e2 != ent and p in q for (bb, e2), q in paq.items() if bb == b2))
                i = dv.indice_tramo(abs(cc.block_timestamp(b) - cc.block_timestamp(b2)))
                suma[i] += comp / tot
                cuenta[i] += 1
        for i in range(dv.NT):
            if cuenta[i]:
                self.assertAlmostEqual(c[i], suma[i] / cuenta[i], places=9)
            else:
                self.assertTrue(np.isnan(c[i]))

    def test_primer_tramo_y_bootstrap(self):
        n = np.zeros(dv.NT + 1)
        n[2], n[3], n[4] = 199, 200, 500
        self.assertEqual(dv.primer_tramo(n), 3)
        n[3] = 0
        self.assertEqual(dv.primer_tramo(n), 4)
        n[:] = 0
        self.assertIsNone(dv.primer_tramo(n))
        # bootstrap: IC contiene la estimacion puntual
        b0 = bloque_inicio_hora()
        txs = [(b0 + k, e, ("p1",), e == "A") for k in (0, 1, 300) for e in "AB"]
        res = dv.construir([b0, b0 + 1, b0 + 300], txs)
        bo = dv.bootstrap(res, 4, b=50, rng=np.random.default_rng(1))
        self.assertIsNotNone(bo["E_hora"])


class Primario(unittest.TestCase):
    def test_lee_E(self):
        import tempfile
        linea = ("S1          807   116   47289  0.985  0.765 [0.754,0.776]   0.714 [0.711,0.716]"
                 "   0.051 [0.046,0.056]   0.0010   0.0040\n")
        with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
            f.write("encabezado\n" + linea)
        try:
            self.assertEqual(dv.leer_primario(f.name), {"S1": 0.051})
        finally:
            os.unlink(f.name)


if __name__ == "__main__":
    unittest.main()
