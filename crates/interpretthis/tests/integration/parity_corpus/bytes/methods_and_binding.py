# Newly added methods.
print(b"ABC".isascii(), b"caf\xc3\xa9".isascii())
print(b"a\tb\tc".expandtabs(4))
print(b"a-b-c".rsplit(b"-", 1), b"a b c".rsplit())
print(bytearray(b"ABC").isascii(), bytearray(b"a\tb").expandtabs(2))
print(bytearray(b"x-y-z").rsplit(b"-", 1))

# Bytes methods bind as first-class attributes and hasattr sees them.
f = b"hello".upper
print(f())
g = b"123".isdigit
print(g())
print(hasattr(b"x", "isascii"), hasattr(b"x", "expandtabs"), hasattr(b"x", "rsplit"))
print(hasattr(b"x", "upper"), hasattr(b"x", "nope"))
print(list(map(bytes.upper, [b"a", b"b"])))

# bytearray bound methods + hasattr, and correct return types.
ba = bytearray(b"hello world")
print(ba.title())
print(hasattr(ba, "partition"), hasattr(ba, "center"), hasattr(ba, "zfill"))
print(bytearray(b"a-b-c").partition(b"-"))
print(bytearray(b"a-b-c").rpartition(b"-"))
print(bytearray(b"x\ny").splitlines())
print(bytearray(b"a-b").split(b"-"))
print(bytearray(b"hi").center(6, b"*"), bytearray(b"42").zfill(5))

# Absent methods still raise AttributeError.
try:
    b"x".casefold()
except AttributeError:
    print("no casefold")
try:
    b"x".append(1)
except AttributeError:
    print("bytes has no append")
