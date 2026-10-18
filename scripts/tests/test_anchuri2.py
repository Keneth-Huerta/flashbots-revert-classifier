"""Pruebas de anchuri2.py con datos sinteticos. Uso: python3 -I -m unittest discover scripts/tests"""
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "analisis"))
import anchuri2 as an  # noqa: E402

V2, V3, VIEW = "0x022c0d9f", "0x128acb08", "0x3850c7bd"


def llamada(to, sel, hijos=()):
    return {"to": to, "input": sel + "00", "calls": list(hijos)}


class Atribucion(unittest.TestCase):
    def test_ultimo_swap_en_preorden(self):
        raiz = {"to": "0xbot", "input": "0xa1d60127", "calls": [
            llamada("0xp1", V2),
            llamada("0xrouter", "0x12345678", [llamada("0xP2", V3), llamada("0xp3", VIEW)]),
        ]}
        self.assertEqual(an.ultimo_swap(raiz, ["0xp1", "0xp2", "0xp3"]), "0xp2")
        # un pool fuera de P(f) no cuenta; una vista tampoco
        self.assertEqual(an.ultimo_swap(raiz, ["0xp1", "0xp3"]), "0xp1")
        self.assertIsNone(an.ultimo_swap(raiz, ["0xp3"]))
        self.assertIsNone(an.ultimo_swap({"to": "0xbot", "input": V2}, ["0xbot"]))

    def test_caso_de_un_pool(self):
        w = {"0xp1": 0.2, "0xp2": 0.9}
        self.assertEqual(an.atribuir(("0xp1", "0xp2"), w, "0xp1"), ("0xp1", "swap"))
        self.assertEqual(an.atribuir(("0xp1", "0xp2"), w, None), ("0xp2", "maxw"))
        self.assertEqual(an.atribuir(("0xp1",), w, None), ("0xp1", "maxw"))
        # desempate determinista por direccion
        self.assertEqual(an.atribuir(("0xb", "0xa"), {"0xa": 1, "0xb": 1}, None)[1], "maxw")


class Conteo(unittest.TestCase):
    def test_a_mano(self):
        # (pool, bloque, unidad, exito)
        x = [("p", 1, "A", False),            # par (p,1): solo A -> 1 contrato
             ("q", 1, "A", False),            # par (q,1): A revert + B exito -> 2
             ("q", 1, "B", True),
             ("r", 1, "B", True),             # sin revert: no es par
             ("p", 2, "A", False), ("p", 2, "A", False),   # mismo contrato repetido -> 1
             ("p", 3, "A", False), ("p", 3, "B", False)]   # 2 reverts distintos
        n, uno, uno_rev = an.contar(x)
        self.assertEqual((n, uno), (4, 2))
        self.assertEqual(uno_rev, 3)  # sin exitos, (q,1) pasa a tener un solo contrato

    def test_entidad_junta_contratos(self):
        # dos contratos de la misma entidad: unidad entidad -> 1, unidad contrato -> 2
        x = [("p", 1, "c1", False), ("p", 1, "c2", False)]
        self.assertEqual(an.contar(x)[1], 0)
        self.assertEqual(an.contar([("p", 1, "E", False), ("p", 1, "E", False)])[1], 1)


if __name__ == "__main__":
    unittest.main()
