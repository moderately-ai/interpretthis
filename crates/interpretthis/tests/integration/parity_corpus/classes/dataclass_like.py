# Pin: an empty `class P: pass` instance still supports ad-hoc attribute
# assignment via `__dict__`, modelling the dataclass-shaped usage of bare
# attribute writes after construction.
# Expected stdout: `1`.
class P:
    pass


p = P()
p.x = 1
print(p.x)
