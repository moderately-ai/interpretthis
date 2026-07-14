# complex() constructor: no args, a number, two numbers (real + imag*1j,
# including complex arguments), bool coercion, and string parsing (with parens,
# exponents, bare 'j'). Malformed strings and str+number raise.
print(complex())
print(complex(3))
print(complex(1, 2))
print(complex(1.5, -2.5))
print(complex(2j))
print(complex(1 + 2j, 3 + 4j))     # a + b*1j
print(complex("1+2j"))
print(complex("3j"))
print(complex("-1.5e2-2j"))
print(complex("(1+2j)"))
print(complex("inf"))
print(complex("1"))
print(complex("j"))
print(complex("1+j"))
print(complex(True, False))

try:
    complex("bad")
except ValueError:
    print("ValueError")
try:
    complex("1", 2)
except TypeError:
    print("TypeError")
