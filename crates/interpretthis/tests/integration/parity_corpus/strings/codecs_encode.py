print("héllo".encode("utf-8"), "héllo".encode("latin-1"))
print("hello".encode("ascii"), b"hello".decode("ascii"))
print("café".encode("utf-8").decode("utf-8"))
print("abc".encode("utf-16"), "abc".encode("utf-16").decode("utf-16"))
print("é".encode("utf-8"), "\U0001F600".encode("utf-8"))
print(b"\xc3\xa9".decode("utf-8"), b"\xff\xfe".decode("utf-8", "replace"))
print("test".encode("utf-8", "strict"))
print("a\nb".encode("utf-8"), b"a\tb".decode("utf-8"))
print(len("héllo"), len("héllo".encode("utf-8")))
print("naïve café".encode("utf-8").hex())
print(bytes.fromhex("48656c6c6f").decode("ascii"))
print("Ω".encode("utf-8"), ord("Ω"), chr(937))
print("résumé".upper(), "RÉSUMÉ".lower())
try:
    "café".encode("ascii")
except UnicodeEncodeError:
    print("encode error")
try:
    b"\xff".decode("ascii")
except UnicodeDecodeError:
    print("decode error")
# errors= handler: replace/ignore, positional and keyword forms
print(b"a\xffb\xfec".decode("utf-8", "ignore"), b"\xff".decode("utf-8", errors="replace"))
print(b"abc\xff".decode("ascii", "ignore"), b"abc\xff".decode("ascii", "replace"))
print(b"\xe2\x9c".decode("utf-8", "replace"), b"\xe2\x9c".decode("utf-8", "ignore"))
