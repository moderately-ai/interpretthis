# Pins: set literal `{1, 2, 3}` has len == 3.
# Set repr is order-dependent so we assert len rather than print(x).
x = {1, 2, 3}
print(len(x))
