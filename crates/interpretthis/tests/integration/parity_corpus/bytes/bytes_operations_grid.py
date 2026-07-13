# Pins: bytes indexing, decode, concat, repeat, split, len, slice.
b = b"hello"
print(b[0])
print(b.decode("utf-8"))
print(b + b" world")
print(b * 2)
print(b"prefix-data".split(b"-"))
print(len(b))
print(b"abc"[1:])
