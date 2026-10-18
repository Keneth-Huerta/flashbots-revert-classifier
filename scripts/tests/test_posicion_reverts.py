"""Pruebas de posicion_reverts.py con datos sinteticos. Uso: python3 -I -m unittest discover scripts/tests"""
import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analisis"))
import posicion_reverts as pr  # noqa: E402


def tx(idx, ent, kind, pools=("p",), sw=None):
    return (idx, ent, pools, kind, frozenset(pools if sw is None else sw) if kind == "success" else frozenset())


class Grupos(unittest.TestCase):
    def test_sprev(self):
        lista = [tx(0, "X", "revert"), tx(1, "Y", "success"), tx(2, "X", "revert"),
                 tx(3, "Y", "revert"), tx(4, "Z", "probe")]
        g = [x[1] for x in pr.grupos_de_bloque(lista)]
        self.assertEqual(g, ["EA", "ED", "EB", "EA", "EC"])  # el exito de Y no cuenta para Y

    def test_exito_sin_swap_no_cuenta(self):
        lista = [tx(0, "Y", "success", sw=()), tx(1, "X", "revert")]
        self.assertEqual(pr.grupos_de_bloque(lista)[1][1], "EA")

    def test_otro_pool_no_cuenta(self):
        lista = [tx(0, "Y", "success", pools=("q",)), tx(1, "X", "revert", pools=("p",))]
        self.assertEqual(pr.grupos_de_bloque(lista)[1][1], "EA")


class Acumulacion(unittest.TestCase):
    def test_q_y_medias(self):
        b = 30000000
        txs = [(b, 0, "Y", ("p",), "success", frozenset({"p"})), (b, 2, "X", ("p",), "revert", frozenset())]
        q, H = pr.acumular(txs, {b: 4})
        self.assertEqual(H, 1)
        self.assertEqual(q[None]["B"], [(0, 0.5)])
        self.assertEqual(pr.resumen(q[None]["E"])[1], 0.25)
        self.assertEqual(q["X"]["E"], [(0, 0.5)])
        self.assertEqual(q["X"]["A"], [])


class Bootstrap(unittest.TestCase):
    def test_diferencia_constante(self):
        ma = np.array([[10.0, 8.0]] * 6)   # media 0.8
        me = np.array([[10.0, 5.0]] * 6)   # media 0.5
        d, ic = pr.diff_boot(ma, me, np.random.default_rng(1), b=200)
        self.assertAlmostEqual(d, 0.3)
        self.assertAlmostEqual(ic[0], 0.3)
        self.assertAlmostEqual(ic[1], 0.3)

    def test_grupo_vacio(self):
        d, ic = pr.diff_boot(np.zeros((3, 2)), np.ones((3, 2)), np.random.default_rng(1))
        self.assertIsNone(d)
        self.assertIsNone(ic)


if __name__ == "__main__":
    unittest.main()
