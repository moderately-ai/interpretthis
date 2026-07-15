# sorted/min/max with key, reverse; stability; mixed comparisons.
words = ["banana", "Apple", "cherry", "date"]
print(sorted(words), sorted(words, key=str.lower))
print(sorted(words, key=len), sorted(words, key=len, reverse=True))
print(sorted([3, 1, 2], reverse=True), max("hello"), min([5, 3, 8], key=lambda x: -x))
nums = [(1, "b"), (2, "a"), (1, "a"), (2, "b")]
print(sorted(nums), sorted(nums, key=lambda p: p[1]))
print(max(nums, key=lambda p: p[0]), min(nums, key=lambda p: (p[0], p[1])))
# sort in place, stability
data = [("a", 3), ("b", 1), ("c", 3), ("d", 1)]
data.sort(key=lambda x: x[1])
print(data)
print(sorted([1.5, 1, 2, True, 0, False]))
print(sorted(["10", "9", "100", "2"]), sorted(["10", "9", "100", "2"], key=int))
# functools.cmp_to_key
from functools import cmp_to_key
print(sorted([3, 1, 2], key=cmp_to_key(lambda a, b: b - a)))
