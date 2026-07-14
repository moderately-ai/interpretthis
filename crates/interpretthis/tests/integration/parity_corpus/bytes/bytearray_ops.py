b = bytearray(b"abc")
print(b, type(b).__name__)
b[0] = 88
print(b)
b.append(100)
print(b)
b.extend(b"xy")
print(b)
print(b[0], b[1:3])
b[1:3] = b"ZZZZ"
print(b)
print(bytearray(5))
print(bytearray([65, 66, 67]))
ba = bytearray(b"hello")
print(ba.upper(), ba.replace(b"l", b"L"))
ba[0] = ord("H")
print(ba.decode("utf-8"))
del ba[0]
print(ba)
print(len(bytearray(b"abcd")))
mutable = bytearray(b"test")
mutable += b"!!"
print(mutable)
