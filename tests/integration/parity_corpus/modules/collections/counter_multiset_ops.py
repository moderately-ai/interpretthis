# Counter & and | — multiset intersection / union.
#
# Pins CPython semantics: | combines two Counters per-key by taking the
# MAX of the two counts; & takes the MIN. Both filter out non-positive
# results (CPython's `_keep_positive`); a key present in only one
# Counter survives | (max with absent=0) but not & (min with absent=0).
from collections import Counter

a = Counter(apple=3, banana=1, cherry=2)
b = Counter(apple=1, banana=2, date=5)

print(a | b)  # union: max(3,1)=3, max(1,2)=2, cherry=2 (only a), date=5 (only b)
print(a & b)  # intersection: min(3,1)=1, min(1,2)=1, no cherry, no date

# Empty Counter cases.
print(Counter() | a)
print(Counter() & a)

# Self-arithmetic.
print(a | a)
print(a & a)
