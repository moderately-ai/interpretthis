# CPython's `//` and `%` use floor division semantics (result follows
# the sign of the divisor), not C-style truncation toward zero.
print(-7 // 3)   # CPython: -3 (floors toward negative infinity)
print(-7 % 3)    # CPython: 2 (sign of divisor)
print(7 // -3)   # CPython: -3
print(7 % -3)    # CPython: -2
print(divmod(-7, 3))
print(divmod(7, -3))
print(divmod(-7, -3))
