data = [("b", 2), ("a", 1), ("b", 1), ("a", 2)]
print(sorted(data))
print(sorted(data, key=lambda x: x[1]))
print(sorted(data, key=lambda x: (x[0], -x[1])))
print(sorted("hello world"))
print(sorted([3, 1, 2], key=lambda x: -x))
words = ["banana", "pie", "Washington", "book"]
print(sorted(words, key=len))
print(sorted(words, key=str.lower))
nums = [5, 2, 8, 1, 9]
nums.sort(reverse=True)
print(nums)
print(sorted([1.5, 1, 2, 1.5]))
print(max(["apple", "banana"], key=len))
print(min([(1, "z"), (1, "a")]))
import functools
print(sorted([3,1,2], key=functools.cmp_to_key(lambda a,b: b-a)))
