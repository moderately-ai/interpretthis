# CPython's sequence-specific concatenation messages and int() repr quoting.
def show(f):
    try:
        f()
    except (TypeError, ValueError) as e:
        print(f"{type(e).__name__}: {e}")

show(lambda: "abc" + 5)
show(lambda: [1, 2] + 5)
show(lambda: (1,) + "x")
show(lambda: b"a" + 5)
show(lambda: bytearray(b"a") + 5)
show(lambda: 5 + "a")
show(lambda: int("not a number"))
show(lambda: int("  bad  "))
show(lambda: int("12x", 16))
show(lambda: int("hello's"))
