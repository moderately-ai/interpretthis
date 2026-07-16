import heapq

# nlargest / nsmallest break key ties by first-seen order in BOTH directions
# (a sort-then-reverse would invert nlargest's ties).
words = ["bb", "aa", "cc", "d", "eee", "ff"]
print(heapq.nlargest(3, words, key=len))
print(heapq.nsmallest(3, words, key=len))
print(heapq.nlargest(2, ["apple", "banana", "cherry"], key=len))
print(heapq.nsmallest(2, ["apple", "banana", "cherry", "kiwi", "pear"], key=len))

nums = [(1, "a"), (2, "b"), (1, "c"), (2, "d"), (1, "e")]
print(heapq.nlargest(3, nums, key=lambda t: t[0]))
print(heapq.nsmallest(3, nums, key=lambda t: t[0]))

# Ties on the values themselves (no key).
print(heapq.nlargest(4, [5, 3, 5, 1, 5, 2]))
print(heapq.nsmallest(4, [5, 3, 5, 1, 5, 2]))
