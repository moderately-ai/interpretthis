import struct
print(struct.pack("<I", 1234), struct.pack(">I", 1234))
print(struct.unpack("<hh", struct.pack("<hh", -1, 256)))
print(struct.pack("=i", 42))
print(struct.calcsize("<10s"), struct.calcsize(">3i"), struct.calcsize("bhiq"))
print(struct.unpack(">d", struct.pack(">d", 3.14159265358979)))
print(struct.pack(">3s", b"hello"))
print(struct.pack(">5s", b"hi"))
print(struct.unpack(">Q", struct.pack(">Q", 18446744073709551615)))
print(struct.unpack(">i", struct.pack(">i", -2147483648)))
print(struct.pack(">bB", -128, 255))
print(struct.pack("2c", b"a", b"b"))
print(struct.pack(">hxh", 1, 2))
print(struct.calcsize(">hxh"))
try:
    struct.pack(">b", 200)
except struct.error as e:
    print("err:", "range" in str(e))
try:
    struct.unpack(">i", b"\x00\x00")
except struct.error as e:
    print("bufferr")
print(struct.pack(">?", True), struct.pack(">?", False))
