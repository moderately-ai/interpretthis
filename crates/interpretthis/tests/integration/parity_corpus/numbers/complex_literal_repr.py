# complex literals (`3j`) construct a distinct complex value with CPython's
# repr/str formatting and type. Regression: `3j` was stored as its real part
# (a float), losing the imaginary component entirely.
print(3j)
print(2.5j)
print(0j)
print(1000j)
print(repr(3j))
print(str(2.5j))
print(type(3j).__name__)
print(bool(0j), bool(3j))
print(f"value={3j}")
print(1e300j)          # scientific-notation component
print(0.001j)
