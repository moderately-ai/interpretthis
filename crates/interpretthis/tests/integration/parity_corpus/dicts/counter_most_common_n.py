# Counter.most_common(n) clamps a negative n to the empty list. Regression: a
# negative n fell through usize::try_from(...).unwrap_or(len) and returned every
# entry.
from collections import Counter

c = Counter("aaabbc")
print(c.most_common(-1))     # [] not everything
print(c.most_common(-5))     # []
print(c.most_common(0))      # []
print(c.most_common(2))
print(c.most_common(100))    # clamps to all
print(c.most_common())       # all
