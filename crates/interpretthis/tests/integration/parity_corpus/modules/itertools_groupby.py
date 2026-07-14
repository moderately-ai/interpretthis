import itertools
print([(k, list(g)) for k, g in itertools.groupby("aaabbbccd")])
print([(k, list(g)) for k, g in itertools.groupby([1, 1, 2, 3, 3, 3])])
data = [("a", 1), ("a", 2), ("b", 3)]
print([(k, list(g)) for k, g in itertools.groupby(data, key=lambda p: p[0])])
# Only consecutive runs group.
print([(k, len(list(g))) for k, g in itertools.groupby([1, 2, 1])])
print([(k, list(g)) for k, g in itertools.groupby("")])
