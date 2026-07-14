# int / float / bool expose the numeric-tower attributes .real / .imag (and int
# .numerator / .denominator). Regression: these raised AttributeError — only the
# method forms (.conjugate()) worked.
print((5).real, (5).imag, (5).numerator, (5).denominator)
print((-3).real, (-3).imag)
print((2**70).real, (2**70).numerator, (2**70).denominator)
print((1.5).real, (1.5).imag)
print((2.0).real, (2.0).imag)
print(True.real, True.imag, True.numerator, True.denominator)
print(False.real, False.numerator)
print((5).conjugate())          # int method still works
print((1.5).conjugate())        # float methods
print((2.0).is_integer(), (1.5).is_integer())
print((1.5).as_integer_ratio())
print((0.5).as_integer_ratio())
print((255.0).hex())
print((1.5).hex())

try:
    (5).nope
except AttributeError:
    print("AttributeError")
try:
    (1.5).nope
except AttributeError:
    print("AttributeError")
