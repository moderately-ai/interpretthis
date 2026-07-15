# dict and bytearray are Arc-backed reference types: two separately-built equal
# objects are distinct (`is` False, distinct ids), an alias shares identity, and
# id is stable across mutation (it is the object's address, not its contents).
print({} is {})
print({1: 2} is {1: 2})

d = {"a": 1}
e = d
print(e is d, id(e) == id(d))
before = id(d)
d["b"] = 2
print(id(d) == before, sorted(e.items()))

# Two equal dicts are still distinct objects with distinct ids.
f = {"a": 1, "b": 2}
print(d is f, id(d) == id(f), d == f)

ba = bytearray(b"abc")
print(bytearray(b"abc") is bytearray(b"abc"))
bb = ba
print(bb is ba, id(bb) == id(ba))
ba_before = id(ba)
ba.append(100)
print(id(ba) == ba_before, bb == ba)

bc = bytearray(b"abcd")
print(ba is bc, id(ba) == id(bc), ba == bc)

from array import array

ar = array("i", [1, 2, 3])
print(array("i", [1, 2, 3]) is array("i", [1, 2, 3]))
ad = ar
print(ad is ar, id(ad) == id(ar))
ar_before = id(ar)
ar.append(4)
print(id(ar) == ar_before, ad.tolist())
