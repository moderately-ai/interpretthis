# itertools.permutations validates r: a negative r raises ValueError (not
# "treat as len"), a non-integer raises TypeError. Regression: a negative r fell
# through usize::try_from(...).unwrap_or(len) and returned the full-length runs.
import itertools

print(list(itertools.permutations([1, 2, 3], 2)))
print(len(list(itertools.permutations([1, 2, 3]))))     # r defaults to len -> 6
print(list(itertools.permutations([1, 2, 3], 0)))       # [()]
print(list(itertools.permutations([1, 2, 3], None)))    # None -> len

try:
    list(itertools.permutations([1, 2, 3], -1))
except ValueError:
    print("perm ValueError")
try:
    list(itertools.permutations([1, 2, 3], 1.5))
except TypeError:
    print("perm TypeError")

# Sibling: combinations already validates a negative r.
try:
    list(itertools.combinations([1, 2, 3], -1))
except ValueError:
    print("comb ValueError")
