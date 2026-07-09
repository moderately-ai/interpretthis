# Iterating a `Counter`'s keys follows first-appearance order in both engines —
# the relevant deterministic-iteration guarantee carried over from `dict`.
import collections
c = collections.Counter('hello')
for k in c:
    print(k)
