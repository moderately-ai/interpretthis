import cmath
print(cmath.sqrt(-1))
print(cmath.phase(1j))
print(round(cmath.pi, 5))
print(cmath.polar(1 + 1j))
print(cmath.rect(1, 0))
print(abs(cmath.exp(1j * cmath.pi) + 1) < 1e-10)
print(cmath.isnan(complex(float("nan"), 0)))
print(cmath.isinf(complex(float("inf"), 0)))
