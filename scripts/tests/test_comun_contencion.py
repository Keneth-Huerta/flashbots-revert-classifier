"""Pruebas de comun_contencion con filas sinteticas. Uso: python3 -I -m unittest discover scripts/tests"""
import gzip
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analisis"))
import comun_contencion as cc  # noqa: E402


def fila(to, from_, kind="revert", pools=("p1",), block=1, idx=0):
    return cc.Fila(block, idx, f"0x{to}{from_}{idx}{block}", from_, to, kind, False, tuple(pools))


class Universo(unittest.TestCase):
    def test_umbral_de_cinco_fallidas_con_pools(self):
        self.assertIn("A", cc.universo_bots([fila("A", "x", idx=i) for i in range(5)]))
        self.assertNotIn("A", cc.universo_bots([fila("A", "x", idx=i) for i in range(4)]))

    def test_fallidas_sin_pools_no_cuentan(self):
        filas = [fila("A", "x", pools=(), idx=i) for i in range(9)]
        filas += [fila("A", "x", idx=i + 9) for i in range(4)]
        self.assertNotIn("A", cc.universo_bots(filas))

    def test_proporcion_de_fallidas(self):
        filas = [fila("A", "x", idx=i) for i in range(5)]
        filas += [fila("A", "x", kind="success", idx=i + 5) for i in range(5)]
        self.assertIn("A", cc.universo_bots(filas))  # 5 >= 0.5 * 10
        filas.append(fila("A", "x", kind="success", idx=99))
        self.assertNotIn("A", cc.universo_bots(filas))  # 5 < 0.5 * 11


class Entidades(unittest.TestCase):
    def test_componentes_por_from(self):
        filas = [fila("B", "s1"), fila("C", "s1"), fila("C", "s2"), fila("D", "s2"),
                 fila("E", "s3"), fila("Z", "s1")]
        ent = cc.entidades(filas, {"B", "C", "D", "E"})
        self.assertEqual(ent["B"], "B")
        self.assertEqual({ent["B"], ent["C"], ent["D"]}, {"B"})
        self.assertEqual(ent["E"], "E")

    def test_sin_from_compartido(self):
        ent = cc.entidades([fila("B", "s1"), fila("C", "s2")], {"B", "C"})
        self.assertNotEqual(ent["B"], ent["C"])


class Entrada(unittest.TestCase):
    def test_split_invalido(self):
        with self.assertRaises(ValueError):
            cc.cargar_muestra("no_importa.csv", "all")

    def test_raw_fuera_del_split_se_rechaza(self):
        with self.assertRaises(PermissionError):
            cc.leer_raw("/no/existe", 5, {1, 2})

    def test_tiene_raw_exige_trazas_y_receipts(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertFalse(cc.tiene_raw(d, 7))
            for sufijo in ("trace", "receipts"):
                ruta = os.path.join(d, f"8.{sufijo}.json.gz")
                with gzip.open(ruta, "wt") as f:
                    f.write('{"result": []}')
            self.assertTrue(cc.tiene_raw(d, 8))
            with gzip.open(os.path.join(d, "9.trace.json.gz"), "wt") as f:
                f.write('{"result": []}')
            self.assertFalse(cc.tiene_raw(d, 9))  # falta receipts

    def test_timestamp(self):
        self.assertEqual(cc.block_timestamp(10), 1686789367)


if __name__ == "__main__":
    unittest.main()
