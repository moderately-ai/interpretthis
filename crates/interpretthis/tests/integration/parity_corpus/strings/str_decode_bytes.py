# str(bytes, encoding[, errors]) decodes a bytes-like object.
print(str(b"hello", "utf-8"))
print(str(b"hello", encoding="utf-8"))
print(str(bytearray(b"world"), "ascii"))
print(str(b"caf\xc3\xa9", "utf-8"))
print(str(b"\xff\xfeh\x00i\x00", "utf-16"))
try:
    str("already text", "utf-8")
except TypeError as e:
    print(type(e).__name__)
# Without an encoding it is the ordinary conversion.
print(str(42), str(), str([1, 2]), repr(str(3.5)))
