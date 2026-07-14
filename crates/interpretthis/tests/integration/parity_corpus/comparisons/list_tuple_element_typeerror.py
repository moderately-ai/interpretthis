# Ordering lists/tuples propagates a TypeError from the first differing pair of
# uncomparable elements. Regression: lex_lt swallowed the element TypeError via
# `dispatch_lt(x, y).unwrap_or(false)`, so `[1, 2] < [1, "a"]` returned False.
print([1, 2] < [1, 3])          # True (comparable)
print([1, 2, 3] < [1, 2])       # False (prefix)
print((1, 2) < (1, 2, 0))       # True

try:
    [1, 2] < [1, "a"]           # 2 < "a" -> TypeError
except TypeError:
    print("list TypeError")
try:
    (1, "a") < (1, 2)           # "a" < 2 -> TypeError
except TypeError:
    print("tuple TypeError")

# The uncomparable element only matters at the first differing position: a
# leading difference decides before the bad pair is reached.
print([1, 2] < [2, "a"])        # True: 1 < 2 decides, "a" never compared
print(["a", 1] < ["b", {}])     # True: "a" < "b" decides, dict at pos 2 never compared
