"""Pruebas de orden_swap.py con datos sinteticos. Uso: python3 -I -m unittest discover scripts/tests"""
import os
import random
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analisis"))
import comun_contencion as cc  # noqa: E402
import orden_swap as osw  # noqa: E402

SW = next(iter(cc.SWAP_TOPICS))


def exito(b, idx, ent, pools, swaps=None):
    return (b, idx, ent, pools, "success", frozenset(pools if swaps is None else swaps))


def fallo(b, idx, ent, pools, kind="revert"):
    return (b, idx, ent, pools, kind, frozenset())


class Receipts(unittest.TestCase):
    def test_solo_exitosas_con_swap(self):
        rc = [
            {"status": "0x1", "transactionHash": "0xAA", "logs": [{"address": "0xP1", "topics": [SW]}]},
            {"status": "0x0", "transactionHash": "0xbb", "logs": [{"address": "0xp2", "topics": [SW]}]},
            {"status": "0x1", "transactionHash": "0xcc", "logs": [{"address": "0xp3", "topics": ["0x1"]}]},
        ]
        self.assertEqual(osw.swaps_por_tx(rc), {"0xaa": frozenset({"0xp1"})})


class ACuenta(unittest.TestCase):
    def test_s_prev_a_mano(self):
        w = {"p1": 0.5, "p2": 0.5}
        txs = [fallo(1, 0, "B", ("p1",)),            # antes del exito: 0
               exito(1, 1, "A", ("p1",)),
               fallo(1, 2, "B", ("p1", "p2")),       # despues: p1 de 2 pools -> 0.5
               fallo(1, 3, "A", ("p1",)),            # misma entidad: 0
               fallo(1, 4, "C", ("p2",), "probe")]   # p2 sin exito: 0
        d = osw.preparar(txs, w)
        self.assertEqual(list(d.den[0]), [3, 1])  # reverts B,B,A ; probe C
        res = osw.analizar(d, k=5, b=0, rng=np.random.default_rng(1))
        # reverts: (0 + 0.5 + s(A)=0) / 3
        self.assertAlmostEqual(res["revert"]["obs"], 0.5 / 3)
        self.assertAlmostEqual(res["probe"]["obs"], 0.0)
        self.assertEqual(res["todos"]["n"], 4)

    def test_exito_sin_swap_no_es_candidato(self):
        w = {"p1": 1.0}
        txs = [exito(1, 0, "A", ("p1",), swaps=()), fallo(1, 1, "B", ("p1",))]
        d = osw.preparar(txs, w)
        self.assertEqual(osw.analizar(d, k=3, b=0)["revert"]["obs"], 0.0)

    def test_nulo_a_mano(self):
        # un exito y un fallo, un pool: P(fallo despues) = 1/2 bajo permutacion
        w = {"p1": 1.0}
        txs = [exito(b, 0, "A", ("p1",)) for b in range(1, 41)]
        txs += [fallo(b, 1, "B", ("p1",)) for b in range(1, 41)]
        res = osw.analizar(osw.preparar(txs, w), k=400, b=0, rng=np.random.default_rng(3))
        self.assertAlmostEqual(res["revert"]["obs"], 1.0)
        self.assertAlmostEqual(res["revert"]["nulo"], 0.5, delta=0.03)


def sintetico(semilla, despues, bloques=60):
    r = random.Random(semilla)
    txs = []
    for b in range(1, bloques + 1):
        idx = list(range(5))
        if despues:
            e, fi = 0, [1, 2, 3, 4]
        else:
            r.shuffle(idx)
            e, fi = idx[0], idx[1:]
        txs.append(exito(b, e, "A", ("p1",)))
        for k, i in enumerate(fi):
            txs.append(fallo(b, i, "BC"[k % 2], ("p1",), "revert" if k % 2 == 0 else "probe"))
    return txs


class Calibracion(unittest.TestCase):
    def test_sin_efecto_p_no_pequeno(self):
        ps = []
        for s in range(60):
            d = osw.preparar(sintetico(s, False), {"p1": 1.0})
            ps.append(osw.analizar(d, k=200, b=0, rng=np.random.default_rng(s))["todos"]["p"])
        # bajo H0 el p es ~uniforme: P(p<0.05) ~ 0.05 (50 semillas dieron 5%); cota holgada
        self.assertLessEqual(np.mean(np.array(ps) < 0.05), 0.15)
        self.assertGreater(np.mean(ps), 0.35)
        self.assertLess(np.mean(ps), 0.65)

    def test_con_efecto_se_detecta(self):
        d = osw.preparar(sintetico(0, True), {"p1": 1.0})
        r = osw.analizar(d, k=300, b=100, rng=np.random.default_rng(0))["todos"]
        self.assertAlmostEqual(r["obs"], 1.0)
        self.assertGreater(r["exceso"], 0.4)
        self.assertLess(r["p"], 0.01)
        self.assertGreater(r["ic"][0], 0.3)


class Pesos(unittest.TestCase):
    def test_pesos_semana(self):
        filas = [cc.Fila(1, 0, "h1", "x", "A", "revert", False, ("p1", "p2")),
                 cc.Fila(1, 1, "h2", "x", "A", "revert", False, ("p1",)),   # mismo paquete
                 cc.Fila(1, 2, "h3", "y", "B", "revert", False, ("p1",)),
                 cc.Fila(2, 0, "h4", "y", "B", "revert", False, ("p1",))]
        w = osw.pesos_semana(filas, lambda r: r.to)
        self.assertEqual(w, {"p1": 1 / 3, "p2": 1.0})


if __name__ == "__main__":
    unittest.main()
