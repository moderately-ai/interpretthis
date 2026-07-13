# Pins: functools.cmp_to_key for sorted().
from functools import cmp_to_key

def rev(a, b):
    if a < b:
        return 1
    if a > b:
        return -1
    return 0

print(sorted([3, 1, 2], key=cmp_to_key(rev)))
