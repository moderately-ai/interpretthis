# int.to_bytes / int.from_bytes — byte encoding of integers, including the
# signed two's-complement form, both byte orders, arbitrary-precision values,
# and the OverflowError edges. Regression: neither method existed.
print(int.from_bytes(b"\x01\x00", "big"))
print(int.from_bytes(b"\x01\x00", "little"))
print(int.from_bytes(b"\xff\xff", "big", signed=True))
print(int.from_bytes([255, 0], "big"))
print(int.from_bytes(b"", "big"))

print((256).to_bytes(2, "big"))
print((256).to_bytes(2, "little"))
print((255).to_bytes(1))                       # byteorder defaults to "big"
print((-1).to_bytes(1, "big", signed=True))
print((-129).to_bytes(2, "big", signed=True))

# Round-trips past i64.
big = 2**70 + 12345
encoded = big.to_bytes(16, "big")
print(encoded)
print(int.from_bytes(encoded, "big") == big)
print(int.from_bytes((2**64).to_bytes(9, "little"), "little"))

# Overflow / sign errors.
def expect_overflow(fn):
    try:
        fn()
        print("no-error")
    except OverflowError:
        print("OverflowError")


expect_overflow(lambda: (256).to_bytes(1, "big"))
expect_overflow(lambda: (-1).to_bytes(1, "big"))
expect_overflow(lambda: (128).to_bytes(1, "big", signed=True))
