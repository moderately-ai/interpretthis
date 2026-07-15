# bytes / bytearray methods, operators, and % formatting.
b = b"Hello, World"
print(b.upper(), b.lower(), b.split(b", "))
print(b.replace(b"o", b"0"), b.find(b"World"), b.count(b"l"))
print(b"abc" + b"def", b"ab" * 3, b"x" in b"xyz")
# bytes membership: bytes-subsequence, and int byte-value.
print(b"xy" in b"xyz", b"xz" in b"xyz", b"" in b"xyz", b"xyz" in b"xyz")
print(120 in b"xyz", 200 in b"xyz", 0 in b"xyz")
print(bytearray(b"cd") in b"abcde", 99 in bytearray(b"abc"))
print(bytes([65, 66, 67]), bytes(range(65, 70)))
print(b"%d apples and %s" % (3, b"pears"))
print(b"%.2f" % 3.14159, b"%x" % 255)
ba = bytearray(b"hello")
ba[0] = 72
ba.append(33)
print(ba, ba.hex(), bytes(ba))
print(b"\x00\x01\x02".hex(":"), bytearray.fromhex("48 65 6c"))
print(list(b"abc"), b"a,b,c".split(b","), b"  strip  ".strip())
