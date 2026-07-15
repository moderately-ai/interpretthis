d = {"x": 1, "y": 2}
a, b = d.items()
print(a, b)
k1, k2 = d.keys()
print(k1, k2)
v1, v2 = d.values()
print(v1, v2)
a, b, c = {"p": 1, "q": 2, "r": 3}
print(a, b, c)
from collections import deque
x, y, z = deque([1, 2, 3])
print(x, y, z)
b1, b2, b3 = b"abc"
print(b1, b2, b3)
ba1, ba2 = bytearray(b"hi")
print(ba1, ba2)
first, *rest = d.items()
print(first, rest)
for a, b in d.items():
    print(a, b)
print(dict(d.items()))
print([k for k in d.keys()])
