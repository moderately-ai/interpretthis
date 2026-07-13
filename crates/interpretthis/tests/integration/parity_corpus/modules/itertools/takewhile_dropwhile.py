# itertools.takewhile + dropwhile — predicate-driven prefix/suffix.
#
# Pins CPython semantics: takewhile yields items while the predicate
# is truthy then stops; dropwhile skips items while truthy then yields
# all remaining items unconditionally (even ones the predicate would
# now reject).
from itertools import takewhile, dropwhile

# takewhile stops at the first falsy verdict and consumes no more.
print(list(takewhile(lambda x: x < 5, [1, 2, 3, 4, 5, 6, 1, 2])))
print(list(takewhile(lambda x: x > 0, [1, 2, 3, 0, 4, 5])))
print(list(takewhile(lambda x: x > 0, [])))
print(list(takewhile(lambda x: x > 100, [1, 2, 3])))

# dropwhile skips until the first falsy verdict, then yields the rest
# UNCONDITIONALLY — the predicate is not re-tested.
print(list(dropwhile(lambda x: x < 5, [1, 4, 6, 4, 1])))
print(list(dropwhile(lambda x: x < 5, [10, 1, 2, 3])))
print(list(dropwhile(lambda x: x < 5, [1, 2, 3])))
