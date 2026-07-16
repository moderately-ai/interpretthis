# iter(bytearray) shares the buffer (CPython's bytearray_iterator), yielding
# each byte as an int and reflecting mutations before the cursor.
ba = bytearray(b"abc")
it = iter(ba)
print(next(it))
ba.append(100)
print(list(it))

print(type(iter(bytearray(b"x"))).__name__)
print(list(iter(bytearray(b"hello"))))
print(list(iter(bytearray())))
print([b for b in iter(bytearray(b"\x01\x02\x03"))])
print(sum(iter(bytearray(b"\x01\x02\x03"))))
