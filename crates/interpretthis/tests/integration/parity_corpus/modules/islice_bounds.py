# itertools.islice validates its bounds: start/stop must be a non-negative int
# or None, step a positive int or None. Regression: bad bounds were silently
# folded to a default via opt_usize.
import itertools

print(list(itertools.islice([1, 2, 3, 4, 5], 1, 4)))     # [2, 3, 4]
print(list(itertools.islice([1, 2, 3, 4, 5], 3)))        # [1, 2, 3]
print(list(itertools.islice([1, 2, 3, 4, 5], 1, None)))  # None stop -> rest
print(list(itertools.islice([1, 2, 3, 4, 5], 0, 5, 2)))  # step 2

for label, fn in [
    ("neg_stop", lambda: itertools.islice([1, 2, 3], -1)),
    ("neg_start", lambda: itertools.islice([1, 2, 3], -1, 2)),
    ("float_stop", lambda: itertools.islice([1, 2, 3], 1.5)),
    ("neg_step", lambda: itertools.islice([1, 2, 3], 0, 3, -1)),
    ("zero_step", lambda: itertools.islice([1, 2, 3], 0, 3, 0)),
]:
    try:
        list(fn())
    except ValueError:
        print(label, "ValueError")
