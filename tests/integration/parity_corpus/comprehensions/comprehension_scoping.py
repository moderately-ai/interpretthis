# Pins: list comprehension has its own scope — the loop variable
# does not leak. CPython 3 behaviour; CPython 2 leaked, which is a
# common gotcha for porting.
x = 100
result = [x for x in range(3)]
print(result)
print(x)
