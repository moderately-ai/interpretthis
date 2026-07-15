# int / bool methods bind as first-class attributes and via the type.
print(hasattr(5, "bit_length"), hasattr(5, "to_bytes"), hasattr(True, "bit_length"))
h = (255).bit_length
print(h(), h())
print((255).to_bytes(2, "big").hex())
print((10).as_integer_ratio(), (10).conjugate())
print(list(map(int.bit_length, [1, 255, 1024])))
b = (12).bit_count
print(b())

# float methods.
print(hasattr(1.5, "is_integer"), hasattr(1.5, "hex"), hasattr(1.5, "as_integer_ratio"))
f = (2.0).is_integer
print(f(), (3.14).is_integer())
print((0.5).hex(), (0.25).as_integer_ratio())
print(list(map(float.is_integer, [1.0, 1.5, 2.0])))

# complex.
print(hasattr(complex(1, 2), "conjugate"))
c = complex(3, 4).conjugate
print(c())
print(complex(1, -1).conjugate())

# range attributes and methods.
r = range(2, 20, 3)
print(r.start, r.stop, r.step)
print(hasattr(r, "count"), hasattr(r, "index"))
print(r.count(5), r.index(5))
cnt = range(10).count
print(cnt(3), range(10).index(7))
print(range(5).start, range(5).stop, range(5).step)
print(range(0, 10, 2).start, range(0, 10, 2).stop, range(0, 10, 2).step)

# Absent attributes still raise.
try:
    (5).nope
except AttributeError:
    print("int no nope")
try:
    range(5).append
except AttributeError:
    print("range no append")
