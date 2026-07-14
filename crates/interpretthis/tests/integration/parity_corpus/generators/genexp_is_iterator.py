# A generator expression is a one-shot lazy iterator: next() advances it, a
# following list()/sum() consumes only the remainder, and re-iterating yields
# nothing. Regression: genexps were materialised as lists, so next() failed.
g = (x * 2 for x in range(4))
print(next(g))
print(next(g))
print(list(g))
print(list(g))                       # exhausted -> empty

print(sum(x for x in range(5)))
print(max(x * x for x in range(1, 4)))
print(sorted((x for x in [3, 1, 2])))
print("-".join(str(x) for x in range(3)))
print(any(x > 3 for x in [1, 2, 3, 4]))
print(list(enumerate(c for c in "ab")))
print(dict((k, v) for k, v in [("a", 1), ("b", 2)]))

g2 = (i for i in range(3))
total = 0
for v in g2:
    total += v
print(total)
print(list(g2))                      # already consumed by the for loop

print(type(x for x in range(1)).__name__)
