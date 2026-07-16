import struct

# iter_unpack yields one tuple per fixed-size record in the buffer.
print(list(struct.iter_unpack(">h", b"\x00\x01\x00\x02")))
print(list(struct.iter_unpack("<i", struct.pack("<i", 1) + struct.pack("<i", 2))))
print(list(struct.iter_unpack("2b", b"\x01\x02\x03\x04")))
print(list(struct.iter_unpack(">hh", b"\x00\x01\x00\x02\x00\x03\x00\x04")))
print(len(list(struct.iter_unpack("B", b"abcde"))))

# A buffer that is not a whole multiple of the record size raises struct.error.
try:
    list(struct.iter_unpack(">h", b"\x00"))
except struct.error as e:
    print("error:", e)

# The result is a one-shot iterator.
it = struct.iter_unpack(">h", b"\x00\x01\x00\x02")
print(next(it))
print(next(it))
