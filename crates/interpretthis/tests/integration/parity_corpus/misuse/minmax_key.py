words = ["bb", "a", "ccc"]
print(min(words, key=len))
print(max(words, key=len))
print(min(1, 2, 3))
print(max("a", "b", "c"))
print(sorted(words, key=len, reverse=True))
print(min([(1,"z"),(1,"a")]))
