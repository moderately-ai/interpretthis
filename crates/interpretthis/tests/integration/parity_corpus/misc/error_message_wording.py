# CPython 3.12 wording/type for a batch of builtin errors.

def show(fn):
    try:
        fn()
    except Exception as e:
        print(type(e).__name__, "::", e)


# min/max argument errors (note the ", got 0" and "iterable argument is empty").
show(lambda: max())
show(lambda: min())
show(lambda: min([]))
show(lambda: max([]))

# A non-integer where an integer index/count/bound is required.
show(lambda: range("a"))
show(lambda: range(1.5))
show(lambda: "abc"[1.0:])
show(lambda: chr("x"))

# str.format replacement-index overflow is IndexError, not ValueError.
show(lambda: "{}".format())
show(lambda: "{0} {1}".format(1))
show(lambda: "{2}".format("a", "b"))
