# Pins: `in` / `not in` work across list, dict (keys), and string.
a = 2 in [1, 2, 3]
b = 5 not in [1, 2, 3]
c = "x" in {"x": 1, "y": 2}
d = "lo" in "hello"
print(f"{a},{b},{c},{d}")
