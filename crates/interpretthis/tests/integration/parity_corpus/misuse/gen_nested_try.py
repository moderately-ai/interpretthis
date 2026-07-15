def nested():
    try:
        try:
            yield 1
            yield 2
        finally:
            print("inner-fin")
    finally:
        print("outer-fin")
print(list(nested()))
def deep():
    try:
        yield "a"
        try:
            yield "b"
            yield "c"
        except ValueError:
            yield "inner-caught"
        finally:
            print("inner-cleanup")
        yield "d"
    finally:
        print("outer-cleanup")
g = deep()
print(next(g))
print(next(g))
print(g.throw(ValueError))
print(list(g))
