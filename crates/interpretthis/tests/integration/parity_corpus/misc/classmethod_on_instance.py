# CPython exposes dict.fromkeys / bytes.fromhex / bytes.maketrans (and the
# bytearray variants) on instances too, not only on the type. Regression for
# routing the instance form through the type-form dispatch, and reporting them
# to hasattr/getattr.
print({}.fromkeys([1, 2, 3], 0))
print({"a": 1}.fromkeys(["x", "y"]))
d = {"k": "v"}
print(d.fromkeys([10, 20]))
print(dict.fromkeys([9], 1))
print(b"".fromhex("48656c6c6f"))
print(bytearray().fromhex("4142"))
print(b"abc".maketrans(b"ab", b"xy") == bytes.maketrans(b"ab", b"xy"))
print(hasattr({}, "fromkeys"), hasattr(b"", "fromhex"), hasattr(b"", "maketrans"))
print(hasattr(bytearray(), "fromhex"), hasattr(bytearray(), "maketrans"))
# captured (getattr-value) forms are callable
f = {}.fromkeys
print(f([1, 2]))
g = b"".fromhex
print(g("4344"))
print(callable({}.fromkeys), callable(b"".fromhex))
