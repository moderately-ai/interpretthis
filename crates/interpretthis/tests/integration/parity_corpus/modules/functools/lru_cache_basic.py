# Pins: functools.lru_cache / cache memoize pure functions.
from functools import lru_cache, cache

calls = [0]

@lru_cache(maxsize=32)
def twice(x):
    calls[0] = calls[0] + 1
    return x * 2

print(twice(3))
print(twice(3))
print(calls[0])

@lru_cache
def add1(x):
    return x + 1

print(add1(10), add1(10))

@cache
def thrice(x):
    return x * 3

print(thrice(4), thrice(4))
