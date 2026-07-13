# `len(...)` dispatch over every builtin sized type. Pins
# types::dispatch_len -> per-type len_slot routing for str / bytes / list
# / tuple / set / dict / range. CPython str length is codepoint count,
# not byte count — a multibyte string asserts that pathway.
print(len(""))
print(len("abc"))
print(len("héllo"))              # 5 codepoints; UTF-8 would be 6 bytes
print(len(b""))
print(len(b"abc"))                # 3 bytes
print(len([]))
print(len([1, 2, 3, 4, 5]))
print(len(()))
print(len((1, 2, 3)))
print(len({1, 2, 3}))
print(len({}))
print(len({"a": 1, "b": 2}))
print(len(range(0)))
print(len(range(10)))
print(len(range(0, 10, 2)))      # 5
print(len(range(10, 0, -1)))     # 10
print(len(range(10, 0)))         # 0 (empty)
try:
    len(42)
except TypeError:
    print("TypeError")
try:
    len(None)
except TypeError:
    print("TypeError")
