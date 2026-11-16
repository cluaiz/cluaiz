import unittest
from calc import add, sub, mul, div

class TestCalc(unittest.TestCase):
    def test_add(self):
        self.assertEqual(add(1, 2), 3)
        self.assertEqual(add(-1, 1), 0)

    def test_sub(self):
        self.assertEqual(sub(5, 3), 2)
        self.assertEqual(sub(3, 5), -2)

    def test_mul(self):
        self.assertEqual(mul(4, 5), 20)
        self.assertEqual(mul(-2, 3), -6)

    def test_div(self):
        self.assertEqual(div(10, 2), 5)
        self.assertEqual(div(9, 3), 3)
        with self.assertRaises(ZeroDivisionError):
            div(1, 0)

if __name__ == '__main__':
    unittest.main()
