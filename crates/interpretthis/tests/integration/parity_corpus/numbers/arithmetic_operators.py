# Pins: integer +, -, *, // (floor), % (mod), ** (pow) on small ints.
# True division (/) intentionally elided since its repr would drift across
# the version pin; integer division and modulo carry the spec point.
a = 10 + 3    # 13
b = 10 - 3    # 7
c = 10 * 3    # 30
e = 10 // 3   # 3
f = 10 % 3    # 1
g = 2 ** 10   # 1024
print(f"{a},{b},{c},{e},{f},{g}")
