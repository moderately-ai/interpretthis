# iter() returns a real iterator and next() advances it. Regression: iter(x)
# returned x unchanged, and next() on a non-iterator materialised and returned
# the FIRST element every time (never advancing), and swallowed the
# "not an iterator" error into the default.

it = iter([1, 2, 3])
print(next(it))
print(next(it))
print(next(it))
print(next(it, "end"))            # default on exhaustion

# Partial consumption: the cursor is shared, so the rest iterates from where
# next() left off.
it2 = iter([10, 20, 30, 40])
print(next(it2))
print(list(it2))

# next() on a non-iterator raises TypeError, even with a default.
for label, thunk in [
    ("list", lambda: next([1, 2, 3])),
    ("list-default", lambda: next([1, 2, 3], "x")),
    ("int", lambda: next(42, "x")),
    ("str", lambda: next("abc")),
]:
    try:
        thunk()
        print(label, "NO ERROR")
    except TypeError:
        print(label, "TypeError")

# iter() over other iterables.
print(list(iter((1, 2))))
print(list(iter(range(3))))
print(sorted(iter({3, 1, 2})))
