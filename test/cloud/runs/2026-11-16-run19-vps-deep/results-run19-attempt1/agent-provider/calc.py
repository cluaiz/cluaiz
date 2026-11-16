def add(a, b):
    """Return the sum of a and b."""
    return a + b

def sub(a, b):
    """Return the difference of a and b (a - b)."""
    return a - b

def mul(a, b):
    """Return the product of a and b."""
    return a * b

def div(a, b):
    """Return the division of a by b.

    Raises ZeroDivisionError if b is zero.
    """
    if b == 0:
        raise ZeroDivisionError("division by zero")
    return a / b
