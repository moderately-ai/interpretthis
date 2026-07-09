# divmod(a, b) returns the tuple (a // b, a % b) — the ORDER matters for
# user code that unpacks. Plus the standard "modulo sign follows divisor"
# rule for negative operands.
print(divmod(10, 3))
print(divmod(10, -3))
print(divmod(-10, 3))
print(divmod(-10, -3))
print(divmod(7, 2))
print(divmod(2.5, 1))
q, r = divmod(17, 5)
print(q, r)
