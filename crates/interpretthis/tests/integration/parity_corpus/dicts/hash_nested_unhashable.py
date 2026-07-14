# hash() of a tuple containing an unhashable element raises TypeError. A tuple
# reports itself hashable, but hashing recurses into its elements. Regression:
# the fallback hash slot returned 0 on an unhashable element, so `hash((1, [2]))`
# was 0 and every such tuple collided into one bucket.
try:
    hash((1, [2]))
    print("NO ERROR")
except TypeError:
    print("TypeError")

try:
    {(1, [2]): "v"}
    print("dict-key NO ERROR")
except TypeError:
    print("dict-key TypeError")

# Hashable tuples still hash, and equal tuples hash equal.
print(hash((1, 2)) == hash((1, 2)))
print(hash(()) == hash(()))
print(hash((1, (2, 3))) == hash((1, (2, 3))))
