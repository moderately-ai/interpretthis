# complex arithmetic: +, -, *, /, ** and unary +/-, mixed with int/float, plus
# equality and hashing. // and % raise TypeError; div-by-zero raises
# ZeroDivisionError; ordering raises TypeError.
print(1 + 2j)
print(1.5 - 2j)
print(2j * 3j)
print((1 + 2j) * (3 + 4j))
print((1 + 2j) / (1 - 1j))
print(2j ** 2)
print((1 + 1j) ** 2)
print(-3j)
print(-(1 + 2j))
print(+3j)
print(1j + 1)

# Equality (value, with real/imag) and hashing parity with int/float.
print(complex(0, 2) == 2j)
print((2 + 0j) == 2)
print(1j == 1)
print(hash(1 + 0j) == hash(1))     # real-valued complex hashes like the int
print(len({1, 1 + 0j, 1.0}))       # all equal + same hash -> 1
print(len({2j, 2j}))
print(len({1j, 2j, 1 + 1j}))
print({2j: "a"}[2j])

# Unsupported operations raise.
try:
    (1j) // 2
except TypeError:
    print("floordiv TypeError")
try:
    1j % 2
except TypeError:
    print("mod TypeError")
try:
    1j / 0
except ZeroDivisionError:
    print("ZeroDivisionError")
try:
    1j < 2j
except TypeError:
    print("lt TypeError")
