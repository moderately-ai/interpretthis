data = [("b", 2), ("a", 2), ("c", 1), ("a", 1)]
print(sorted(data, key=lambda x: x[1]))            # stable on equal keys
print(sorted(data, key=lambda x: x[0]))
print(sorted([3, 1, 2], reverse=True))
print(sorted("banana"))
print(sorted([1, 2, 3], key=lambda x: -x))
words = ["bb", "a", "ccc", "dd"]
print(sorted(words, key=len))
print(sorted(words, key=lambda w: (len(w), w)))
