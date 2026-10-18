"""Pruebas de descomposicion.py con datos sinteticos. Uso: python3 -I -m unittest discover scripts/tests"""
import os
import sys
import tempfile
import unittest

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analisis"))
import deriva as dv  # noqa: E402
import descomposicion as ds  # noqa: E402
import orden_swap as osw  # noqa: E402
from test_deriva import bloque_inicio_hora  # noqa: E402


def correr(bloques, txs, nh=1):
    ac = ds.Acum(bloques, txs)
    res = dv.construir(bloques, txs, gancho=ac)
    return res, ac.v.sum(0)


class Acumulador(unittest.TestCase):
    def test_una_entidad_a_mano(self):
        # B toca p en b0 y b1, no en b2: s = (1, 1, 0); n = 3; C_-b = 0.5
        # E_nulo = 1 - (1 - 2/3) = 2/3; e_f = 1/3; cota = (2/3)(1 - 0.5) = 1/3 (igualdad)
        b0 = bloque_inicio_hora()
        txs = [(b0, "A", ("p",), True), (b0, "B", ("p",), False), (b0 + 1, "B", ("p",), False)]
        _, t = correr([b0, b0 + 1, b0 + 2], txs)
        self.assertEqual(t[ds.C_M], 1)
        self.assertAlmostEqual(t[ds.C_INV], 1 / 3)
        self.assertAlmostEqual(t[ds.C_ENULL], 2 / 3)
        self.assertAlmostEqual(t[ds.C_EF], 1 / 3)
        self.assertAlmostEqual(t[ds.C_RHS], 1 / 3)
        self.assertEqual(t[ds.C_VIOL], 0)

    def test_redundancia_n2_k2(self):
        # Contraejemplo del fundamento: n=2, B y C tocan p solo en b0: E_nulo = 1-(1/2)^2=3/4,
        # e = 1/4; cota (1-1/2)(1 - 0) = 1/2; k_p = 2
        b0 = bloque_inicio_hora()
        txs = [(b0, "A", ("p",), True), (b0, "B", ("p",), False), (b0, "C", ("p",), False)]
        _, t = correr([b0, b0 + 1], txs)
        self.assertAlmostEqual(t[ds.C_ENULL], 0.75)
        self.assertAlmostEqual(t[ds.C_EF], 0.25)
        self.assertAlmostEqual(t[ds.C_RHS], 0.5)
        self.assertEqual(t[ds.C_KSUM], 2)
        self.assertEqual(t[ds.C_NK2], 1)
        self.assertEqual(t[ds.C_VIOL], 0)
        # P(ningun u toca p) = 1/2 >= (1/2)^2: la asociacion se cumple
        self.assertEqual(t[ds.C_NCOND], 1)

    def test_violacion_sin_asociacion(self):
        # n=4; B sin p en {1,2}, C sin p en {1,3}... construccion con tres entidades:
        # B toca p en b0,b1; C en b2,b3 -> nunca coinciden. Ningun bloque libre de p:
        # P(cap N) = 0 < (1/2)(1/2): asociacion falla (correlacion negativa).
        b0 = bloque_inicio_hora()
        txs = [(b0, "A", ("p",), True), (b0, "B", ("p",), False), (b0 + 1, "B", ("p",), False),
               (b0 + 2, "C", ("p",), False), (b0 + 3, "C", ("p",), False)]
        _, t = correr([b0, b0 + 1, b0 + 2, b0 + 3], txs)
        self.assertEqual(t[ds.C_NCOND], 0)

    def test_hora_de_un_bloque_aporta_cero(self):
        b0 = bloque_inicio_hora()
        txs = [(b0, "A", ("p",), True), (b0, "B", ("p",), False)]
        _, t = correr([b0], txs)
        self.assertEqual(t[ds.C_INV], 1.0)  # 1 - E[1/n] = 0
        self.assertEqual(t[ds.C_N1], 0.0)  # excluido de P2
        self.assertAlmostEqual(t[ds.C_ENULL], t[ds.C_S0])  # E_nulo = s => e = 0

    def test_ponderado_por_intento(self):
        b0 = bloque_inicio_hora()
        h2 = b0 + 3600 // 2 * 2  # otra hora
        txs = [(b0, "A", ("p",), True), (b0, "A", ("p",), True), (b0, "B", ("p",), False),
               (h2, "A", ("p",), True), (h2 + 1, "B", ("p",), False)]
        _, t = correr([b0, h2, h2 + 1], txs)
        self.assertEqual(t[ds.C_M], 3)
        self.assertAlmostEqual(t[ds.C_INV], 2 * 1.0 + 0.5)  # n=1 (m=2) y n=2


