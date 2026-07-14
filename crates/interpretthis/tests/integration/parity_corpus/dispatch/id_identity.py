# id() reflects object identity: id(a) == id(b) iff a is b. Regression: id()
# returned 0 for everything, so every pair compared equal. (The raw id values are
# addresses and differ from CPython, so only the relations are pinned.)
a = [1, 2]
b = a
c = [1, 2]
print(id(a) == id(a))
print(id(a) == id(b))     # alias
print(id(a) == id(c))     # distinct objects with equal contents

# Identity-based dedup, the common use.
xs = [a, b, c]
print(len({id(x) for x in xs}))   # a and b share an id; c is distinct -> 2


class C:
    pass


p, q = C(), C()
print(id(p) == id(p))
print(id(p) == id(q))

# Consistency with `is`.
print((a is b) == (id(a) == id(b)))
print((a is c) == (id(a) == id(c)))
