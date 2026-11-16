import unittest

from calc import add, sub, mul, div

class TestCalc(unittest.TestCase):
    def test_add(self):
        self.assertEqual(add(1, 2), 3)
        self.assertAlmostEqual(add(1.5, 2.5), 4.0)

    def test_sub(self):
        self.assertEqual(sub(5, 3), 2)
        self.assertAlmostEqual(sub(5.5, 2.5), 3.0)

    def test_mul(self):
        self.assertEqual(mul(4, 3), 12)
        self.assertAlmostEqual(mul(2.5, 4), 10.0)

    def test_div(self):
        self.assertEqual(div(10, 2), 5)
        self.assertAlmostEqual(div(7, 2), 3.5)
        with self.assertRaises(ZeroDivisionError):
            div(1, 0)

if __name__ == "__main__":
    unittest.main()
