# Pin: `set(<genexp>)` deduplicates the generator's yielded values; printing it
# sorted keeps the test independent of set iteration order.
# Expected stdout: `[1, 2, 3]`.
print(sorted(set(x for x in [1, 1, 2, 2, 3])))
