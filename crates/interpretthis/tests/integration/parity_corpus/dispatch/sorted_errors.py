# sorted() rejects extra positional args and propagates a comparison TypeError.
# Regression: sorted checked only for zero args (so `sorted(xs, keyfn)` dropped
# the second arg), and the sort comparator swallowed the element TypeError
# (`sorted([1,"a",2])` returned a silently-wrong order).
print(sorted([3, 1, 2]))
print(sorted([3, 1, 2], reverse=True))
print(sorted(["b", "a", "c"], key=str.upper))

for label, thunk in [
    ("extra-positional", lambda: sorted([3, 1, 2], len)),
    ("uncomparable", lambda: sorted([1, "a", 2])),
    ("none-and-int", lambda: sorted([1, None])),
]:
    try:
        thunk()
        print(label, "NO ERROR")
    except TypeError:
        print(label, "TypeError")

# list.sort has the same comparator, so it must raise too.
xs = [1, "a"]
try:
    xs.sort()
    print("list.sort NO ERROR")
except TypeError:
    print("list.sort TypeError")
