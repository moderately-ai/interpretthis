# ASCII-only case, predicate, strip, join and affix methods on bytes.
print(b"Hello".upper(), b"Hello".lower(), b"Hello".swapcase())
print(b"hello world".title(), b"hello".capitalize())
print(b"123".isdigit(), b"abc".isalpha(), b"a1".isalnum(), b"  ".isspace())
print(b"ABC".isupper(), b"abc".islower(), b"Abc".isupper())
print(b"  hi  ".strip(), b"xxhixx".strip(b"x"), b"  hi".lstrip(), b"hi  ".rstrip())
print(b"-".join([b"a", b"b", b"c"]))
print(b"TestCase".removeprefix(b"Test"), b"TestCase".removesuffix(b"Case"))
print(b"a,b,c".split(b","))
