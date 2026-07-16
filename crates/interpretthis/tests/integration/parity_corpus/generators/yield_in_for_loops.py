# A `for ... yield` inside `try`/`if`/`with` must suspend at the yield, not
# eagerly drain the loop. The divergence only shows under partial consumption:
# with full `list()` consumption the loop finishes either way, but `next()` a
# few times then `close()` reveals whether `finally`/`__exit__`/trailing
# statements ran at the right time.


# try/finally: cleanup must run on close(), not before the first yield.
def tf():
    try:
        for i in range(5):
            yield i
    finally:
        print("tf finally")


g = tf()
print("a", next(g))
print("b", next(g))
print("close")
g.close()
print("after")
print("---")


# for inside if: the post-if statement must not run until the loop is exhausted.
def fi():
    if True:
        for i in range(5):
            yield i
    print("fi loop-done")


g = fi()
print(next(g), next(g))
g.close()
print("===")


# for inside with: __exit__ runs at real block exit, not on each yield.
class CM:
    def __enter__(self):
        print("enter")
        return self

    def __exit__(self, *a):
        print("exit")
        return False


def fw():
    with CM():
        for i in range(5):
            yield i


g = fw()
print("x", next(g))
print("y", next(g))
g.close()
print("---")


# A top-level `for ... yield` must not swallow the statements that follow it.
def trailing():
    for i in range(2):
        yield i
    yield 99
    print("trailing body-end")


print(list(trailing()))


# for with a nested if, then a trailing yield after the loop.
def nested_if_trailing():
    for i in range(4):
        if i % 2 == 0:
            yield i
    yield "end"


print(list(nested_if_trailing()))


# try/except/else/finally around a for, fully consumed.
def teef():
    try:
        for i in range(3):
            yield i
        print("body-done")
    except ValueError:
        print("handler")
    finally:
        print("finally")


print(list(teef()))
