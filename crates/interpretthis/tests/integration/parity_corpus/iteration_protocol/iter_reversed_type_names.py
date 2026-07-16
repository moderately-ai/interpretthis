# iter(x) / reversed(x) report CPython's per-source-type iterator names.
def tn(x):
    return type(x).__name__


print(tn(iter([1])))
print(tn(iter((1,))))
print(tn(iter("abc")))
print(tn(iter("héllo")))
print(tn(iter({1})))
print(tn(iter(frozenset({1}))))
print(tn(iter({1: 2})))
print(tn(iter(range(3))))
print(tn(iter(b"x")))
print(tn(iter(bytearray(b"x"))))
print(tn(iter({1: 2}.keys())))
print(tn(iter({1: 2}.values())))
print(tn(iter({1: 2}.items())))

print(tn(reversed([1])))
print(tn(reversed((1,))))
print(tn(reversed("abc")))
print(tn(reversed(range(3))))
print(tn(reversed(bytearray(b"x"))))
print(tn(reversed(b"x")))
print(tn(reversed({1: 2})))

# Iteration itself still works over each.
print(list(iter((1, 2, 3))))
print(list(iter("ab")))
print(sorted(iter({3, 1, 2})))
print(list(iter(range(3))))
print(list(reversed([1, 2, 3])))
print(list(reversed("abc")))
print(list(iter({1: "a", 2: "b"})))
print(list(iter({1: "a", 2: "b"}.values())))
print(list(iter({1: "a"}.items())))
