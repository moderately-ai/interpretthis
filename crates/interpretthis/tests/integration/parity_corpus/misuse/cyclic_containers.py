# Self-referential containers: a container can hold itself (reference
# semantics). CPython handles each operation distinctly — repr shows the
# ellipsis form, len/is are shallow, deepcopy preserves the cycle, and json /
# distinct-cyclic equality raise.

# repr: the container currently being formatted renders as [...] / {...}.
a = [1, 2]
a.append(a)
print(a)
print(str(a), repr(a))
print(len(a), a[2] is a, len(a[2]))

# self-equality short-circuits via identity (no deep recursion).
print(a == a)

d = {}
d["x"] = 1
d["self"] = d
print(d)
print(d["self"] is d)

# a cycle one level down.
n = [1, [2, 3]]
n[1].append(n[1])
print(n)

# mutation through the alias is visible (it is the same object).
b = a
b.append(99)
print(a[3], a is b)

# deepcopy preserves the cycle (independent copy, self-ref points to the copy).
import copy

lc = copy.deepcopy(a)
print(lc[2] is lc, lc is a, len(lc))
dc = copy.deepcopy(d)
print(dc["self"] is dc, dc is d)

# json.dumps raises on a circular reference.
import json

try:
    json.dumps(a)
except ValueError as e:
    print("json:", e)

# Comparing two DISTINCT cyclic containers raises RecursionError (CPython).
c1 = []
c1.append(c1)
c2 = []
c2.append(c2)
try:
    print(c1 == c2)
except RecursionError:
    print("RecursionError")
