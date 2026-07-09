# Comparison chains: a < b < c is `(a < b) AND (b < c)` per PEP 308, with
# `b` evaluated ONCE. The chain shortcuts on the first False — pins both
# the AND semantics and the dispatch through types::dispatch_lt.
print(1 < 2 < 3)
print(1 < 2 < 2)        # False on second comparison
print(3 < 2 < 1)        # False on first comparison (shortcircuit)
print(1 <= 1 <= 1)
print(5 > 4 > 3 > 2 > 1)
