c = 3 + 4j
print(c.__abs__())
print(c.__neg__(), c.__pos__())
print(c.__bool__(), (0j).__bool__())
print(c.__add__(1), c.__sub__(1j), c.__mul__(2))
print(c.__truediv__(2), c.__pow__(2))
print((1 + 1j).__add__(2 + 2j))
print(c.conjugate(), c.real, c.imag)
print(complex(0).__bool__(), complex(0, 0.1).__bool__())
print((2 + 0j).__abs__())
# consistency: hasattr agrees with a successful call
for m in ["__abs__", "__neg__", "__add__", "conjugate", "real", "imag", "__bool__"]:
    print(m, hasattr(c, m))
