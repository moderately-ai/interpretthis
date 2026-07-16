# The `n` presentation type is locale-aware; with no locale support it renders
# the C-locale form — `g` for floats, `d` for ints, neither grouped.
print(format(123.456, "n"))
print(format(1234567, "n"))
print(format(1234567.0, "n"))
print(format(0.0001, "n"))
print(format(1e20, "n"))
print(format(0, "n"))
print(f"{3.14159:n}")
print(f"{42:n}")
print(format(float("inf"), "n"))
print(format(float("nan"), "n"))
print(format(2 + 3j, "n"))
