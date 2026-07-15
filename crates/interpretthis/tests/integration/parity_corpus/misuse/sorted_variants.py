print(sorted([3, 1, 2]))
print(sorted([3, 1, 2], reverse=True))
print(sorted(["banana", "apple"], key=len))
print(sorted([(1, "b"), (1, "a")], key=lambda x: x[1]))
data = [{"name": "Bob", "age": 30}, {"name": "Alice", "age": 25}]
print([d["name"] for d in sorted(data, key=lambda d: d["age"])])
from functools import cmp_to_key
print(sorted([3, 1, 2], key=cmp_to_key(lambda a, b: a - b)))
print(sorted("dcba"))
print(sorted({3, 1, 2}))
print(sorted({"c": 1, "a": 2, "b": 3}))
print(sorted([-3, 1, -2], key=abs))
words = "the quick brown fox".split()
print(sorted(words, key=lambda w: (len(w), w)))
print(sorted(range(5), reverse=True))
print(sorted([1.5, 1.1, 1.9]))
lst = [5, 2, 8, 1]
print(sorted(lst))
print(lst)
