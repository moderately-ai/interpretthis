def f():
    return 1
f.calls = 0
f.calls += 1
f.calls += 1
print(f.calls)
f.label = "hello"
print(f.label)
print(hasattr(f, "calls"), hasattr(f, "nonexistent"))
def counter():
    counter.count += 1
    return counter.count
counter.count = 0
print(counter(), counter(), counter())
print(f.__name__, f.calls)
g = f
g.calls = 100
print(f.calls)
def tagged():
    pass
tagged.tags = []
tagged.tags.append("a")
tagged.tags.append("b")
print(tagged.tags)
