# Pins: user-class __call__ makes an instance callable. Customer
# pattern: factory objects, partial-application helpers, configured
# strategy objects.
class Multiplier:
    def __init__(self, n):
        self.n = n
    def __call__(self, x):
        return x * self.n
    def __repr__(self):
        return f"Multiplier({self.n})"

double = Multiplier(2)
triple = Multiplier(3)
print(double(5))
print(triple(5))
print([f(10) for f in [double, triple]])