class Celdas(unittest.TestCase):
    def test_celda_rel(self):
        self.assertEqual(ds.celda_rel(0, 10, 5), 0)
        self.assertEqual(ds.celda_rel(9, 10, 5), 4)
        self.assertEqual(ds.celda_rel(5, 10, 5), 2)
        self.assertEqual(ds.celda_rel(3, 0, 5), 4)  # tamano degenerado no revienta

    def test_nulo_en_celdas_conserva_orden_entre_celdas(self):
        # bloque de 10 tx: exito en 1 (celda 0 de 5), revert en 8 (celda 4): en el nulo por
        # celdas el exito siempre queda antes; con el nulo de bloque entero, no.
        w = {"p": 1.0}
        txs = [(100, 1, "B", ("p",), "success", frozenset({"p"})),
               (100, 8, "A", ("p",), "revert", frozenset())]
        d = osw.preparar(txs, w)
        dc = ds.con_nulo_en_celdas(d, [100], {100: 10}, 5)
        rng = np.random.default_rng(3)
        i = int(np.argmin(d.rank))  # el exito (tx_index 1)
        for _ in range(50):
            r = osw._rango_nulo(dc, rng)
            self.assertLess(r[i], r[1 - i])
        libre = {tuple(osw._rango_nulo(d, rng)) for _ in range(50)}
        self.assertEqual(len(libre), 2)


class Auxiliares(unittest.TestCase):
    def test_leer_E_secciones(self):
        fila = "S1  201  80  12649  0.974  0.950 [0.941,0.960]  0.943 [0.940,0.947]  {e} [0.003,0.014]  0.001  0.004\n"
        txt = "x | permutations 500 | y\n== metric binary\nhdr\n" + fila.format(e="0.007") + \
              "== metric weighted\nhdr\n" + fila.format(e="0.022")
        with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as fh:
            fh.write(txt)
        try:
            self.assertAlmostEqual(ds.leer_E(fh.name, "weighted")["S1"][0], 0.022)
            self.assertAlmostEqual(ds.leer_E(fh.name, "binary")["S1"][0], 0.007)
            self.assertEqual(ds.leer_E(fh.name, "weighted")["S1"][3], 12649)
            self.assertAlmostEqual(ds.leer_E(fh.name, "weighted")["S1"][4], 0.943)
            self.assertEqual(ds.leer_E(fh.name, "weighted")["S1"][7], 500)
        finally:
            os.unlink(fh.name)

    def test_cociente(self):
        pred, ro, ic = ds.cociente_p1(0.4, 0.5, (0.02, 0.01, 0.03), (0.05, 0.04, 0.06))
        self.assertAlmostEqual(pred, 0.8)
        self.assertAlmostEqual(ro, 0.4)
        self.assertLess(ic[0], ro)
        self.assertGreater(ic[1], ro)

    def test_bootstrap_conserva_total_con_una_hora(self):
        v = np.array([[1.0, 2, 3, 4, 5, 6]])
        bo = ds.boot_horas(v, np.random.default_rng(1), b=5)
        self.assertTrue(np.allclose(bo, np.tile(v, (5, 1))))


if __name__ == "__main__":
    unittest.main()
