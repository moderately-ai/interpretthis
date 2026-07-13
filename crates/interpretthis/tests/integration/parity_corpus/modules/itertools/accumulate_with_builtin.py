# Pins: itertools.accumulate accepts a builtin (max) as the binary fn.
# Builtin names resolve to the `__builtin__<n>` sentinel which
# call_callable doesn't recognize today.
import itertools
print(list(itertools.accumulate([1, 3, 2, 5, 4], max)))
