data = [("b", 2), ("a", 1), ("b", 1), ("a", 2)]
print(sorted(data))
print(sorted(data, key=lambda x: x[0]))
print(sorted(data, key=lambda x: x[1]))
print(sorted([3, 1, 2, 1, 3], reverse=True))
words = ["banana", "apple", "cherry", "date"]
print(sorted(words, key=len))
print(sorted(words, key=lambda w: (len(w), w)))
nums = [-5, 3, -1, 4, -2]
print(sorted(nums, key=abs))
lst = [5, 2, 8, 1, 9]
lst.sort()
print(lst)
lst.sort(reverse=True)
print(lst)
print(sorted("hello"))
print(sorted([{"age": 30}, {"age": 20}], key=lambda p: p["age"]))
mixed = [(1, "z"), (1, "a"), (2, "m")]
print(sorted(mixed))
