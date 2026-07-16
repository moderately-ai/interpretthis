# The `_`/`,` grouping separator groups binary/octal/hex digits every 4 and
# decimal/float digits every 3, and `0`-fill re-groups across the zero padding
# (groups are atomic, so the field can overshoot the requested width).
cases = [
    (255, "_b"), (255, "_o"), (255, "_x"), (255, "_X"), (255, "_d"),
    (255, "08_b"), (255, "08_x"), (1048575, "_x"), (1234567, "_d"),
    (5, "08_b"), (5, "08_x"), (5, "09_d"), (1234, "010,d"), (255, "#010_x"),
    (5, "010_b"), (5, "011_b"), (5, "012_b"), (5, "09_b"), (5, "07_d"),
    (-5, "08_b"), (-1234, "010,d"), (5, "+08_b"), (255, "#08_x"), (255, "+#012_x"),
    (5, "*=8_b"), (5, "*>8_b"), (5, "8_b"),
    (3.14159, "015,.2f"), (1234.5, "012,.1f"), (1234567.89, ",.2f"),
    (255, "#_x"), (511, "#_o"), (255, "#_b"),
]
for value, spec in cases:
    print(repr(spec), format(value, spec))
