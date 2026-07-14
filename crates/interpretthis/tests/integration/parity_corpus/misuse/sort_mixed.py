try:
    print(sorted([1, "a", 2]))
except TypeError as e:
    print("sort:", type(e).__name__)
