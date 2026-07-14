# An integer-expecting builtin must reject a float, not silently truncate it.
# Regression: value_to_i64 accepted a float and truncated toward zero, so
# `range(2.9)` became `range(2)`, `chr(65.0)` became 'A', etc. CPython raises
# TypeError for all of these. (int(float) still truncates — that is int()'s own
# path, unaffected.)
for label, thunk in [
    ("range", lambda: range(2.9)),
    ("chr", lambda: chr(65.0)),
    ("insert", lambda: [1, 2].insert(1.0, 9)),
    ("ljust", lambda: "x".ljust(5.0)),
    ("enumerate-start", lambda: list(enumerate([1], start=1.5))),
]:
    try:
        thunk()
        print(label, "NO ERROR")
    except TypeError:
        print(label, "TypeError")

# Integer args still work; int() still truncates a float.
print(list(range(3)))
print(chr(65))
print(int(2.9))
print("x".ljust(3))
