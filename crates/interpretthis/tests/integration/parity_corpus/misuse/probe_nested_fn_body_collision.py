def make1():
    def helper():
        return "one"
    return helper
def make2():
    def helper():
        return "two"
    return helper
h1 = make1()
h2 = make2()
print(h1(), h2())
print(h2(), h1())
fns = []
def factory(tag):
    def worker():
        return tag
    return worker
for t in ["a", "b", "c"]:
    fns.append(factory(t))
print([f() for f in fns])
def recursive_maker(depth):
    def step():
        if depth == 0:
            return "base"
        return recursive_maker(depth - 1)()
    return step
print(recursive_maker(3)())
