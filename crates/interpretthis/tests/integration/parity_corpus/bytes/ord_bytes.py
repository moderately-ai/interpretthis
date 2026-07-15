print(ord(b"A"), ord(b"z"), ord(b"\x00"), ord(b"\xff"))
print(ord(bytearray(b"Q")))
print(ord("A"), ord("€"))
for bad in [b"AB", b"", bytearray(b"xyz")]:
    try:
        ord(bad)
    except TypeError as e:
        print("TypeError", len(bad))
