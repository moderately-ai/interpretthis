# `x in range(...)` uses the O(1) step-aware modular check (types::
# range_contains), not a linear scan. Pins the boundary + step cases.
print(0 in range(5))
print(5 in range(5))            # exclusive upper
print(4 in range(5))
print(2 in range(0, 10, 2))     # step-aligned
print(3 in range(0, 10, 2))     # not step-aligned
print(-1 in range(5))
print(1.0 in range(5))          # CPython: True (1.0 == 1 numerically)
print(1.5 in range(5))          # CPython: False (non-integer-valued)
print(9 in range(10, 0, -1))
