# A second batch of CPython 3.12 error wording: unpacking, dict(), indexing,
# reversed, str.join/split.

def show(fn):
    try:
        fn()
    except Exception as e:
        print(type(e).__name__, "::", e)


# Sequence unpacking is a ValueError, with direction-specific wording.
def unpack_too_many():
    a, b = [1, 2, 3]


def unpack_too_few():
    a, b, c = [1, 2]


def unpack_star_few():
    a, *b, c = [1]


show(unpack_too_many)
show(unpack_too_few)
show(unpack_star_few)
show(lambda: dict([[1, 2, 3]]))
show(lambda: dict([(1,)]))
show(lambda: dict([5]))

# Container-specific index-type wording.
show(lambda: [1, 2, 3][1.5])
show(lambda: (1, 2, 3)[1.5])
show(lambda: "abc"[1.5])
show(lambda: b"abc"[1.5])
show(lambda: bytearray(b"ab")[1.5])
show(lambda: range(5)[1.5])

# reversed needs a reversible sequence, not just any iterable.
show(lambda: reversed(5))
show(lambda: reversed({1, 2, 3}))
print(list(reversed([1, 2, 3])), list(reversed("ab")))

# dict.update arity, set.pop empty, str.join/split.
show(lambda: {}.update(1, 2, 3))
show(lambda: set().pop())
show(lambda: "-".join(5))
show(lambda: "a b".split(1))
