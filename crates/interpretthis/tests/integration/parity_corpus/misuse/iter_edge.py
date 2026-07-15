it = iter([1, 2, 3])
print(next(it), next(it), next(it))
try:
    next(it)
except StopIteration:
    print("stopped")
print(next(iter([]), "default"))
it2 = iter(range(3))
print(list(it2))
print(list(it2))
gen = (x for x in range(3))
print(next(gen))
print(list(gen))
print(sum(iter([1, 2, 3])))
sentinel_calls = []
def counter():
    sentinel_calls.append(1)
    return len(sentinel_calls)
print(list(iter(counter, 3)))
print(list(enumerate(iter(["a", "b"]))))
combined = list(zip(iter([1, 2]), iter([3, 4])))
print(combined)
