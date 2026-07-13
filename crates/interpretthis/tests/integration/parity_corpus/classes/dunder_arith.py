# Pins: user-class arithmetic slots (__add__, __sub__, __mul__,
# __truediv__, __floordiv__, __mod__) and the reflected __radd__
# path. Customer pattern: value types with custom math (Vector,
# Money, units).
class V:
    def __init__(self, n):
        self.n = n
    def __repr__(self):
        return f"V({self.n})"
    def __add__(self, other):
        return V(self.n + (other.n if isinstance(other, V) else other))
    def __radd__(self, other):
        return V(self.n + other)
    def __sub__(self, other):
        return V(self.n - (other.n if isinstance(other, V) else other))
    def __mul__(self, other):
        return V(self.n * (other.n if isinstance(other, V) else other))
    def __truediv__(self, other):
        return V(self.n / (other.n if isinstance(other, V) else other))
    def __floordiv__(self, other):
        return V(self.n // (other.n if isinstance(other, V) else other))
    def __mod__(self, other):
        return V(self.n % (other.n if isinstance(other, V) else other))

a, b = V(10), V(3)
print(a + b)
print(a - b)
print(a * b)
print(a / b)
print(a // b)
print(a % b)
print(5 + a)
print(a + 7)
