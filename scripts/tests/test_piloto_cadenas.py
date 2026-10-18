import os, sys, unittest
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "ingesta"))
import piloto_cadenas as p


class T(unittest.TestCase):
    def test_determinista(self):
        a = p.sortear(20261017, "opt-mainnet", 10_000_000, 43200)
        b = p.sortear(20261017, "opt-mainnet", 10_000_000, 43200)
        self.assertEqual(a, b)
        self.assertEqual(len(set(a)), 30)
        self.assertTrue(all(10_000_000 - 43200 * 7 <= x < 10_000_000 for x in a))
        self.assertNotEqual(a, p.sortear(20261017, "unichain-mainnet", 10_000_000, 43200))

    def test_mascara(self):
        k = "SECRETKEY123"
        u = f"https://opt-mainnet.g.alchemy.com/v2/{k}"
        for t in (f"fallo en {u} timeout", f"x {k} y", "error https://h/v2/OTRA-llave."):
            m = p.enmascarar(t, k)
            self.assertNotIn(k, m); self.assertNotIn("OTRA-llave", m)

    def test_cliente_no_expone(self):
        c = p.Cliente("opt-mainnet", "KK")
        self.assertEqual(p.enmascarar(RuntimeError(c.url), "KK"), "https://opt-mainnet.g.alchemy.com/v2/***")


if __name__ == "__main__":
    unittest.main()
