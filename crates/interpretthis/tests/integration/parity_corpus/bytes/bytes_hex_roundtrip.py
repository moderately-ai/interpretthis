# Pins: bytes.fromhex (classmethod) parses hex strings, allows
# whitespace between pairs; bytes.hex() emits lowercase hex.
print(bytes.fromhex("ff aa 1b"))
print(b"\xff\xaa".hex())
