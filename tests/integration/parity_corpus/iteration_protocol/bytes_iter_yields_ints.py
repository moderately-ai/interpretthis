# Iterating bytes yields the integer byte values. Pins types::bytes_iter.
b = b"abc"
print(list(b))
for byte in b"hi":
    print(byte)
print(sum(b"abc"))               # 97 + 98 + 99 = 294
