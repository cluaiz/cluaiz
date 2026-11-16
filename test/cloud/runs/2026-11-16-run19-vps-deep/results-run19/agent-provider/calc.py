"""Simple calculator module.

Provides four basic arithmetic functions.
"""


def add(a, b):
    """Return the sum of *a* and *b*.

    Parameters
    ----------
    a, b: int or float
        Operands.

    Returns
    -------
    int or float
        The result of *a* + *b*.
    """
    return a + b


def sub(a, b):
    """Return *a* minus *b*.
    """
    return a - b


def mul(a, b):
    """Return *a* times *b*.
    """
    return a * b


def div(a, b):
    """Return *a* divided by *b*.

    Raises
    ------
    ZeroDivisionError
        If *b* is zero.
    """
    if b == 0:
        raise ZeroDivisionError("division by zero")
    return a / b

if __name__ == "__main__":
    print("Simple calculator module.")
