# Pins: ExceptionGroup.subgroup / split.
eg = ExceptionGroup("g", [ValueError("a"), TypeError("b")])
sub = eg.subgroup(ValueError)
print(type(sub).__name__)
print(len(sub.exceptions))
print(type(sub.exceptions[0]).__name__)
m, r = eg.split(ValueError)
print(type(m).__name__, type(r).__name__)
print(len(m.exceptions), len(r.exceptions))
