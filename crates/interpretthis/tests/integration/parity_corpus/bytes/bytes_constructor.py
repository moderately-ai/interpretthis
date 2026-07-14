# bytes() rejects out-of-range values and a negative count. Regression:
# bytes([300]) masked with `& 0xFF` (silently -> b','), and bytes(-5) used
# `.max(0)` (silently -> b'').
print(bytes(3))
print(list(bytes([65, 66, 67])))
print(list(bytes((1, 2, 3))))
print(list(bytes(range(4))))      # any iterable of ints now works
print(bytes("hi", "utf-8"))

for label, thunk in [
    ("neg-count", lambda: bytes(-5)),
    ("over-255", lambda: bytes([300])),
    ("under-0", lambda: bytes([-1])),
    ("non-int-item", lambda: bytes([1, "x"])),
    ("float-arg", lambda: bytes(3.5)),
]:
    try:
        thunk()
        print(label, "NO ERROR")
    except ValueError:
        print(label, "ValueError")
    except TypeError:
        print(label, "TypeError")
