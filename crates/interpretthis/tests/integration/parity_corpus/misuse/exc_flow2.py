def gen():
    try:
        yield 1
        yield 2
    finally:
        print("gen cleanup")
g = gen()
print(next(g))
g.close()
def loop_finally():
    for i in range(5):
        try:
            if i == 2:
                break
            print(i)
        finally:
            print(f"f{i}")
loop_finally()
def nested_except():
    try:
        try:
            raise ValueError("inner")
        except ValueError:
            raise TypeError("outer")
    except TypeError as e:
        return str(e)
print(nested_except())
try:
    raise ValueError("a")
except (ValueError, TypeError) as e:
    print("caught", type(e).__name__)
def cont_finally():
    r = []
    for i in range(4):
        try:
            if i % 2 == 0:
                continue
            r.append(i)
        finally:
            r.append(f"f{i}")
    return r
print(cont_finally())
